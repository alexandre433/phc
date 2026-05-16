// SPDX-License-Identifier: MIT
//! Local-scope walker that resolves every `$name` and `$this`
//! reference to a [`SymbolId`].
//!
//! Operates on top-level functions and on class members (constructor,
//! fields with defaults / hooks, methods) and on trait methods and
//! lambdas. Type-level references (`User`, `Status::Ok`) are not yet
//! resolved; that is pack-level work coming in a follow-up.

use phc_ast::{
    Block, ClassDecl, ClassMember, Expr, FunctionDecl, Ident, Item, LambdaBody, MatchArm, Param,
    Pattern, PropertyHook, SourceFile, Stmt, StrPart, TraitDecl,
};
use phc_errors::{Diagnostic, Severity};
use phc_span::Span;

use crate::{Resolved, Symbol, SymbolId, SymbolKind};

const THIS: &str = "this";

pub fn resolve_bodies(file: &SourceFile, resolved: &mut Resolved) {
    let mut env = Env::default();
    for item in &file.items {
        match item {
            Item::Function(f) => walk_function(f, &mut env, resolved, false),
            Item::Class(c) => walk_class(c, &mut env, resolved),
            Item::Trait(t) => walk_trait(t, &mut env, resolved),
            Item::Test(t) => walk_block(&t.body, &mut env, resolved),
            // Enums / interfaces have no executable bodies in v0.
            Item::Enum(_) | Item::Interface(_) => {}
        }
    }
}

/// One scope frame in the lookup chain. `parent` walks back to the
/// enclosing scope; a `None` parent means top-level.
struct Frame {
    parent: Option<usize>,
    names: Vec<(String, SymbolId)>,
    /// `Some` symbol id when `$this` is in scope (method body,
    /// constructor body, hook body). `None` everywhere else.
    this: Option<SymbolId>,
}

#[derive(Default)]
struct Env {
    frames: Vec<Frame>,
    current: Option<usize>,
}

impl Env {
    fn enter(&mut self, this: Option<SymbolId>) {
        let parent = self.current;
        self.frames.push(Frame {
            parent,
            names: Vec::new(),
            this,
        });
        self.current = Some(self.frames.len() - 1);
    }

    fn leave(&mut self) {
        let cur = self.current.expect("balanced enter/leave");
        self.current = self.frames[cur].parent;
    }

    fn bind(&mut self, name: String, id: SymbolId) {
        let cur = self.current.expect("bind requires an active frame");
        self.frames[cur].names.push((name, id));
    }

    fn lookup(&self, name: &str) -> Option<SymbolId> {
        let mut cur = self.current;
        while let Some(idx) = cur {
            let frame = &self.frames[idx];
            if name == THIS {
                if let Some(id) = frame.this {
                    return Some(id);
                }
            }
            for (n, id) in frame.names.iter().rev() {
                if n == name {
                    return Some(*id);
                }
            }
            cur = frame.parent;
        }
        None
    }
}

fn intro(resolved: &mut Resolved, name: &Ident, kind: SymbolKind) -> SymbolId {
    let id = SymbolId(resolved.symbols.len() as u32);
    resolved.symbols.push(Symbol {
        id,
        name: name.name.clone(),
        kind,
        // Body-level introducers (params, locals, $this) are
        // pack-scoped by construction; visibility is meaningless
        // off the top level but Default keeps the field uniform.
        visibility: crate::Visibility::Default,
        def_span: name.span,
    });
    id
}

fn intro_synthetic(resolved: &mut Resolved, name: &str, def_span: Span) -> SymbolId {
    let id = SymbolId(resolved.symbols.len() as u32);
    resolved.symbols.push(Symbol {
        id,
        name: name.to_string(),
        kind: SymbolKind::Value,
        visibility: crate::Visibility::Default,
        def_span,
    });
    id
}

fn walk_function(f: &FunctionDecl, env: &mut Env, resolved: &mut Resolved, has_this: bool) {
    let this = if has_this {
        Some(intro_synthetic(resolved, THIS, f.name.span))
    } else {
        None
    };
    env.enter(this);
    for p in &f.params {
        bind_param(p, env, resolved);
    }
    walk_block_body(&f.body, env, resolved);
    env.leave();
}

