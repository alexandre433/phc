// SPDX-License-Identifier: MIT
//! Static linter for PHC (D-036 v0a + D-036b).
//!
//! v0a ruleset (Phase 8 starter):
//! - `unused_local`: an `int $x = ...;` local that no expression
//!   references. Surfaces as a warning so users can suppress by
//!   renaming the binding `$_` (or `$_unused` etc. — any name
//!   starting with `_` is treated as deliberately ignored).
//! - `unreachable_after_return`: any statement that follows a
//!   `return` in the same block. Warning per offending stmt.
//! - `class_naming`: class / enum / interface / trait names should
//!   be PascalCase (D-006a's "user types are PascalCase" rule).
//!
//! D-036b additions:
//! - `shadow_local`: a `let` binding whose name matches a binding
//!   visible in any enclosing scope of the same function. Lambda
//!   bodies start a fresh scope chain. Params seed the outermost
//!   function scope.
//! - `dead_branch`: an `if` whose leading condition is a literal
//!   `true` or `false`, making one branch statically unreachable.
//!   `else if` literal-bool conditions are a Phase 8 follow-up.
//!
//! All rules emit `Severity::Warning` so existing tooling that
//! runs the linter does not regress on a clean source file merely
//! because a style nit is present. The CLI's `phc lint` command
//! exits non-zero only when at least one warning surfaces, so CI
//! scripts can still gate on cleanliness.
//!
//! Out of scope (tracked as Phase 8 follow-ups):
//! - `else if` literal-bool conditions in `dead_branch`.
//! - Naming for functions / methods / fields / locals — depends
//!   on a project-wide style decision the user hasn't picked yet.
//! - Suggestion / autofix surfaces.
//! - Per-rule configuration / suppression attributes.

#[cfg(test)]
mod tests;

use phc_ast::{
    Block, ClassDecl, ClassMember, ConstructDecl, EnumDecl, Expr, FunctionDecl, Item, LambdaBody,
    MatchArm, Param, Pattern, SourceFile, Stmt, StrPart, TraitDecl,
};
use phc_errors::{Diagnostic, Severity};
use phc_parser::parse;
use phc_semantic::{resolve, Resolved, SymbolId};
use phc_span::{FileId, Span};
use std::collections::{HashMap, HashSet};

/// Aggregate result. `setup_errors` is parse / resolve errors;
/// `warnings` is the lint findings themselves.
#[derive(Debug, Default)]
pub struct LintReport {
    pub setup_errors: Vec<Diagnostic>,
    pub warnings: Vec<Diagnostic>,
}

impl LintReport {
    pub fn ok(&self) -> bool {
        self.setup_errors.is_empty() && self.warnings.is_empty()
    }
}

/// Run the v0a ruleset over `source`. Parse / resolve failures
/// abort the lint and surface as `setup_errors`; otherwise every
/// rule runs and warnings accumulate in declaration order.
pub fn lint_source(source: &str) -> LintReport {
    let mut report = LintReport::default();
    let parsed = parse(source, FileId(0));
    report.setup_errors.extend(parsed.diagnostics.clone());
    let Some(file) = parsed.file else {
        return report;
    };
    let resolved = resolve(&file);
    report.setup_errors.extend(resolved.diagnostics.clone());
    if !report.setup_errors.is_empty() {
        return report;
    }
    check_class_naming(&file, &mut report.warnings);
    check_unreachable_after_return(&file, &mut report.warnings);
    check_unused_locals(&file, &resolved, &mut report.warnings);
    check_shadow_local(&file, &mut report.warnings);
    check_dead_branch(&file, &mut report.warnings);
    report
}

// ===== class_naming =====

fn check_class_naming(file: &SourceFile, out: &mut Vec<Diagnostic>) {
    for item in &file.items {
        match item {
            Item::Class(c) => warn_pascal(&c.name.name, c.name.span, "class", out),
            Item::Enum(e) => warn_pascal(&e.name.name, e.name.span, "enum", out),
            Item::Interface(i) => warn_pascal(&i.name.name, i.name.span, "interface", out),
            Item::Trait(t) => warn_pascal(&t.name.name, t.name.span, "trait", out),
            _ => {}
        }
    }
}

