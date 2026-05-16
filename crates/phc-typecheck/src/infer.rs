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
            // Call return type. Three shapes need recovery so that
            // chained calls (`$x->m()->n()`) and locals fed by a
            // method call (`int $n = $xs->len();`) get propagated
            // types instead of falling to `Ty::Unknown`:
            //
            // 1. Free function: callee is `TypeName(name)` and the
            //    name resolves to a known function sig.
            // 2. Class method: callee is `Member { receiver, field }`
            //    and the receiver's type is `Path[ClassName]`. Look
            //    up the method's symbol id via the resolver's
            //    members_of and read its FunctionSig.
            // 3. Stdlib method: receiver typed `string`, `list<T>`,
            //    `result<T, E>`, or `option<T>`. Hardcoded surface
            //    matching D-025 / D-026 / D-027.
            let ret_ty = call_return_ty(callee, resolved, typed);
            record(typed, *span, ret_ty);
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
            // `$xs[i]` on a typed `list<T>` resolves to T. Other
            // receivers stay Unknown for now.
            let ty = index_return_ty(target, typed);
            record(typed, *span, ty);
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

/// Compute a Call expression's return type from its callee shape.
/// See the matching comment at the call site for the three cases.
fn call_return_ty(callee: &Expr, resolved: &Resolved, typed: &Typed) -> Ty {
    // D-024 stored-lambda call: callee's static type is `fn(...): R`,
    // lowered to `Ty::Path { path:["fn"], args:[R, P1, ..., Pn] }`.
    // Return type is args[0]. Checked before the syntactic dispatch
    // so a `Var` / `Member` callee typed as `fn` routes correctly.
    if let Some(Ty::Path { path, args, .. }) = typed.expr_types.get(&span_of(callee)) {
        if path.len() == 1 && path[0] == "fn" {
            if let Some(ret) = args.first() {
                return ret.clone();
            }
        }
    }
    match callee {
        Expr::TypeName { name, .. } => resolved
            .top_level
            .get(&name.name)
            .copied()
            .and_then(|sid| typed.function_sigs.get(&sid))
            .map(|sig| sig.return_ty.clone())
            .unwrap_or(Ty::Unknown),
        Expr::Member {
            receiver, field, ..
        } => {
            let recv_ty = match typed.expr_types.get(&span_of(receiver)) {
                Some(t) => t,
                None => return Ty::Unknown,
            };
            // 1. Stdlib method on a primitive / generic container.
            if let Some(t) = stdlib_method_return_ty(recv_ty, &field.name) {
                return t;
            }
            // 2. Class method: walk the resolver's members for the
            //    receiver's class and match by name.
            if let Ty::Path { path, .. } = recv_ty {
                if path.len() == 1 {
                    if let Some(class_id) = resolved.top_level.get(&path[0]).copied() {
                        if let Some(members) = resolved.members_of.get(&class_id) {
                            for &mid in members {
                                if resolved.symbol(mid).name == field.name {
                                    if let Some(sig) = typed.function_sigs.get(&mid) {
                                        return sig.return_ty.clone();
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Ty::Unknown
        }
        _ => Ty::Unknown,
    }
}

/// Map a stdlib method to its return type given the receiver's
/// static type. Mirrors the codegen / interpreter dispatch tables
/// for D-025 (string), D-026 (result/option), D-027 (list).
fn stdlib_method_return_ty(recv_ty: &Ty, method: &str) -> Option<Ty> {
    match recv_ty {
        Ty::Primitive(crate::Primitive::String) => match method {
            "len" => Some(Ty::Primitive(crate::Primitive::Int)),
            "contains" | "startsWith" | "endsWith" => Some(Ty::Primitive(crate::Primitive::Bool)),
            "trim" | "upper" | "lower" => Some(Ty::Primitive(crate::Primitive::String)),
            // toInt returns result<int, parseError>; precise E type
            // is stdlib-pending so leave Unknown rather than fake it.
            _ => None,
        },
        Ty::Path { path, args, .. } if path.len() == 1 => {
            let elem = args.first().cloned();
            match (path[0].as_str(), method) {
                ("list", "len") => Some(Ty::Primitive(crate::Primitive::Int)),
                ("list", "push") => Some(Ty::Primitive(crate::Primitive::Void)),
                ("list", "at") => elem,
                ("result", "isOk") | ("result", "isErr") => {
                    Some(Ty::Primitive(crate::Primitive::Bool))
                }
                ("result", "unwrapOr") => elem,
                ("option", "isSome") | ("option", "isNone") => {
                    Some(Ty::Primitive(crate::Primitive::Bool))
                }
                ("option", "unwrapOr") => elem,
                ("option", "orElse") => Some(recv_ty.clone()),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Compute the result type of `target[index]`. For a `list<T>`
/// target, the result is T; otherwise Unknown.
fn index_return_ty(target: &Expr, typed: &Typed) -> Ty {
    let target_ty = match typed.expr_types.get(&span_of(target)) {
        Some(t) => t,
        None => return Ty::Unknown,
    };
    if let Ty::Path { path, args, .. } = target_ty {
        if path.len() == 1 && path[0] == "list" {
            return args.first().cloned().unwrap_or(Ty::Unknown);
        }
    }
    Ty::Unknown
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
