// SPDX-License-Identifier: MIT
//! Monomorphization for generic free functions.
//!
//! Scans the source file for every call site that targets a generic
//! free function, builds a substituted clone of the [`FunctionDecl`]
//! for each unique concrete-type tuple, and gives the C emitter a
//! deterministic per-instance name to emit and dispatch under.
//!
//! Scope today: free functions whose generic parameters are bound by
//! a direct-position parameter (`function max<T>(T $a, T $b): T`).
//! Nested binding (`function len<T>(list<T> $xs): int`), generic
//! class methods, and generic type instantiations on the return-only
//! position are deferred. Anything outside scope leaves the original
//! generic function emitted as-is, with `phc_value` placeholders.

use phc_ast::{
    ClassMember, Expr, FunctionDecl, Ident, Item, LambdaBody, MatchArm, Pattern, PropertyHook,
    SourceFile, Stmt, StrPart, TypeRef,
};
use phc_semantic::{Resolved, SymbolKind};
use phc_span::Span;
use phc_typecheck::{Primitive, Ty, Typed};

use std::collections::HashMap;

/// One concrete instantiation of a generic free function.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct MonoInstance {
    /// Source-level name of the generic function (`max`).
    pub source_name: String,
    /// Concrete types, in `generic_params` order, used to specialise
    /// the function (`[Ty::Int]` for `max(3, 7)`).
    pub concrete_tys: Vec<Ty>,
    /// C symbol the specialised function emits and call sites use to
    /// dispatch (`phc_max__int`).
    pub mangled: String,
    /// Cloned [`FunctionDecl`] with every `T`-shaped [`TypeRef`]
    /// rewritten to the matching concrete TypeRef. The clone keeps
    /// the original spans so diagnostics still point at source.
    pub decl: FunctionDecl,
}

/// Collect every monomorphization instance the source file requires.
///
/// Returns a `Vec` (rather than a map) so the C emitter can iterate
/// in stable order. Each `(source_name, concrete_tys)` tuple appears
/// at most once.
pub fn collect_mono_instances(
    file: &SourceFile,
    resolved: &Resolved,
    typed: &Typed,
) -> Vec<MonoInstance> {
    let mut generics: HashMap<&str, &FunctionDecl> = HashMap::new();
    for item in &file.items {
        if let Item::Function(f) = item {
            if !f.generic_params.is_empty() {
                generics.insert(f.name.name.as_str(), f);
            }
        }
    }
    if generics.is_empty() {
        return Vec::new();
    }

    let mut instances: Vec<MonoInstance> = Vec::new();
    let mut seen: HashMap<(String, Vec<Ty>), ()> = HashMap::new();
    walk_calls(file, &mut |callee, args| {
        let name = match callee_name(callee) {
            Some(n) => n,
            None => return,
        };
        let id = match resolved.top_level.get(name).copied() {
            Some(id) => id,
            None => return,
        };
        if resolved.symbol(id).kind != SymbolKind::Function {
            return;
        }
        let decl = match generics.get(name) {
            Some(d) => *d,
            None => return,
        };
        let concrete = match concrete_tys_for_call(decl, args, typed) {
            Some(t) => t,
            None => return,
        };
        let key = (name.to_string(), concrete.clone());
        if seen.contains_key(&key) {
            return;
        }
        seen.insert(key, ());
        let mangled = mangle_instance(name, &concrete);
        let substituted = substitute_function(decl, &concrete);
        instances.push(MonoInstance {
            source_name: name.to_string(),
            concrete_tys: concrete,
            mangled,
            decl: substituted,
        });
    });
    instances
}

/// Compute the mangled C symbol for a `(source_name, concrete_tys)`
/// pair. Exposed so the C emitter can resolve a call site without
/// re-running the full collection pass.
pub fn mangle_instance(source_name: &str, concrete_tys: &[Ty]) -> String {
    let mut s = format!("phc_{source_name}__");
    for (i, t) in concrete_tys.iter().enumerate() {
        if i > 0 {
            s.push('_');
        }
        push_ty_mangle(t, &mut s);
    }
    s
}

/// Compute the concrete-type tuple a call site provides for a
/// generic function, in the function's `generic_params` order.
/// Returns `None` if any binding can't be inferred (the call stays
/// unspecialised so the existing diagnostic path runs).
pub fn concrete_tys_for_call(decl: &FunctionDecl, args: &[Expr], typed: &Typed) -> Option<Vec<Ty>> {
    let mut out = Vec::with_capacity(decl.generic_params.len());
    for gp in &decl.generic_params {
        let pos = direct_param_position(decl, &gp.name.name)?;
        let arg = args.get(pos)?;
        let ty = typed.expr_types.get(&span_of_expr(arg))?.clone();
        if matches!(ty, Ty::Unknown) {
            return None;
        }
        out.push(ty);
    }
    Some(out)
}