fn warn_pascal(name: &str, span: Span, kind: &str, out: &mut Vec<Diagnostic>) {
    if !is_pascal_case(name) {
        out.push(Diagnostic {
            severity: Severity::Warning,
            message: format!(
                "{kind} name `{name}` should be PascalCase (D-006a: user types are PascalCase)"
            ),
            span,
        });
    }
}

fn is_pascal_case(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_uppercase() {
        return false;
    }
    // Allow ASCII letters / digits / underscore in the tail. No
    // consecutive uppercase letters rule — `HTTPClient` reads as
    // PascalCase to most users.
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

// ===== unreachable_after_return =====

fn check_unreachable_after_return(file: &SourceFile, out: &mut Vec<Diagnostic>) {
    for item in &file.items {
        match item {
            Item::Function(f) => walk_block_for_unreachable(&f.body, out),
            Item::Class(c) => {
                for m in &c.members {
                    match m {
                        ClassMember::Method(f) => walk_block_for_unreachable(&f.body, out),
                        ClassMember::Construct(con) => walk_block_for_unreachable(&con.body, out),
                        _ => {}
                    }
                }
            }
            Item::Trait(t) => {
                for m in &t.methods {
                    walk_block_for_unreachable(&m.body, out);
                }
            }
            Item::Test(t) => walk_block_for_unreachable(&t.body, out),
            _ => {}
        }
    }
}

fn walk_block_for_unreachable(block: &Block, out: &mut Vec<Diagnostic>) {
    let mut seen_return = false;
    for stmt in &block.statements {
        if seen_return {
            out.push(Diagnostic {
                severity: Severity::Warning,
                message: "unreachable statement after `return`".to_string(),
                span: stmt_span(stmt),
            });
            continue;
        }
        if matches!(stmt, Stmt::Return(_)) {
            seen_return = true;
        }
        // Recurse into nested blocks regardless — each block has
        // its own reachability frame.
        walk_stmt_for_unreachable(stmt, out);
    }
}

fn walk_stmt_for_unreachable(stmt: &Stmt, out: &mut Vec<Diagnostic>) {
    match stmt {
        Stmt::If(i) => {
            for (_cond, blk) in &i.branches {
                walk_block_for_unreachable(blk, out);
            }
            if let Some(else_blk) = &i.else_block {
                walk_block_for_unreachable(else_blk, out);
            }
        }
        Stmt::While(w) => walk_block_for_unreachable(&w.body, out),
        Stmt::For(f) => walk_block_for_unreachable(&f.body, out),
        Stmt::Expr(e) => walk_expr_for_unreachable(&e.expr, out),
        Stmt::Local(b) => walk_expr_for_unreachable(&b.value, out),
        Stmt::Reassign(r) => walk_expr_for_unreachable(&r.value, out),
        Stmt::MemberAssign(m) => walk_expr_for_unreachable(&m.value, out),
        Stmt::Return(r) => {
            if let Some(v) = &r.value {
                walk_expr_for_unreachable(v, out);
            }
        }
        Stmt::Break { .. } | Stmt::Continue { .. } => {}
    }
}

fn walk_expr_for_unreachable(expr: &Expr, out: &mut Vec<Diagnostic>) {
    if let Expr::Lambda { body, .. } = expr {
        match body {
            LambdaBody::Expr(_) => {}
            LambdaBody::Block(b) => walk_block_for_unreachable(b, out),
        }
    }
    walk_expr_descend(expr, &mut |e| walk_expr_for_unreachable(e, out));
}

// ===== unused_local =====

fn check_unused_locals(file: &SourceFile, resolved: &Resolved, out: &mut Vec<Diagnostic>) {
    let mut decls: HashMap<SymbolId, (String, Span)> = HashMap::new();
    let mut uses: HashSet<SymbolId> = HashSet::new();
    for item in &file.items {
        match item {
            Item::Function(f) => collect_locals(&f.body, resolved, &mut decls, &mut uses),
            Item::Class(c) => {
                for m in &c.members {
                    match m {
                        ClassMember::Method(f) => {
                            collect_locals(&f.body, resolved, &mut decls, &mut uses)
                        }
                        ClassMember::Construct(con) => {
                            collect_locals(&con.body, resolved, &mut decls, &mut uses)
                        }
                        _ => {}
                    }
                }
            }
            Item::Trait(t) => {
                for m in &t.methods {
                    collect_locals(&m.body, resolved, &mut decls, &mut uses);
                }
            }
            Item::Test(t) => collect_locals(&t.body, resolved, &mut decls, &mut uses),
            _ => {}
        }
    }
    for (sid, (name, span)) in &decls {
        if uses.contains(sid) {
            continue;
        }
        if name.starts_with('_') {
            continue;
        }
        out.push(Diagnostic {
            severity: Severity::Warning,
            message: format!("unused local binding `${name}` — rename to `$_{name}` to silence"),
            span: *span,
        });
    }
}

fn collect_locals(
    block: &Block,
    resolved: &Resolved,
    decls: &mut HashMap<SymbolId, (String, Span)>,
    uses: &mut HashSet<SymbolId>,
) {
    for stmt in &block.statements {
        collect_locals_stmt(stmt, resolved, decls, uses);
    }
}

fn collect_locals_stmt(
    stmt: &Stmt,
    resolved: &Resolved,
    decls: &mut HashMap<SymbolId, (String, Span)>,
    uses: &mut HashSet<SymbolId>,
) {
    match stmt {
        Stmt::Local(b) => {
            if let Some(sid) = symbol_at_def(resolved, b.name.span) {
                decls.insert(sid, (b.name.name.clone(), b.name.span));
            }
            collect_locals_expr(&b.value, resolved, decls, uses);
        }
        Stmt::Reassign(r) => {
            collect_locals_expr(&r.lhs, resolved, decls, uses);
            collect_locals_expr(&r.value, resolved, decls, uses);
        }
        Stmt::MemberAssign(m) => {
            collect_locals_expr(&m.lhs, resolved, decls, uses);
            collect_locals_expr(&m.value, resolved, decls, uses);
        }
        Stmt::Expr(e) => collect_locals_expr(&e.expr, resolved, decls, uses),
        Stmt::Return(r) => {
            if let Some(v) = &r.value {
                collect_locals_expr(v, resolved, decls, uses);
            }
        }
        Stmt::If(i) => {
            for (cond, blk) in &i.branches {
                collect_locals_expr(cond, resolved, decls, uses);
                collect_locals(blk, resolved, decls, uses);
            }
            if let Some(else_blk) = &i.else_block {
                collect_locals(else_blk, resolved, decls, uses);
            }
        }
        Stmt::While(w) => {
            collect_locals_expr(&w.cond, resolved, decls, uses);
            collect_locals(&w.body, resolved, decls, uses);
        }
        Stmt::For(f) => {
            // The `for` elem-name is itself a local binding; count
            // its uses inside the body before flagging.
            if let Some(sid) = symbol_at_def(resolved, f.elem_name.span) {
                decls.insert(sid, (f.elem_name.name.clone(), f.elem_name.span));
            }
            collect_locals_expr(&f.iter, resolved, decls, uses);
            collect_locals(&f.body, resolved, decls, uses);
        }
        Stmt::Break { .. } | Stmt::Continue { .. } => {}
    }
}

fn collect_locals_expr(
    expr: &Expr,
    resolved: &Resolved,
    decls: &mut HashMap<SymbolId, (String, Span)>,
    uses: &mut HashSet<SymbolId>,
) {
    match expr {
        Expr::Var { span, .. } | Expr::This { span } => {
            if let Some(&sid) = resolved.uses.get(span) {
                uses.insert(sid);
            }
        }
        Expr::Lambda { params, body, .. } => {
            // Lambda params are locals too — count uses inside the
            // body before flagging. The interp / typecheck already
            // track them via the resolver.
            for p in params {
                if let Some(sid) = symbol_at_def(resolved, p.name.span) {
                    decls.insert(sid, (p.name.name.clone(), p.name.span));
                }
            }
            match body {
                LambdaBody::Expr(e) => collect_locals_expr(e, resolved, decls, uses),
                LambdaBody::Block(b) => collect_locals(b, resolved, decls, uses),
            }
        }
        _ => walk_expr_descend(expr, &mut |e| collect_locals_expr(e, resolved, decls, uses)),
    }
}

// ===== shadow_local =====

fn check_shadow_local(file: &SourceFile, out: &mut Vec<Diagnostic>) {
    for item in &file.items {
        match item {
            Item::Function(f) => {
                let mut scopes: Vec<HashSet<String>> = vec![HashSet::new()];
                seed_params(f.params.as_slice(), &mut scopes);
                shadow_walk_block(&f.body, &mut scopes, out);
            }
            Item::Class(c) => {
                for m in &c.members {
                    match m {
                        ClassMember::Method(f) => {
                            let mut scopes: Vec<HashSet<String>> = vec![HashSet::new()];
                            seed_params(f.params.as_slice(), &mut scopes);
                            shadow_walk_block(&f.body, &mut scopes, out);
                        }
                        ClassMember::Construct(con) => {
                            let mut scopes: Vec<HashSet<String>> = vec![HashSet::new()];
                            seed_construct_params(con.params.as_slice(), &mut scopes);
                            shadow_walk_block(&con.body, &mut scopes, out);
                        }
                        _ => {}
                    }
                }
            }
            Item::Trait(t) => {
                for m in &t.methods {
                    let mut scopes: Vec<HashSet<String>> = vec![HashSet::new()];
                    seed_params(m.params.as_slice(), &mut scopes);
                    shadow_walk_block(&m.body, &mut scopes, out);
                }
            }
            Item::Test(t) => {
                let mut scopes: Vec<HashSet<String>> = vec![HashSet::new()];
                shadow_walk_block(&t.body, &mut scopes, out);
            }
            _ => {}
        }
    }
}

/// Insert function param names into the outermost scope so that a
/// `let $param = ...;` inside the body fires a shadow warning.
fn seed_params(params: &[Param], scopes: &mut [HashSet<String>]) {
    if let Some(top) = scopes.last_mut() {
        for p in params {
            top.insert(p.name.name.clone());
        }
    }
}

/// Same for construct params (which may have `$` promotion syntax).
fn seed_construct_params(params: &[phc_ast::ConstructParam], scopes: &mut [HashSet<String>]) {
    if let Some(top) = scopes.last_mut() {
        for p in params {
            top.insert(p.name.name.clone());
        }
    }
}

/// Returns `true` if `name` exists in any scope strictly outer than
/// the current (innermost) one.
fn name_in_outer_scopes(name: &str, scopes: &[HashSet<String>]) -> bool {
    let outer = scopes.len().saturating_sub(1);
    scopes[..outer].iter().any(|s| s.contains(name))
}

fn shadow_walk_block(block: &Block, scopes: &mut Vec<HashSet<String>>, out: &mut Vec<Diagnostic>) {
    scopes.push(HashSet::new());
    for stmt in &block.statements {
        shadow_walk_stmt(stmt, scopes, out);
    }
    scopes.pop();
}

fn shadow_walk_stmt(stmt: &Stmt, scopes: &mut Vec<HashSet<String>>, out: &mut Vec<Diagnostic>) {
    match stmt {
        Stmt::Local(b) => {
            // Check before inserting — only fires for outer scopes.
            if name_in_outer_scopes(&b.name.name, scopes) {
                out.push(Diagnostic {
                    severity: Severity::Warning,
                    message: format!(
                        "local '${name}' shadows an outer binding",
                        name = b.name.name
                    ),
                    span: b.span,
                });
            }
            // Recurse into initialiser before inserting the name so
            // `int $x = $x + 1;` doesn't trigger self-shadow.
            shadow_walk_expr(&b.value, scopes, out);
            if let Some(top) = scopes.last_mut() {
                top.insert(b.name.name.clone());
            }
        }
        Stmt::If(i) => {
            for (cond, blk) in &i.branches {
                shadow_walk_expr(cond, scopes, out);
                shadow_walk_block(blk, scopes, out);
            }
            if let Some(else_blk) = &i.else_block {
                shadow_walk_block(else_blk, scopes, out);
            }
        }
        Stmt::While(w) => {
            shadow_walk_expr(&w.cond, scopes, out);
            shadow_walk_block(&w.body, scopes, out);
        }
        Stmt::For(f) => {
            shadow_walk_expr(&f.iter, scopes, out);
            // Push a scope that includes the elem binding before
            // walking the body — mirrors how the runtime scopes it.
            scopes.push(HashSet::new());
            if let Some(top) = scopes.last_mut() {
                top.insert(f.elem_name.name.clone());
            }
            for s in &f.body.statements {
                shadow_walk_stmt(s, scopes, out);
            }
            scopes.pop();
        }
        Stmt::Expr(e) => shadow_walk_expr(&e.expr, scopes, out),
        Stmt::Return(r) => {
            if let Some(v) = &r.value {
                shadow_walk_expr(v, scopes, out);
            }
        }
        Stmt::Reassign(r) => {
            shadow_walk_expr(&r.lhs, scopes, out);
            shadow_walk_expr(&r.value, scopes, out);
        }
        Stmt::MemberAssign(m) => {
            shadow_walk_expr(&m.lhs, scopes, out);
            shadow_walk_expr(&m.value, scopes, out);
        }
        Stmt::Break { .. } | Stmt::Continue { .. } => {}
    }
}

#[allow(clippy::only_used_in_recursion)]
fn shadow_walk_expr(expr: &Expr, scopes: &mut Vec<HashSet<String>>, out: &mut Vec<Diagnostic>) {
    match expr {
        Expr::Lambda { params, body, .. } => {
            // Lambda = fresh scope root; outer locals don't shadow here.
            let mut lambda_scopes: Vec<HashSet<String>> = vec![HashSet::new()];
            seed_params(params, &mut lambda_scopes);
            match body {
                LambdaBody::Expr(e) => shadow_walk_expr(e, &mut lambda_scopes, out),
                LambdaBody::Block(b) => shadow_walk_block(b, &mut lambda_scopes, out),
            }
        }
        _ => walk_expr_descend(expr, &mut |e| shadow_walk_expr(e, scopes, out)),
    }
}

// ===== dead_branch =====

fn check_dead_branch(file: &SourceFile, out: &mut Vec<Diagnostic>) {
    for item in &file.items {
        match item {
            Item::Function(f) => dead_walk_block(&f.body, out),
            Item::Class(c) => {
                for m in &c.members {
                    match m {
                        ClassMember::Method(f) => dead_walk_block(&f.body, out),
                        ClassMember::Construct(con) => dead_walk_block(&con.body, out),
                        _ => {}
                    }
                }
            }
            Item::Trait(t) => {
                for m in &t.methods {
                    dead_walk_block(&m.body, out);
                }
            }
            Item::Test(t) => dead_walk_block(&t.body, out),
            _ => {}
        }
    }
}

fn dead_walk_block(block: &Block, out: &mut Vec<Diagnostic>) {
    for stmt in &block.statements {
        dead_walk_stmt(stmt, out);
    }
}

fn dead_walk_stmt(stmt: &Stmt, out: &mut Vec<Diagnostic>) {
    if let Stmt::If(i) = stmt {
        // Only check the leading `if` condition (branches[0]).
        // Literal-bool `else if` conditions are a Phase 8 follow-up.
        if let Some((Expr::BoolLit { value, .. }, _blk)) = i.branches.first() {
            let msg = if *value {
                if i.else_block.is_some() {
                    "else branch is unreachable (condition is always true)".to_string()
                } else {
                    "if condition is always true; consider removing the branch".to_string()
                }
            } else if i.else_block.is_some() {
                "then branch is unreachable (condition is always false)".to_string()
            } else {
                "if condition is always false; then branch never runs".to_string()
            };
            out.push(Diagnostic {
                severity: Severity::Warning,
                message: msg,
                span: i.span,
            });
        }
        // Recurse into all branches regardless.
        for (_cond, blk) in &i.branches {
            dead_walk_block(blk, out);
        }
        if let Some(else_blk) = &i.else_block {
            dead_walk_block(else_blk, out);
        }
        return;
    }
    // Recurse into other statement types.
    match stmt {
        Stmt::While(w) => dead_walk_block(&w.body, out),
        Stmt::For(f) => dead_walk_block(&f.body, out),
        Stmt::Expr(e) => dead_walk_expr(&e.expr, out),
        Stmt::Local(b) => dead_walk_expr(&b.value, out),
        Stmt::Reassign(r) => dead_walk_expr(&r.value, out),
        Stmt::MemberAssign(m) => dead_walk_expr(&m.value, out),
        Stmt::Return(r) => {
            if let Some(v) = &r.value {
                dead_walk_expr(v, out);
            }
        }
        Stmt::Break { .. } | Stmt::Continue { .. } => {}
        Stmt::If(_) => unreachable!(), // handled above
    }
}

fn dead_walk_expr(expr: &Expr, out: &mut Vec<Diagnostic>) {
    if let Expr::Lambda { body, .. } = expr {
        match body {
            LambdaBody::Expr(_) => {}
            LambdaBody::Block(b) => dead_walk_block(b, out),
        }
    }
    walk_expr_descend(expr, &mut |e| dead_walk_expr(e, out));
}

// ===== helpers =====

fn symbol_at_def(resolved: &Resolved, span: Span) -> Option<SymbolId> {
    resolved
        .symbols
        .iter()
        .find(|s| s.def_span == span)
        .map(|s| s.id)
}

fn stmt_span(stmt: &Stmt) -> Span {
    match stmt {
        Stmt::Local(b) => b.span,
        Stmt::Reassign(r) => r.span,
        Stmt::MemberAssign(m) => m.span,
        Stmt::If(i) => i.span,
        Stmt::While(w) => w.span,
        Stmt::For(f) => f.span,
        Stmt::Return(r) => r.span,
        Stmt::Break { span } | Stmt::Continue { span } => *span,
        Stmt::Expr(e) => e.span,
    }
}

/// Descend a single level into an expression, calling `f` on each
/// immediate sub-expression. Used by every rule's expression
/// walker so the tree is traversed exactly once per rule.
fn walk_expr_descend<F>(expr: &Expr, f: &mut F)
where
    F: FnMut(&Expr),
{
    match expr {
        Expr::Paren { inner, .. } => f(inner),
        Expr::Unary { operand, .. } | Expr::Borrow { operand, .. } => f(operand),
        Expr::Try { value, .. } | Expr::Cast { value, .. } => f(value),
        Expr::Member { receiver, .. } => f(receiver),
        Expr::Static { ty, .. } => f(ty),
        Expr::Binary { lhs, rhs, .. } => {
            f(lhs);
            f(rhs);
        }
        Expr::Call { callee, args, .. } => {
            f(callee);
            for a in args {
                f(a);
            }
        }
        Expr::Index { target, index, .. } => {
            f(target);
            f(index);
        }
        Expr::StrLit { parts, .. } => {
            for p in parts {
                if let StrPart::Expr(e) = p {
                    f(e);
                }
            }
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            f(scrutinee);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    f(g);
                }
                f(&arm.body);
            }
            let _: &[MatchArm] = arms;
        }
        Expr::Lambda { body, .. } => match body {
            LambdaBody::Expr(e) => f(e),
            LambdaBody::Block(b) => {
                for stmt in &b.statements {
                    walk_stmt_descend(stmt, f);
                }
            }
        },
        _ => {}
    }
}

