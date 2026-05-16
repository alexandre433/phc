// SPDX-License-Identifier: MIT
//! Borrowcheck walker. Builds a SymbolId → mutability map up front
//! by walking declarations, then walks every function body once and
//! emits diagnostics for the rules listed in [`crate`] doc.

use phc_ast::{
    Block, Borrow, ClassDecl, ClassMember, ConstructDecl, Expr, FieldDecl, FunctionDecl, Item,
    LambdaBody, MatchArm, Param, Pattern, SourceFile, Stmt, StrPart, TraitDecl,
};
use phc_errors::{Diagnostic, Severity};
use phc_semantic::{Resolved, SymbolId};
use phc_span::Span;
use phc_typecheck::{Ty, Typed};
use std::collections::HashMap;

use crate::Borrowed;

pub(crate) fn run(file: &SourceFile, resolved: &Resolved, typed: &Typed) -> Borrowed {
    let mut out = Borrowed::default();
    let mut_bindings = collect_mutable_bindings(file, resolved);
    let class_fields = collect_class_fields(file);
    let mut ctx = Ctx {
        diagnostics: &mut out.diagnostics,
        resolved,
        typed,
        mut_bindings: &mut_bindings,
        class_fields: &class_fields,
        in_constructor: false,
    };
    for item in &file.items {
        match item {
            Item::Function(f) => ctx.check_block(&f.body),
            Item::Class(c) => ctx.check_class(c),
            Item::Trait(t) => {
                for m in &t.methods {
                    ctx.check_block(&m.body);
                }
            }
            Item::Test(t) => ctx.check_block(&t.body),
            _ => {}
        }
    }
    out
}

struct Ctx<'a> {
    diagnostics: &'a mut Vec<Diagnostic>,
    resolved: &'a Resolved,
    typed: &'a Typed,
    /// Set of every binding (param, local, lambda param, for-elem,
    /// pattern Var, field) declared `flip`. Looked up by SymbolId.
    mut_bindings: &'a std::collections::HashSet<SymbolId>,
    /// (class_name, field_name) → field is_mut. Used to decide
    /// whether `$obj->f = expr;` is legal when the receiver's static
    /// type resolves to `class_name`.
    class_fields: &'a HashMap<(String, String), bool>,
    /// True while walking a `construct(...)` body. Inside a
    /// constructor, `$this->field = expr;` is a binding/initialisation
    /// of an immutable field per D-012 — the flip-field rule only
    /// applies to writes that happen after construction.
    in_constructor: bool,
}