fn direct_param_position(decl: &FunctionDecl, generic_name: &str) -> Option<usize> {
    for (i, p) in decl.params.iter().enumerate() {
        if p.ty.path.len() == 1 && p.ty.args.is_empty() && p.ty.path[0].name == generic_name {
            return Some(i);
        }
    }
    None
}

fn substitute_function(decl: &FunctionDecl, concrete_tys: &[Ty]) -> FunctionDecl {
    let mut subst: HashMap<String, TypeRef> = HashMap::new();
    for (gp, ty) in decl.generic_params.iter().zip(concrete_tys.iter()) {
        subst.insert(gp.name.name.clone(), ty_to_typeref(ty, gp.name.span));
    }
    let mut cloned = decl.clone();
    cloned.generic_params.clear();
    for p in &mut cloned.params {
        substitute_typeref(&mut p.ty, &subst);
    }
    substitute_typeref(&mut cloned.return_type, &subst);
    cloned
}

fn substitute_typeref(t: &mut TypeRef, subst: &HashMap<String, TypeRef>) {
    if t.fn_return.is_none() && t.args.is_empty() && t.path.len() == 1 {
        if let Some(repl) = subst.get(&t.path[0].name) {
            let span = t.span;
            *t = repl.clone();
            t.span = span;
            return;
        }
    }
    for a in &mut t.args {
        substitute_typeref(a, subst);
    }
    if let Some(r) = t.fn_return.as_mut() {
        substitute_typeref(r, subst);
    }
}

fn ty_to_typeref(ty: &Ty, span: Span) -> TypeRef {
    let name = match ty {
        Ty::Primitive(Primitive::Int) => "int",
        Ty::Primitive(Primitive::Float) => "float",
        Ty::Primitive(Primitive::Bool) => "bool",
        Ty::Primitive(Primitive::String) => "string",
        Ty::Primitive(Primitive::Byte) => "byte",
        Ty::Primitive(Primitive::Bytes) => "bytes",
        Ty::Primitive(Primitive::Void) => "void",
        Ty::Path { path, .. } => {
            return TypeRef {
                path: path
                    .iter()
                    .map(|seg| Ident {
                        name: seg.clone(),
                        span,
                    })
                    .collect(),
                args: Vec::new(),
                nullable: false,
                fn_return: None,
                span,
            };
        }
        Ty::NullablePrimitive(p) => {
            return TypeRef {
                path: vec![Ident {
                    name: p.as_str().to_string(),
                    span,
                }],
                args: Vec::new(),
                nullable: true,
                fn_return: None,
                span,
            };
        }
        _ => "phc_value",
    };
    TypeRef {
        path: vec![Ident {
            name: name.to_string(),
            span,
        }],
        args: Vec::new(),
        nullable: false,
        fn_return: None,
        span,
    }
}

fn push_ty_mangle(ty: &Ty, out: &mut String) {
    match ty {
        Ty::Primitive(p) => out.push_str(p.as_str()),
        Ty::Path { path, .. } => {
            for (i, seg) in path.iter().enumerate() {
                if i > 0 {
                    out.push('_');
                }
                out.push_str(seg);
            }
        }
        Ty::NullablePrimitive(p) => {
            out.push_str(p.as_str());
            out.push_str("_opt");
        }
        Ty::Generic { name } => out.push_str(name),
        Ty::Unknown => out.push_str("unknown"),
    }
}

fn callee_name(callee: &Expr) -> Option<&str> {
    match callee {
        Expr::TypeName { name, .. } => Some(&name.name),
        _ => None,
    }
}

fn walk_calls<F: FnMut(&Expr, &[Expr])>(file: &SourceFile, f: &mut F) {
    for item in &file.items {
        match item {
            Item::Function(fd) => walk_block(&fd.body, f),
            Item::Class(c) => {
                for m in &c.members {
                    match m {
                        ClassMember::Method(md) => walk_block(&md.body, f),
                        ClassMember::Construct(cd) => walk_block(&cd.body, f),
                        ClassMember::Field(fd) => {
                            if let Some(d) = &fd.default {
                                walk_expr(d, f);
                            }
                            for h in &fd.hooks {
                                match h {
                                    PropertyHook::GetExpr { expr, .. } => walk_expr(expr, f),
                                    PropertyHook::GetBlock { body, .. } => walk_block(body, f),
                                    PropertyHook::Set { body, .. } => walk_block(body, f),
                                }
                            }
                        }
                        ClassMember::TraitUse(_) => {}
                    }
                }
            }
            Item::Trait(t) => {
                for m in &t.methods {
                    walk_block(&m.body, f);
                }
            }
            Item::Test(t) => walk_block(&t.body, f),
            Item::Enum(_) | Item::Interface(_) => {}
        }
    }
}