fn walk_stmt_descend<F>(stmt: &Stmt, f: &mut F)
where
    F: FnMut(&Expr),
{
    match stmt {
        Stmt::Local(b) => f(&b.value),
        Stmt::Reassign(r) => {
            f(&r.lhs);
            f(&r.value);
        }
        Stmt::MemberAssign(m) => {
            f(&m.lhs);
            f(&m.value);
        }
        Stmt::Expr(e) => f(&e.expr),
        Stmt::Return(r) => {
            if let Some(v) = &r.value {
                f(v);
            }
        }
        Stmt::If(i) => {
            for (cond, blk) in &i.branches {
                f(cond);
                for s in &blk.statements {
                    walk_stmt_descend(s, f);
                }
            }
            if let Some(else_blk) = &i.else_block {
                for s in &else_blk.statements {
                    walk_stmt_descend(s, f);
                }
            }
        }
        Stmt::While(w) => {
            f(&w.cond);
            for s in &w.body.statements {
                walk_stmt_descend(s, f);
            }
        }
        Stmt::For(f_stmt) => {
            f(&f_stmt.iter);
            for s in &f_stmt.body.statements {
                walk_stmt_descend(s, f);
            }
        }
        Stmt::Break { .. } | Stmt::Continue { .. } => {}
    }
}

// Pull the unused AST imports back into play through this no-op
// reference. Keeps the file compiling without `#[allow(dead_code)]`
// on a public surface that only some rules need today; future rule
// additions will use these directly.
#[allow(dead_code)]
fn _exhaustiveness_marker(
    _e: &EnumDecl,
    _t: &TraitDecl,
    _c: &ClassDecl,
    _f: &FunctionDecl,
    _con: &ConstructDecl,
    _p: &Param,
    _pat: &Pattern,
) {
}