impl Ctx<'_> {
    fn check_class(&mut self, c: &ClassDecl) {
        for member in &c.members {
            match member {
                ClassMember::Method(m) => self.check_block(&m.body),
                ClassMember::Construct(con) => {
                    self.in_constructor = true;
                    self.check_block(&con.body);
                    self.in_constructor = false;
                }
                ClassMember::Field(f) => {
                    if let Some(default) = &f.default {
                        self.check_expr(default);
                    }
                }
                ClassMember::TraitUse(_) => {}
            }
        }
    }

    fn check_block(&mut self, block: &Block) {
        for stmt in &block.statements {
            self.check_stmt(stmt);
        }
    }

    fn check_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Local(b) => self.check_expr(&b.value),
            Stmt::Reassign(r) => {
                // Rule (a): `:=` LHS must be a mutable binding.
                self.check_reassign_lhs(&r.lhs, r.span);
                self.check_expr(&r.value);
            }
            Stmt::MemberAssign(m) => {
                // Rule (b): `=` on `$obj->field` only when field is flip.
                self.check_member_assign(&m.lhs, m.span);
                self.check_expr(&m.value);
            }
            Stmt::Expr(e) => self.check_expr(&e.expr),
            Stmt::Return(r) => {
                if let Some(v) = &r.value {
                    self.check_expr(v);
                }
            }
            Stmt::If(i) => {
                for (cond, blk) in &i.branches {
                    self.check_expr(cond);
                    self.check_block(blk);
                }
                if let Some(else_blk) = &i.else_block {
                    self.check_block(else_blk);
                }
            }
            Stmt::While(w) => {
                self.check_expr(&w.cond);
                self.check_block(&w.body);
            }
            Stmt::For(f) => {
                self.check_expr(&f.iter);
                self.check_block(&f.body);
            }
            Stmt::Break { .. } | Stmt::Continue { .. } => {}
        }
    }

    /// Walk a `:=` LHS. The parser guarantees it's a chain of
    /// `Member` rooted at `Var` / `This`. The mutability rule applies
    /// to the **root** binding: `$x := ...` requires `$x` flip;
    /// `$x->a := ...` reaches into `$x`'s field, which is governed
    /// by the field's own `flip` (handled by member-assign), not by
    /// `$x`'s mutability — so a chain LHS is legal regardless of the
    /// root's flip state.
    fn check_reassign_lhs(&mut self, lhs: &Expr, span: Span) {
        if let Expr::Var {
            name,
            span: var_span,
        } = lhs
        {
            if let Some(&sid) = self.resolved.uses.get(var_span) {
                if !self.mut_bindings.contains(&sid) {
                    self.diag(
                        span,
                        format!(
                            "cannot reassign immutable binding `${}` — declare it with `flip` to allow `:=`",
                            name.name
                        ),
                    );
                }
            }
        }
    }

    /// Walk a `=` member-assign LHS. The parser guarantees the LHS
    /// is `Member { receiver, field }` (possibly chained). The
    /// terminal field's `flip` flag is what matters; intermediate
    /// chain segments only need to resolve to readable members.
    fn check_member_assign(&mut self, lhs: &Expr, span: Span) {
        let Expr::Member {
            receiver, field, ..
        } = lhs
        else {
            return;
        };
        // D-012: inside `construct(...)`, `$this->field = expr;` is
        // initialisation, not reassignment — exempt from the flip
        // rule. The exemption is scoped to `$this` only; writes
        // through other receivers inside a constructor still follow
        // the normal field-mutability rule.
        if self.in_constructor && matches!(receiver.as_ref(), Expr::This { .. }) {
            return;
        }
        let class = match self.class_of(receiver) {
            Some(c) => c,
            None => return, // unknown receiver type — typecheck will warn separately
        };
        match self.class_fields.get(&(class.clone(), field.name.clone())) {
            Some(true) => {} // flip field: write OK
            Some(false) => self.diag(
                span,
                format!(
                    "cannot assign to immutable field `{}` on `{}` — declare it `flip` to allow writes",
                    field.name, class
                ),
            ),
            None => {
                // Field unknown to the field map. Could be a
                // typecheck-recovered chain (e.g. nested member),
                // a hook-only property, or a typo — leave it for
                // the typechecker to flag.
            }
        }
    }

    fn check_expr(&mut self, expr: &Expr) {
        match expr {
            // Rule (c): `&flip $x` requires $x to be flip. Borrow
            // operands are typically a `Var`; chains like `&flip
            // $u->field` lower the field-mutability question to its
            // declaring class which doesn't apply to operand-level
            // borrows in v0, so we only check the leaf-Var case.
            Expr::Borrow {
                kind,
                operand,
                span,
            } => {
                if matches!(kind, Borrow::Mutable) {
                    if let Expr::Var {
                        name,
                        span: var_span,
                    } = operand.as_ref()
                    {
                        if let Some(&sid) = self.resolved.uses.get(var_span) {
                            if !self.mut_bindings.contains(&sid) {
                                self.diag(
                                    *span,
                                    format!(
                                        "cannot mutably borrow `${}` — declare it with `flip` first",
                                        name.name
                                    ),
                                );
                            }
                        }
                    }
                }
                self.check_expr(operand);
            }
            Expr::Paren { inner, .. } => self.check_expr(inner),
            Expr::Unary { operand, .. } => self.check_expr(operand),
            Expr::Try { value, .. } | Expr::Cast { value, .. } => self.check_expr(value),
            Expr::Member { receiver, .. } => self.check_expr(receiver),
            Expr::Static { ty, .. } => self.check_expr(ty),
            Expr::Binary { lhs, rhs, .. } => {
                self.check_expr(lhs);
                self.check_expr(rhs);
            }
            Expr::Call { callee, args, span } => {
                self.check_expr(callee);
                for a in args {
                    self.check_expr(a);
                }
                // Per-call aliasing rule (D-005 extension): within a
                // single call's arg list, no two borrows of the same
                // root binding may be mutable, and a mutable borrow
                // cannot coexist with any other borrow of the same
                // root. Catches the classic `swap(&flip $x, &flip $x)`
                // and `read(&$x, &flip $x)` shapes; broader aliasing
                // (across statements, through let-bindings) needs the
                // full liveness pass tracked separately.
                self.check_call_aliasing(args, *span);
            }
            Expr::Index { target, index, .. } => {
                self.check_expr(target);
                self.check_expr(index);
            }
            Expr::StrLit { parts, .. } => {
                for p in parts {
                    if let StrPart::Expr(e) = p {
                        self.check_expr(e);
                    }
                }
            }
            Expr::Match {
                scrutinee, arms, ..
            } => {
                self.check_expr(scrutinee);
                for arm in arms {
                    if let Some(g) = &arm.guard {
                        self.check_expr(g);
                    }
                    self.check_expr(&arm.body);
                }
                let _: &[MatchArm] = arms;
            }
            Expr::Lambda { body, .. } => match body {
                LambdaBody::Expr(e) => self.check_expr(e),
                LambdaBody::Block(b) => self.check_block(b),
            },
            Expr::IntLit { .. }
            | Expr::FloatLit { .. }
            | Expr::BoolLit { .. }
            | Expr::NullLit { .. }
            | Expr::Var { .. }
            | Expr::This { .. }
            | Expr::TypeName { .. } => {}
        }
    }

    /// Resolve a member-assign receiver to its declaring class name
    /// by reading the typechecker's expr-types map. Returns None
    /// when the receiver's type has not been resolved (typecheck
    /// will report a separate diagnostic in that case).
    fn class_of(&self, expr: &Expr) -> Option<String> {
        let span = match expr {
            Expr::Var { span, .. }
            | Expr::This { span }
            | Expr::Member { span, .. }
            | Expr::Paren { span, .. } => *span,
            _ => return None,
        };
        match self.typed.expr_types.get(&span)? {
            Ty::Path { path, .. } if path.len() == 1 => Some(path[0].clone()),
            _ => None,
        }
    }

    /// Check borrow-arg aliasing within a single call. Builds a
    /// `(root_sid, borrow_kind)` list from the args' borrow
    /// expressions (only leaf-`Var` borrows in v0) and reports any
    /// pair that violates D-005 aliasing: two mutable borrows of
    /// the same root, or a mutable borrow alongside any other
    /// borrow of the same root.
    fn check_call_aliasing(&mut self, args: &[Expr], call_span: Span) {
        let mut borrows: Vec<(SymbolId, Borrow, &str, Span)> = Vec::new();
        for arg in args {
            if let Expr::Borrow {
                kind,
                operand,
                span,
            } = arg
            {
                if matches!(kind, Borrow::Shared | Borrow::Mutable) {
                    if let Expr::Var {
                        name,
                        span: var_span,
                    } = operand.as_ref()
                    {
                        if let Some(&sid) = self.resolved.uses.get(var_span) {
                            borrows.push((sid, *kind, name.name.as_str(), *span));
                        }
                    }
                }
            }
        }
        for i in 0..borrows.len() {
            for j in (i + 1)..borrows.len() {
                let (sid_i, kind_i, name_i, span_i) = borrows[i];
                let (sid_j, kind_j, _name_j, _span_j) = borrows[j];
                if sid_i != sid_j {
                    continue;
                }
                let conflict = matches!(
                    (kind_i, kind_j),
                    (Borrow::Mutable, _) | (_, Borrow::Mutable)
                );
                if conflict {
                    let _ = call_span;
                    self.diag(
                        span_i,
                        format!(
                            "conflicting borrows of `${}` in the same call: {} and {}",
                            name_i,
                            describe_borrow(kind_i),
                            describe_borrow(kind_j),
                        ),
                    );
                }
            }
        }
    }

    fn diag(&mut self, span: Span, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic {
            severity: Severity::Error,
            message: message.into(),
            span,
        });
    }
}