fn walk_class(c: &ClassDecl, env: &mut Env, resolved: &mut Resolved) {
    for member in &c.members {
        match member {
            ClassMember::Construct(con) => {
                let this = intro_synthetic(resolved, THIS, c.name.span);
                env.enter(Some(this));
                for p in &con.params {
                    let id = intro(resolved, &p.name, SymbolKind::Value);
                    env.bind(p.name.name.clone(), id);
                }
                walk_block_body(&con.body, env, resolved);
                env.leave();
            }
            ClassMember::Field(field) => {
                if let Some(default) = &field.default {
                    walk_expr(default, env, resolved);
                }
                for hook in &field.hooks {
                    walk_hook(hook, env, resolved, c.name.span);
                }
            }
            ClassMember::Method(m) => walk_function(m, env, resolved, true),
            ClassMember::TraitUse(_) => {}
        }
    }
}

fn walk_trait(t: &TraitDecl, env: &mut Env, resolved: &mut Resolved) {
    for m in &t.methods {
        walk_function(m, env, resolved, true);
    }
}

fn walk_hook(hook: &PropertyHook, env: &mut Env, resolved: &mut Resolved, class_name_span: Span) {
    let this = intro_synthetic(resolved, THIS, class_name_span);
    env.enter(Some(this));
    match hook {
        PropertyHook::GetExpr { expr, .. } => walk_expr(expr, env, resolved),
        PropertyHook::GetBlock { body, .. } => walk_block_body(body, env, resolved),
        PropertyHook::Set {
            param_name, body, ..
        } => {
            let id = intro(resolved, param_name, SymbolKind::Value);
            env.bind(param_name.name.clone(), id);
            walk_block_body(body, env, resolved);
        }
    }
    env.leave();
}

fn bind_param(p: &Param, env: &mut Env, resolved: &mut Resolved) {
    let id = intro(resolved, &p.name, SymbolKind::Value);
    env.bind(p.name.name.clone(), id);
}

/// Walk a Block's statements without pushing a new scope frame.
/// Each enclosed `{ ... }` Block inside a statement (`if`, `while`,
/// `for`, lambda block body) gets its own [`walk_block`] which DOES
/// push a frame.
fn walk_block_body(block: &Block, env: &mut Env, resolved: &mut Resolved) {
    for stmt in &block.statements {
        walk_stmt(stmt, env, resolved);
    }
}

fn walk_block(block: &Block, env: &mut Env, resolved: &mut Resolved) {
    env.enter(carried_this(env));
    walk_block_body(block, env, resolved);
    env.leave();
}

fn carried_this(env: &Env) -> Option<SymbolId> {
    env.current.and_then(|idx| env.frames[idx].this)
}

fn walk_stmt(stmt: &Stmt, env: &mut Env, resolved: &mut Resolved) {
    match stmt {
        Stmt::Local(b) => {
            walk_expr(&b.value, env, resolved);
            let id = intro(resolved, &b.name, SymbolKind::Value);
            env.bind(b.name.name.clone(), id);
        }
        Stmt::Reassign(r) => {
            walk_expr(&r.lhs, env, resolved);
            walk_expr(&r.value, env, resolved);
        }
        Stmt::MemberAssign(m) => {
            walk_expr(&m.lhs, env, resolved);
            walk_expr(&m.value, env, resolved);
        }
        Stmt::If(i) => {
            for (cond, blk) in &i.branches {
                walk_expr(cond, env, resolved);
                walk_block(blk, env, resolved);
            }
            if let Some(else_blk) = &i.else_block {
                walk_block(else_blk, env, resolved);
            }
        }
        Stmt::While(w) => {
            walk_expr(&w.cond, env, resolved);
            walk_block(&w.body, env, resolved);
        }
        Stmt::For(f) => {
            walk_expr(&f.iter, env, resolved);
            env.enter(carried_this(env));
            let id = intro(resolved, &f.elem_name, SymbolKind::Value);
            env.bind(f.elem_name.name.clone(), id);
            walk_block_body(&f.body, env, resolved);
            env.leave();
        }
        Stmt::Return(r) => {
            if let Some(v) = &r.value {
                walk_expr(v, env, resolved);
            }
        }
        Stmt::Break { .. } | Stmt::Continue { .. } => {}
        Stmt::Expr(e) => walk_expr(&e.expr, env, resolved),
    }
}

