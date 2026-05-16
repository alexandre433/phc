// SPDX-License-Identifier: MIT
//! Pattern exhaustiveness check (D-015).
//!
//! Walks every `match` expression in the file and verifies the
//! arms cover every possible scrutinee value. Today's coverage:
//!
//! - Enum scrutinee: every declared variant must be matched, or a
//!   catch-all (Wildcard or Var) arm must appear. Or-patterns
//!   spread their atoms. Literal patterns over an enum count
//!   nothing (they cannot match an enum value at this layer).
//! - Non-enum scrutinee (int, string, ...): a catch-all is
//!   required since covering the full value set is impossible.
//!
//! Diagnostics surface on `Typed.diagnostics`.

use phc_ast::{ClassDecl, Expr, FunctionDecl, Item, MatchArm, Pattern, SourceFile, Stmt, StrPart};
use phc_errors::{Diagnostic, Severity};
use phc_semantic::{Resolved, SymbolKind};

use crate::{Ty, Typed};

pub fn check_exhaustiveness(file: &SourceFile, resolved: &Resolved, typed: &mut Typed) {
    let mut ctx = Ctx {
        file,
        resolved,
        diagnostics: Vec::new(),
    };
    for item in &file.items {
        match item {
            Item::Function(f) => walk_function(f, &mut ctx, typed),
            Item::Class(c) => walk_class(c, &mut ctx, typed),
            Item::Trait(t) => {
                for m in &t.methods {
                    walk_function(m, &mut ctx, typed);
                }
            }
            Item::Test(t) => walk_block(&t.body.statements, &mut ctx, typed),
            Item::Enum(_) | Item::Interface(_) => {}
        }
    }
    typed.diagnostics.extend(ctx.diagnostics);
}

struct Ctx<'a> {
    file: &'a SourceFile,
    resolved: &'a Resolved,
    diagnostics: Vec<Diagnostic>,
}

fn walk_function(f: &FunctionDecl, ctx: &mut Ctx<'_>, typed: &Typed) {
    walk_block(&f.body.statements, ctx, typed);
}

fn walk_class(c: &ClassDecl, ctx: &mut Ctx<'_>, typed: &Typed) {
    use phc_ast::ClassMember as CM;
    for member in &c.members {
        match member {
            CM::Construct(con) => walk_block(&con.body.statements, ctx, typed),
            CM::Method(m) => walk_function(m, ctx, typed),
            CM::Field(f) => {
                if let Some(default) = &f.default {
                    walk_expr(default, ctx, typed);
                }
                for hook in &f.hooks {
                    walk_hook(hook, ctx, typed);
                }
            }
            CM::TraitUse(_) => {}
        }
    }
}

fn walk_hook(hook: &phc_ast::PropertyHook, ctx: &mut Ctx<'_>, typed: &Typed) {
    use phc_ast::PropertyHook as H;
    match hook {
        H::GetExpr { expr, .. } => walk_expr(expr, ctx, typed),
        H::GetBlock { body, .. } => walk_block(&body.statements, ctx, typed),
        H::Set { body, .. } => walk_block(&body.statements, ctx, typed),
    }
}

fn walk_block(stmts: &[Stmt], ctx: &mut Ctx<'_>, typed: &Typed) {
    for stmt in stmts {
        walk_stmt(stmt, ctx, typed);
    }
}

fn walk_stmt(stmt: &Stmt, ctx: &mut Ctx<'_>, typed: &Typed) {
    match stmt {
        Stmt::Local(b) => walk_expr(&b.value, ctx, typed),
        Stmt::Reassign(r) => {
            walk_expr(&r.lhs, ctx, typed);
            walk_expr(&r.value, ctx, typed);
        }
        Stmt::MemberAssign(m) => {
            walk_expr(&m.lhs, ctx, typed);
            walk_expr(&m.value, ctx, typed);
        }
        Stmt::If(i) => {
            for (cond, blk) in &i.branches {
                walk_expr(cond, ctx, typed);
                walk_block(&blk.statements, ctx, typed);
            }
            if let Some(else_blk) = &i.else_block {
                walk_block(&else_blk.statements, ctx, typed);
            }
        }
        Stmt::While(w) => {
            walk_expr(&w.cond, ctx, typed);
            walk_block(&w.body.statements, ctx, typed);
        }
        Stmt::For(f) => {
            walk_expr(&f.iter, ctx, typed);
            walk_block(&f.body.statements, ctx, typed);
        }
        Stmt::Return(r) => {
            if let Some(v) = &r.value {
                walk_expr(v, ctx, typed);
            }
        }
        Stmt::Break { .. } | Stmt::Continue { .. } => {}
        Stmt::Expr(e) => walk_expr(&e.expr, ctx, typed),
    }
}