fn walk_block<F: FnMut(&Expr, &[Expr])>(b: &phc_ast::Block, f: &mut F) {
    for s in &b.statements {
        walk_stmt(s, f);
    }
}

fn walk_stmt<F: FnMut(&Expr, &[Expr])>(s: &Stmt, f: &mut F) {
    match s {
        Stmt::Local(l) => walk_expr(&l.value, f),
        Stmt::Reassign(r) => {
            walk_expr(&r.lhs, f);
            walk_expr(&r.value, f);
        }
        Stmt::MemberAssign(m) => {
            walk_expr(&m.lhs, f);
            walk_expr(&m.value, f);
        }
        Stmt::Expr(e) => walk_expr(&e.expr, f),
        Stmt::Return(r) => {
            if let Some(v) = &r.value {
                walk_expr(v, f);
            }
        }
        Stmt::If(i) => {
            for (cond, body) in &i.branches {
                walk_expr(cond, f);
                walk_block(body, f);
            }
            if let Some(eb) = &i.else_block {
                walk_block(eb, f);
            }
        }
        Stmt::While(w) => {
            walk_expr(&w.cond, f);
            walk_block(&w.body, f);
        }
        Stmt::For(fo) => {
            walk_expr(&fo.iter, f);
            walk_block(&fo.body, f);
        }
        Stmt::Break { .. } | Stmt::Continue { .. } => {}
    }
}

fn walk_expr<F: FnMut(&Expr, &[Expr])>(e: &Expr, f: &mut F) {
    match e {
        Expr::Call { callee, args, .. } => {
            f(callee, args);
            walk_expr(callee, f);
            for a in args {
                walk_expr(a, f);
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            walk_expr(lhs, f);
            walk_expr(rhs, f);
        }
        Expr::Unary { operand, .. } => walk_expr(operand, f),
        Expr::Borrow { operand, .. } => walk_expr(operand, f),
        Expr::Member { receiver, .. } => walk_expr(receiver, f),
        Expr::Index { target, index, .. } => {
            walk_expr(target, f);
            walk_expr(index, f);
        }
        Expr::Paren { inner, .. } => walk_expr(inner, f),
        Expr::Cast { value, .. } => walk_expr(value, f),
        Expr::Try { value, .. } => walk_expr(value, f),
        Expr::StrLit { parts, .. } => {
            for p in parts {
                if let StrPart::Expr(inner) = p {
                    walk_expr(inner, f);
                }
            }
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            walk_expr(scrutinee, f);
            for arm in arms {
                walk_match_arm(arm, f);
            }
        }
        Expr::Lambda { body, .. } => match body {
            LambdaBody::Expr(e) => walk_expr(e, f),
            LambdaBody::Block(b) => walk_block(b, f),
        },
        Expr::Static { ty, .. } => walk_expr(ty, f),
        Expr::IntLit { .. }
        | Expr::FloatLit { .. }
        | Expr::BoolLit { .. }
        | Expr::NullLit { .. }
        | Expr::This { .. }
        | Expr::Var { .. }
        | Expr::TypeName { .. } => {}
    }
}

fn walk_match_arm<F: FnMut(&Expr, &[Expr])>(arm: &MatchArm, f: &mut F) {
    walk_pattern(&arm.pattern, f);
    if let Some(g) = &arm.guard {
        walk_expr(g, f);
    }
    walk_expr(&arm.body, f);
}

fn walk_pattern<F: FnMut(&Expr, &[Expr])>(p: &Pattern, f: &mut F) {
    match p {
        Pattern::Literal(e) => walk_expr(e, f),
        Pattern::Or { atoms, .. } => {
            for a in atoms {
                walk_pattern(a, f);
            }
        }
        Pattern::Wildcard { .. } | Pattern::Var { .. } | Pattern::EnumVariant { .. } => {}
    }
}

fn span_of_expr(e: &Expr) -> Span {
    match e {
        Expr::IntLit { span, .. }
        | Expr::FloatLit { span, .. }
        | Expr::BoolLit { span, .. }
        | Expr::NullLit { span }
        | Expr::StrLit { span, .. }
        | Expr::Var { span, .. }
        | Expr::This { span }
        | Expr::TypeName { span, .. }
        | Expr::Member { span, .. }
        | Expr::Static { span, .. }
        | Expr::Call { span, .. }
        | Expr::Binary { span, .. }
        | Expr::Unary { span, .. }
        | Expr::Borrow { span, .. }
        | Expr::Index { span, .. }
        | Expr::Paren { span, .. }
        | Expr::Cast { span, .. }
        | Expr::Try { span, .. }
        | Expr::Match { span, .. }
        | Expr::Lambda { span, .. } => *span,
    }
}