fn walk_expr(expr: &Expr, env: &mut Env, resolved: &mut Resolved) {
    match expr {
        Expr::IntLit { .. }
        | Expr::FloatLit { .. }
        | Expr::BoolLit { .. }
        | Expr::NullLit { .. }
        | Expr::TypeName { .. } => {}
        Expr::This { span } => match env.lookup(THIS) {
            Some(id) => record_use(resolved, *span, id),
            None => unresolved(resolved, *span, "$this"),
        },
        Expr::Var { name, span } => match env.lookup(&name.name) {
            Some(id) => record_use(resolved, *span, id),
            None => unresolved(resolved, *span, &format!("${}", name.name)),
        },
        Expr::StrLit { parts, .. } => {
            for part in parts {
                if let StrPart::Expr(e) = part {
                    walk_expr(e, env, resolved);
                }
            }
        }
        Expr::Paren { inner, .. } => walk_expr(inner, env, resolved),
        Expr::Member { receiver, .. } => walk_expr(receiver, env, resolved),
        Expr::Static { ty, .. } => walk_expr(ty, env, resolved),
        Expr::Call { callee, args, .. } => {
            walk_expr(callee, env, resolved);
            for a in args {
                walk_expr(a, env, resolved);
            }
        }
        Expr::Index { target, index, .. } => {
            walk_expr(target, env, resolved);
            walk_expr(index, env, resolved);
        }
        Expr::Try { value, .. } => walk_expr(value, env, resolved),
        Expr::Unary { operand, .. } => walk_expr(operand, env, resolved),
        Expr::Borrow { operand, .. } => walk_expr(operand, env, resolved),
        Expr::Cast { value, .. } => walk_expr(value, env, resolved),
        Expr::Binary { lhs, rhs, .. } => {
            walk_expr(lhs, env, resolved);
            walk_expr(rhs, env, resolved);
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            walk_expr(scrutinee, env, resolved);
            for arm in arms {
                walk_match_arm(arm, env, resolved);
            }
        }
        Expr::Lambda { params, body, .. } => {
            env.enter(carried_this(env));
            for p in params {
                bind_param(p, env, resolved);
            }
            match body {
                LambdaBody::Expr(e) => walk_expr(e, env, resolved),
                LambdaBody::Block(b) => walk_block_body(b, env, resolved),
            }
            env.leave();
        }
    }
}

fn walk_match_arm(arm: &MatchArm, env: &mut Env, resolved: &mut Resolved) {
    env.enter(carried_this(env));
    bind_pattern(&arm.pattern, env, resolved);
    if let Some(guard) = &arm.guard {
        walk_expr(guard, env, resolved);
    }
    walk_expr(&arm.body, env, resolved);
    env.leave();
}

/// Patterns may introduce bindings: `Pattern::Var { $x }` binds `$x`
/// for the arm body. Literal / wildcard / enum-variant patterns add
/// no bindings; OR patterns must agree on the bindings they
/// introduce, but that check is a follow-up.
fn bind_pattern(pat: &Pattern, env: &mut Env, resolved: &mut Resolved) {
    match pat {
        Pattern::Wildcard { .. } | Pattern::Literal(_) | Pattern::EnumVariant { .. } => {}
        Pattern::Var { name, .. } => {
            let id = intro(resolved, name, SymbolKind::Value);
            env.bind(name.name.clone(), id);
        }
        Pattern::Or { atoms, .. } => {
            for atom in atoms {
                bind_pattern(atom, env, resolved);
            }
        }
    }
}

fn record_use(resolved: &mut Resolved, span: Span, id: SymbolId) {
    resolved.uses.insert(span, id);
}

fn unresolved(resolved: &mut Resolved, span: Span, name: &str) {
    resolved.diagnostics.push(Diagnostic {
        severity: Severity::Error,
        message: format!("unresolved name `{name}`"),
        span,
    });
}