fn walk_expr(expr: &Expr, ctx: &mut Ctx<'_>, typed: &Typed) {
    match expr {
        Expr::Match {
            scrutinee,
            arms,
            span,
        } => {
            walk_expr(scrutinee, ctx, typed);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    walk_expr(g, ctx, typed);
                }
                walk_expr(&arm.body, ctx, typed);
            }
            check_arms(scrutinee, arms, *span, ctx, typed);
        }
        Expr::IntLit { .. }
        | Expr::FloatLit { .. }
        | Expr::BoolLit { .. }
        | Expr::NullLit { .. }
        | Expr::This { .. }
        | Expr::Var { .. }
        | Expr::TypeName { .. } => {}
        Expr::StrLit { parts, .. } => {
            for p in parts {
                if let StrPart::Expr(e) = p {
                    walk_expr(e, ctx, typed);
                }
            }
        }
        Expr::Paren { inner, .. } => walk_expr(inner, ctx, typed),
        Expr::Member { receiver, .. } => walk_expr(receiver, ctx, typed),
        Expr::Static { ty, .. } => walk_expr(ty, ctx, typed),
        Expr::Call { callee, args, .. } => {
            walk_expr(callee, ctx, typed);
            for a in args {
                walk_expr(a, ctx, typed);
            }
        }
        Expr::Index { target, index, .. } => {
            walk_expr(target, ctx, typed);
            walk_expr(index, ctx, typed);
        }
        Expr::Try { value, .. } => walk_expr(value, ctx, typed),
        Expr::Unary { operand, .. } | Expr::Borrow { operand, .. } => {
            walk_expr(operand, ctx, typed);
        }
        Expr::Cast { value, .. } => walk_expr(value, ctx, typed),
        Expr::Binary { lhs, rhs, .. } => {
            walk_expr(lhs, ctx, typed);
            walk_expr(rhs, ctx, typed);
        }
        Expr::Lambda { body, .. } => match body {
            phc_ast::LambdaBody::Expr(e) => walk_expr(e, ctx, typed),
            phc_ast::LambdaBody::Block(b) => walk_block(&b.statements, ctx, typed),
        },
    }
}

fn check_arms(
    scrutinee: &Expr,
    arms: &[MatchArm],
    match_span: phc_span::Span,
    ctx: &mut Ctx<'_>,
    typed: &Typed,
) {
    let scrut_ty = typed.expr_types.get(&expr_span(scrutinee));
    let enum_name = match scrut_ty {
        Some(Ty::Path { path, .. }) if path.len() == 1 => {
            let name = &path[0];
            let kind = ctx
                .resolved
                .top_level
                .get(name)
                .map(|id| ctx.resolved.symbol(*id).kind);
            if kind == Some(SymbolKind::Enum) {
                Some(name.clone())
            } else {
                None
            }
        }
        _ => None,
    };
    let has_catchall = arms.iter().any(|arm| {
        // No guard means the arm is genuinely catch-all when its
        // pattern always matches; a guarded catch-all does not
        // satisfy exhaustiveness.
        arm.guard.is_none() && pattern_is_catchall(&arm.pattern)
    });
    match enum_name {
        Some(name) => {
            if has_catchall {
                return;
            }
            let Some(variants) = enum_variants(ctx.file, &name) else {
                return;
            };
            let mut covered = std::collections::BTreeSet::new();
            for arm in arms {
                if arm.guard.is_some() {
                    continue;
                }
                collect_enum_variants(&arm.pattern, &name, &mut covered);
            }
            let missing: Vec<&str> = variants
                .iter()
                .filter(|v| !covered.contains(v.as_str()))
                .map(|s| s.as_str())
                .collect();
            if !missing.is_empty() {
                ctx.diagnostics.push(Diagnostic {
                    severity: Severity::Error,
                    message: format!(
                        "non-exhaustive `match` on enum `{}`: missing {}",
                        name,
                        missing
                            .iter()
                            .map(|m| format!("`{}::{}`", name, m))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    span: match_span,
                });
            }
        }
        None => {
            if !has_catchall {
                ctx.diagnostics.push(Diagnostic {
                    severity: Severity::Error,
                    message: "non-exhaustive `match`: a `_` wildcard arm is required for non-enum scrutinees".to_string(),
                    span: match_span,
                });
            }
        }
    }
}

fn pattern_is_catchall(pat: &Pattern) -> bool {
    match pat {
        Pattern::Wildcard { .. } | Pattern::Var { .. } => true,
        Pattern::Or { atoms, .. } => atoms.iter().any(pattern_is_catchall),
        _ => false,
    }
}

fn collect_enum_variants(
    pat: &Pattern,
    enum_name: &str,
    out: &mut std::collections::BTreeSet<String>,
) {
    match pat {
        Pattern::EnumVariant { ty, variant, .. } => {
            if ty.name == enum_name {
                out.insert(variant.name.clone());
            }
        }
        Pattern::Or { atoms, .. } => {
            for a in atoms {
                collect_enum_variants(a, enum_name, out);
            }
        }
        _ => {}
    }
}

fn enum_variants(file: &SourceFile, name: &str) -> Option<Vec<String>> {
    file.items.iter().find_map(|item| match item {
        Item::Enum(e) if e.name.name == name => {
            Some(e.variants.iter().map(|v| v.name.name.clone()).collect())
        }
        _ => None,
    })
}

fn expr_span(expr: &Expr) -> phc_span::Span {
    use Expr::*;
    match expr {
        IntLit { span, .. }
        | FloatLit { span, .. }
        | BoolLit { span, .. }
        | NullLit { span }
        | StrLit { span, .. }
        | This { span }
        | Var { span, .. }
        | TypeName { span, .. }
        | Paren { span, .. }
        | Member { span, .. }
        | Static { span, .. }
        | Call { span, .. }
        | Index { span, .. }
        | Try { span, .. }
        | Unary { span, .. }
        | Borrow { span, .. }
        | Cast { span, .. }
        | Binary { span, .. }
        | Match { span, .. }
        | Lambda { span, .. } => *span,
    }
}