/// Walk every binding the resolver registered and record which ones
/// were declared `flip`. Sources of mutability:
/// - `LocalBinding { is_mut: true }`
/// - `Param` / `ConstructParam` with `Borrow::Mutable` borrow OR
///   (for ConstructParam) the explicit `is_mut` flag
/// - `FieldDecl { is_mut: true }`
///
/// For-elem bindings, lambda params, and match-pattern Var bindings
/// all start immutable in v0; `flip` on those forms is not yet
/// surfaced in the AST and lands in a follow-up.
fn collect_mutable_bindings(
    file: &SourceFile,
    resolved: &Resolved,
) -> std::collections::HashSet<SymbolId> {
    let mut out = std::collections::HashSet::new();
    let mut record = |span: Span, out: &mut std::collections::HashSet<SymbolId>| {
        if let Some(sid) = symbol_at_def(resolved, span) {
            out.insert(sid);
        }
    };
    for item in &file.items {
        match item {
            Item::Function(f) => collect_in_function(f, resolved, &mut record, &mut out),
            Item::Class(c) => collect_in_class(c, resolved, &mut record, &mut out),
            Item::Trait(t) => {
                for m in &t.methods {
                    collect_in_function(m, resolved, &mut record, &mut out);
                }
            }
            Item::Test(t) => collect_in_block(&t.body, resolved, &mut record, &mut out),
            _ => {}
        }
    }
    out
}

