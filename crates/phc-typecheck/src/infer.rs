// SPDX-License-Identifier: MIT
//! Expression-type inference for free-function bodies.
//!
//! First slice: literals, variables (via the resolver's use map),
//! and binary operators. Match expressions, lambdas, and class
//! membership land in follow-up commits as the binding-type table
//! grows to cover them.

use phc_ast::{BinOp, Block, Expr, Item, LocalBinding, SourceFile, Stmt, StrPart};
use phc_semantic::{Resolved, SymbolId};
use phc_span::Span;
use std::collections::HashMap;

use crate::lower::lower_type_ref;
use crate::{Primitive, Ty, Typed};

/// Binding-type table populated as the inferer walks declarations
/// and statements. Keys are [`SymbolId`]s assigned by the resolver.
pub(crate) type BindingTypes = HashMap<SymbolId, Ty>;

pub fn infer_function_bodies(file: &SourceFile, resolved: &Resolved, typed: &mut Typed) {
    for item in &file.items {
        if let Item::Function(f) = item {
            let id = match resolved.top_level.get(&f.name.name).copied() {
                Some(id) => id,
                None => continue,
            };
            let mut bindings = BindingTypes::default();
            // Seed with parameter types from the collected sig.
            if let Some(sig) = typed.function_sigs.get(&id) {
                for (param_ast, param_sig) in f.params.iter().zip(sig.params.iter()) {
                    // Look up the parameter's SymbolId via the resolver:
                    // every $param has a use-site at its declaration
                    // span. The resolver registered the SymbolId for
                    // the parameter at the function's body entry.
                    if let Some(sid) = symbol_at_def(resolved, param_ast.name.span) {
                        bindings.insert(sid, param_sig.ty.clone());
                    }
                }
            }
            walk_block(&f.body, resolved, &mut bindings, typed);
        }
    }
}

/// Find the SymbolId whose def_span starts at `span.lo`. Used to
/// link a parameter or local declaration's name span back to the
/// resolver's symbol table without needing a separate def-by-span
/// index — the resolver already keys symbols by their introducing
/// identifier span.
fn symbol_at_def(resolved: &Resolved, span: Span) -> Option<SymbolId> {
    resolved
        .symbols
        .iter()
        .find(|s| s.def_span == span)
        .map(|s| s.id)
}

fn walk_block(block: &Block, resolved: &Resolved, bindings: &mut BindingTypes, typed: &mut Typed) {
    for stmt in &block.statements {
        walk_stmt(stmt, resolved, bindings, typed);
    }
}

fn walk_stmt(stmt: &Stmt, resolved: &Resolved, bindings: &mut BindingTypes, typed: &mut Typed) {
    match stmt {
        Stmt::Local(b) => walk_local(b, resolved, bindings, typed),
        Stmt::Reassign(r) => {
            walk_expr(&r.lhs, resolved, bindings, typed);
            walk_expr(&r.value, resolved, bindings, typed);
        }
        Stmt::MemberAssign(m) => {
            walk_expr(&m.lhs, resolved, bindings, typed);
            walk_expr(&m.value, resolved, bindings, typed);
        }
        Stmt::If(i) => {
            for (cond, blk) in &i.branches {
                walk_expr(cond, resolved, bindings, typed);
                walk_block(blk, resolved, bindings, typed);
            }
            if let Some(else_blk) = &i.else_block {
                walk_block(else_blk, resolved, bindings, typed);
            }
        }
        Stmt::While(w) => {
            walk_expr(&w.cond, resolved, bindings, typed);
            walk_block(&w.body, resolved, bindings, typed);
        }
        Stmt::For(f) => {
            walk_expr(&f.iter, resolved, bindings, typed);
            // The element's type is the iter's element shape; until
            // inference understands containers we just lower the
            // declared elem_ty.
            if let Some(sid) = symbol_at_def(resolved, f.elem_name.span) {
                bindings.insert(sid, lower_type_ref(&f.elem_ty));
            }
            walk_block(&f.body, resolved, bindings, typed);
        }
        Stmt::Return(r) => {
            if let Some(v) = &r.value {
                walk_expr(v, resolved, bindings, typed);
            }
        }
        Stmt::Break { .. } | Stmt::Continue { .. } => {}
        Stmt::Expr(e) => {
            walk_expr(&e.expr, resolved, bindings, typed);
        }
    }
}

fn walk_local(
    b: &LocalBinding,
    resolved: &Resolved,
    bindings: &mut BindingTypes,
    typed: &mut Typed,
) {
    walk_expr(&b.value, resolved, bindings, typed);
    let ty = lower_type_ref(&b.ty);
    if let Some(sid) = symbol_at_def(resolved, b.name.span) {
        bindings.insert(sid, ty);
    }
}