fn collect_in_function<F>(
    f: &FunctionDecl,
    resolved: &Resolved,
    record: &mut F,
    out: &mut std::collections::HashSet<SymbolId>,
) where
    F: FnMut(Span, &mut std::collections::HashSet<SymbolId>),
{
    for p in &f.params {
        if param_is_mut(p) {
            record(p.name.span, out);
        }
    }
    collect_in_block(&f.body, resolved, record, out);
}

fn collect_in_class<F>(
    c: &ClassDecl,
    resolved: &Resolved,
    record: &mut F,
    out: &mut std::collections::HashSet<SymbolId>,
) where
    F: FnMut(Span, &mut std::collections::HashSet<SymbolId>),
{
    for member in &c.members {
        match member {
            ClassMember::Field(f) => {
                if f.is_mut {
                    record(f.name.span, out);
                }
                if let Some(default) = &f.default {
                    collect_in_expr(default, resolved, record, out);
                }
            }
            ClassMember::Method(m) => collect_in_function(m, resolved, record, out),
            ClassMember::Construct(con) => collect_in_construct(con, resolved, record, out),
            ClassMember::TraitUse(_) => {}
        }
    }
}

fn collect_in_construct<F>(
    con: &ConstructDecl,
    resolved: &Resolved,
    record: &mut F,
    out: &mut std::collections::HashSet<SymbolId>,
) where
    F: FnMut(Span, &mut std::collections::HashSet<SymbolId>),
{
    for p in &con.params {
        if matches!(p.borrow, Borrow::Mutable) || p.is_mut {
            record(p.name.span, out);
        }
    }
    collect_in_block(&con.body, resolved, record, out);
}

fn collect_in_block<F>(
    block: &Block,
    resolved: &Resolved,
    record: &mut F,
    out: &mut std::collections::HashSet<SymbolId>,
) where
    F: FnMut(Span, &mut std::collections::HashSet<SymbolId>),
{
    for stmt in &block.statements {
        collect_in_stmt(stmt, resolved, record, out);
    }
}

fn collect_in_stmt<F>(
    stmt: &Stmt,
    resolved: &Resolved,
    record: &mut F,
    out: &mut std::collections::HashSet<SymbolId>,
) where
    F: FnMut(Span, &mut std::collections::HashSet<SymbolId>),
{
    match stmt {
        Stmt::Local(b) => {
            if b.is_mut {
                record(b.name.span, out);
            }
            collect_in_expr(&b.value, resolved, record, out);
        }
        Stmt::Reassign(r) => {
            collect_in_expr(&r.lhs, resolved, record, out);
            collect_in_expr(&r.value, resolved, record, out);
        }
        Stmt::MemberAssign(m) => {
            collect_in_expr(&m.lhs, resolved, record, out);
            collect_in_expr(&m.value, resolved, record, out);
        }
        Stmt::Expr(e) => collect_in_expr(&e.expr, resolved, record, out),
        Stmt::Return(r) => {
            if let Some(v) = &r.value {
                collect_in_expr(v, resolved, record, out);
            }
        }
        Stmt::If(i) => {
            for (cond, blk) in &i.branches {
                collect_in_expr(cond, resolved, record, out);
                collect_in_block(blk, resolved, record, out);
            }
            if let Some(else_blk) = &i.else_block {
                collect_in_block(else_blk, resolved, record, out);
            }
        }
        Stmt::While(w) => {
            collect_in_expr(&w.cond, resolved, record, out);
            collect_in_block(&w.body, resolved, record, out);
        }
        Stmt::For(f) => {
            collect_in_expr(&f.iter, resolved, record, out);
            collect_in_block(&f.body, resolved, record, out);
        }
        Stmt::Break { .. } | Stmt::Continue { .. } => {}
    }
}

fn collect_in_expr<F>(
    expr: &Expr,
    resolved: &Resolved,
    record: &mut F,
    out: &mut std::collections::HashSet<SymbolId>,
) where
    F: FnMut(Span, &mut std::collections::HashSet<SymbolId>),
{
    match expr {
        Expr::Lambda { params, body, .. } => {
            for p in params {
                if param_is_mut(p) {
                    record(p.name.span, out);
                }
            }
            match body {
                LambdaBody::Expr(e) => collect_in_expr(e, resolved, record, out),
                LambdaBody::Block(b) => collect_in_block(b, resolved, record, out),
            }
        }
        Expr::Paren { inner, .. } => collect_in_expr(inner, resolved, record, out),
        Expr::Unary { operand, .. } | Expr::Borrow { operand, .. } => {
            collect_in_expr(operand, resolved, record, out)
        }
        Expr::Try { value, .. } | Expr::Cast { value, .. } => {
            collect_in_expr(value, resolved, record, out)
        }
        Expr::Member { receiver, .. } => collect_in_expr(receiver, resolved, record, out),
        Expr::Static { ty, .. } => collect_in_expr(ty, resolved, record, out),
        Expr::Binary { lhs, rhs, .. } => {
            collect_in_expr(lhs, resolved, record, out);
            collect_in_expr(rhs, resolved, record, out);
        }
        Expr::Call { callee, args, .. } => {
            collect_in_expr(callee, resolved, record, out);
            for a in args {
                collect_in_expr(a, resolved, record, out);
            }
        }
        Expr::Index { target, index, .. } => {
            collect_in_expr(target, resolved, record, out);
            collect_in_expr(index, resolved, record, out);
        }
        Expr::StrLit { parts, .. } => {
            for p in parts {
                if let StrPart::Expr(e) = p {
                    collect_in_expr(e, resolved, record, out);
                }
            }
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            collect_in_expr(scrutinee, resolved, record, out);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    collect_in_expr(g, resolved, record, out);
                }
                collect_in_expr(&arm.body, resolved, record, out);
            }
            let _: &Pattern = &arms[0].pattern; // Pattern Var bindings start immutable
        }
        Expr::IntLit { .. }
        | Expr::FloatLit { .. }
        | Expr::BoolLit { .. }
        | Expr::NullLit { .. }
        | Expr::Var { .. }
        | Expr::This { .. }
        | Expr::TypeName { .. } => {}
    }
}

fn describe_borrow(kind: Borrow) -> &'static str {
    match kind {
        Borrow::None => "owned",
        Borrow::Shared => "shared `&`",
        Borrow::Mutable => "mutable `&flip`",
    }
}

fn param_is_mut(p: &Param) -> bool {
    matches!(p.borrow, Borrow::Mutable)
}

/// Build (class_name, field_name) → is_mut from every class in the
/// file. Promoted constructor params count as fields with the
/// param's `is_mut` flag (or `Borrow::Mutable` borrow on the param).
fn collect_class_fields(file: &SourceFile) -> HashMap<(String, String), bool> {
    let mut out = HashMap::new();
    for item in &file.items {
        if let Item::Class(c) = item {
            let class_name = c.name.name.clone();
            for member in &c.members {
                match member {
                    ClassMember::Field(f) => {
                        out.insert((class_name.clone(), f.name.name.clone()), f.is_mut);
                    }
                    ClassMember::Construct(con) => {
                        for p in &con.params {
                            if p.promoted {
                                let mu = p.is_mut || matches!(p.borrow, Borrow::Mutable);
                                out.insert((class_name.clone(), p.name.name.clone()), mu);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    out
}

fn symbol_at_def(resolved: &Resolved, span: Span) -> Option<SymbolId> {
    resolved
        .symbols
        .iter()
        .find(|s| s.def_span == span)
        .map(|s| s.id)
}

// Reference unused walker types from phc-ast so a future addition
// to the trait/interface surface trips the matcher rather than
// silently bypassing the borrow checker.
#[allow(dead_code)]
fn _exhaustiveness_marker(_t: &TraitDecl, _f: &FieldDecl) {}