fn walk_expr(expr: &Expr, resolved: &Resolved, bindings: &mut BindingTypes, typed: &mut Typed) {
    let ty = match expr {
        Expr::IntLit { span, .. } => {
            record(typed, *span, Ty::Primitive(Primitive::Int));
            return;
        }
        Expr::FloatLit { span, .. } => {
            record(typed, *span, Ty::Primitive(Primitive::Float));
            return;
        }
        Expr::BoolLit { span, .. } => {
            record(typed, *span, Ty::Primitive(Primitive::Bool));
            return;
        }
        Expr::NullLit { span } => {
            // `null` is polymorphic over T?; without a target type
            // we leave it Unknown and let later passes refine.
            record(typed, *span, Ty::Unknown);
            return;
        }
        Expr::StrLit { parts, span } => {
            for part in parts {
                if let StrPart::Expr(inner) = part {
                    walk_expr(inner, resolved, bindings, typed);
                }
            }
            record(typed, *span, Ty::Primitive(Primitive::String));
            return;
        }
        Expr::Var { span, .. } | Expr::This { span } => {
            let ty = resolved
                .uses
                .get(span)
                .and_then(|sid| bindings.get(sid).cloned())
                .unwrap_or(Ty::Unknown);
            record(typed, *span, ty);
            return;
        }
        Expr::Paren { inner, .. } => {
            walk_expr(inner, resolved, bindings, typed);
            typed
                .expr_types
                .get(&span_of(inner))
                .cloned()
                .unwrap_or(Ty::Unknown)
        }
        Expr::Binary { op, lhs, rhs, span } => {
            walk_expr(lhs, resolved, bindings, typed);
            walk_expr(rhs, resolved, bindings, typed);
            infer_binary(*op, lhs, rhs, typed, *span)
        }
        Expr::Unary { operand, span, .. } => {
            walk_expr(operand, resolved, bindings, typed);
            // Unary !/-/await preserve their operand type for now.
            typed
                .expr_types
                .get(&span_of(operand))
                .cloned()
                .map(|t| (t, *span))
                .map(|(t, s)| {
                    record(typed, s, t);
                })
                .unwrap_or(());
            return;
        }
        Expr::Borrow { operand, span, .. }
        | Expr::Try {
            value: operand,
            span,
            ..
        } => {
            walk_expr(operand, resolved, bindings, typed);
            record(typed, *span, Ty::Unknown);
            return;
        }
        Expr::Cast { value, ty, span } => {
            walk_expr(value, resolved, bindings, typed);
            let lowered = lower_type_ref(ty);
            record(typed, *span, lowered);
            return;
        }
        Expr::Member { receiver, span, .. } => {
            walk_expr(receiver, resolved, bindings, typed);
            record(typed, *span, Ty::Unknown);
            return;
        }
        Expr::Static { ty, span, .. } => {
            walk_expr(ty, resolved, bindings, typed);
            record(typed, *span, Ty::Unknown);
            return;
        }
        Expr::Call {
            callee, args, span, ..
        } => {
            walk_expr(callee, resolved, bindings, typed);
            for a in args {
                walk_expr(a, resolved, bindings, typed);
            }
            record(typed, *span, Ty::Unknown);
            return;
        }
        Expr::Index {
            target,
            index,
            span,
            ..
        } => {
            walk_expr(target, resolved, bindings, typed);
            walk_expr(index, resolved, bindings, typed);
            record(typed, *span, Ty::Unknown);
            return;
        }
        Expr::TypeName { span, .. } => {
            record(typed, *span, Ty::Unknown);
            return;
        }
        Expr::Match {
            scrutinee,
            arms,
            span,
        } => {
            walk_expr(scrutinee, resolved, bindings, typed);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    walk_expr(g, resolved, bindings, typed);
                }
                walk_expr(&arm.body, resolved, bindings, typed);
            }
            record(typed, *span, Ty::Unknown);
            return;
        }
        Expr::Lambda { body, span, .. } => {
            match body {
                phc_ast::LambdaBody::Expr(e) => walk_expr(e, resolved, bindings, typed),
                phc_ast::LambdaBody::Block(b) => walk_block(b, resolved, bindings, typed),
            }
            record(typed, *span, Ty::Unknown);
            return;
        }
    };
    record(typed, span_of(expr), ty);
}

/// Comparison / equality / logical → bool (regardless of operand
/// types — operand-type checks land in the "compatibility" pass).
/// Arithmetic → if both operands are int, int; both float, float;
/// otherwise Unknown until a numeric coercion story lands.
fn infer_binary(op: BinOp, lhs: &Expr, rhs: &Expr, typed: &mut Typed, span: Span) -> Ty {
    use BinOp::*;
    match op {
        Lt | Le | Gt | Ge | Eq | Neq | And | Or => {
            let _ = (span, lhs, rhs);
            Ty::Primitive(Primitive::Bool)
        }
        Add | Sub | Mul | Div | Rem => {
            let lt = typed.expr_types.get(&span_of(lhs)).cloned();
            let rt = typed.expr_types.get(&span_of(rhs)).cloned();
            match (lt, rt) {
                (Some(Ty::Primitive(Primitive::Int)), Some(Ty::Primitive(Primitive::Int))) => {
                    Ty::Primitive(Primitive::Int)
                }
                (Some(Ty::Primitive(Primitive::Float)), Some(Ty::Primitive(Primitive::Float))) => {
                    Ty::Primitive(Primitive::Float)
                }
                _ => Ty::Unknown,
            }
        }
        NullCoalesce => Ty::Unknown,
    }
}

fn record(typed: &mut Typed, span: Span, ty: Ty) {
    typed.expr_types.insert(span, ty);
}

fn span_of(expr: &Expr) -> Span {
    match expr {
        Expr::IntLit { span, .. }
        | Expr::FloatLit { span, .. }
        | Expr::BoolLit { span, .. }
        | Expr::NullLit { span }
        | Expr::StrLit { span, .. }
        | Expr::This { span }
        | Expr::Var { span, .. }
        | Expr::TypeName { span, .. }
        | Expr::Paren { span, .. }
        | Expr::Member { span, .. }
        | Expr::Static { span, .. }
        | Expr::Call { span, .. }
        | Expr::Index { span, .. }
        | Expr::Try { span, .. }
        | Expr::Unary { span, .. }
        | Expr::Borrow { span, .. }
        | Expr::Cast { span, .. }
        | Expr::Binary { span, .. }
        | Expr::Match { span, .. }
        | Expr::Lambda { span, .. } => *span,
    }
}
