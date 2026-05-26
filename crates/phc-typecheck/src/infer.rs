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
            let ret_ty = call_return_ty(callee, args, resolved, typed);
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
        Expr::Lambda {
            params, body, span, ..
        } => {
            // Seed the lambda's own parameters into the binding-type
            // table before walking the body so a `$n` inside refers
            // to its declared type, not Ty::Unknown. Each lambda
            // param carries a fresh SymbolId so no outer binding is
            // shadowed by the insert.
            for p in params {
                if let Some(sid) = symbol_at_def(resolved, p.name.span) {
                    bindings.insert(sid, lower_type_ref(&p.ty));
                }
            }
            match body {
                phc_ast::LambdaBody::Expr(e) => walk_expr(e, resolved, bindings, typed),
                phc_ast::LambdaBody::Block(b) => walk_block(b, resolved, bindings, typed),
            }
            // Record the lambda expression itself as the function
            // type `fn(P1, ..., Pn): R` so a callee like `($f)(args)`
            // for an inline lambda — or the lambda value used as an
            // argument to a higher-order method — has a concrete
            // static type to dispatch on. Inferred return type comes
            // from the explicit annotation (best signal); body-type
            // inference is a follow-up.
            let ret_ty = match return_type_ty(expr) {
                Some(t) => t,
                None => lambda_body_ty(body, typed).unwrap_or(Ty::Unknown),
            };
            let mut fn_args = vec![ret_ty];
            for p in params {
                fn_args.push(lower_type_ref(&p.ty));
            }
            record(
                typed,
                *span,
                Ty::Path {
                    path: vec!["fn".to_string()],
                    args: fn_args,
                    nullable: false,
                },
            );
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

/// Extract the explicit `: T` return type annotation off a
/// `Expr::Lambda`, lowered. Returns None when omitted.
fn return_type_ty(expr: &Expr) -> Option<Ty> {
    match expr {
        Expr::Lambda {
            return_type: Some(rt),
            ..
        } => Some(lower_type_ref(rt)),
        _ => None,
    }
}

/// Best-effort lambda body type when there is no explicit return
/// annotation. Block bodies are not statically reduced today — the
/// inferer doesn't yet walk `return` statements to pick one type —
/// so a Block body falls back to `Ty::Unknown` and the caller
/// records that. Expression bodies use the body expression's
/// recorded type.
fn lambda_body_ty(body: &phc_ast::LambdaBody, typed: &Typed) -> Option<Ty> {
    match body {
        phc_ast::LambdaBody::Expr(e) => typed.expr_types.get(&span_of(e)).cloned(),
        phc_ast::LambdaBody::Block(_) => None,
    }
}

/// Compute a Call expression's return type from its callee shape.
/// See the matching comment at the call site for the three cases.
fn call_return_ty(callee: &Expr, args: &[Expr], resolved: &Resolved, typed: &Typed) -> Ty {
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
        // D-032 / D-033: stdlib static calls return predictable
        // shapes that we hardcode here.
        Expr::Static { ty, member, .. } => {
            if let Expr::TypeName { name, .. } = ty.as_ref() {
                if name.name == "io" {
                    return match member.name.as_str() {
                        "print" | "println" | "eprint" | "eprintln" => {
                            Ty::Primitive(crate::Primitive::Void)
                        }
                        "readLine" => Ty::Path {
                            path: vec!["option".to_string()],
                            args: vec![Ty::Primitive(crate::Primitive::String)],
                            nullable: false,
                        },
                        _ => Ty::Unknown,
                    };
                }
                if name.name == "assert" {
                    return match member.name.as_str() {
                        "eq" | "neq" | "isTrue" | "isFalse" | "fail" | "approxEq" | "throws" => {
                            Ty::Primitive(crate::Primitive::Void)
                        }
                        _ => Ty::Unknown,
                    };
                }
                // D-034 numeric namespaces.
                if name.name == "int" {
                    return match member.name.as_str() {
                        "parse" => Ty::Path {
                            path: vec!["result".to_string()],
                            args: vec![
                                Ty::Primitive(crate::Primitive::Int),
                                Ty::Path {
                                    path: vec!["parseError".to_string()],
                                    args: Vec::new(),
                                    nullable: false,
                                },
                            ],
                            nullable: false,
                        },
                        "min" | "max" | "abs" | "pow" => Ty::Primitive(crate::Primitive::Int),
                        "toFloat" => Ty::Primitive(crate::Primitive::Float),
                        _ => Ty::Unknown,
                    };
                }
                if name.name == "float" {
                    return match member.name.as_str() {
                        "parse" => Ty::Path {
                            path: vec!["result".to_string()],
                            args: vec![
                                Ty::Primitive(crate::Primitive::Float),
                                Ty::Path {
                                    path: vec!["parseError".to_string()],
                                    args: Vec::new(),
                                    nullable: false,
                                },
                            ],
                            nullable: false,
                        },
                        "min" | "max" | "abs" | "sqrt" | "floor" | "ceil" | "round" => {
                            Ty::Primitive(crate::Primitive::Float)
                        }
                        "pow" => Ty::Primitive(crate::Primitive::Float),
                        "isNaN" => Ty::Primitive(crate::Primitive::Bool),
                        "toInt" => Ty::Primitive(crate::Primitive::Int),
                        _ => Ty::Unknown,
                    };
                }
            }
            let _ = args;
            Ty::Unknown
        }
        Expr::Member {
            receiver, field, ..
        } => {
            let recv_ty = match typed.expr_types.get(&span_of(receiver)) {
                Some(t) => t,
                None => return Ty::Unknown,
            };
            // 1. Stdlib method on a primitive / generic container.
            //    Some methods (D-029 closure forms) need the call
            //    args to recover the closure's return type, so the
            //    helper takes the args slice too.
            if let Some(t) = stdlib_method_return_ty(recv_ty, &field.name, args, typed) {
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
/// for D-025 (string), D-026 (result/option non-closure), D-027
/// (list), D-028 (map), D-029 (result/option closure forms).
///
/// `call_args` is the Call expression's argument list; the
/// closure-form methods (`map`, `andThen`, `okOr`) pull the
/// transformed type out of the callback argument's static type
/// (`fn(T): U` lowers to `Ty::Path { path:["fn"], args:[U, T] }`
/// so the U is `args[0]`).
fn stdlib_method_return_ty(
    recv_ty: &Ty,
    method: &str,
    call_args: &[Expr],
    typed: &Typed,
) -> Option<Ty> {
    let lambda_return = |idx: usize| -> Option<Ty> {
        let a = call_args.get(idx)?;
        match typed.expr_types.get(&span_of(a))? {
            Ty::Path { path, args, .. } if path.len() == 1 && path[0] == "fn" => {
                args.first().cloned()
            }
            _ => None,
        }
    };
    let arg_static_ty = |idx: usize| -> Option<Ty> {
        let a = call_args.get(idx)?;
        typed.expr_types.get(&span_of(a)).cloned()
    };
    match recv_ty {
        Ty::Primitive(crate::Primitive::String) => match method {
            "len" => Some(Ty::Primitive(crate::Primitive::Int)),
            "contains" | "startsWith" | "endsWith" => Some(Ty::Primitive(crate::Primitive::Bool)),
            "trim" | "upper" | "lower" => Some(Ty::Primitive(crate::Primitive::String)),
            // toInt returns result<int, parseError>. Mirrors
            // int::parse (see static-call inference above) so a
            // trailing `?` can propagate as expected.
            "toInt" => Some(Ty::Path {
                path: vec!["result".to_string()],
                args: vec![
                    Ty::Primitive(crate::Primitive::Int),
                    Ty::Path {
                        path: vec!["parseError".to_string()],
                        args: Vec::new(),
                        nullable: false,
                    },
                ],
                nullable: false,
            }),
            // D-047: split → list<string>; repeat → string;
            // indexOf → option<int>; replace → string.
            "split" => Some(Ty::Path {
                path: vec!["list".to_string()],
                args: vec![Ty::Primitive(crate::Primitive::String)],
                nullable: false,
            }),
            "repeat" => Some(Ty::Primitive(crate::Primitive::String)),
            "indexOf" => Some(Ty::Path {
                path: vec!["option".to_string()],
                args: vec![Ty::Primitive(crate::Primitive::Int)],
                nullable: false,
            }),
            "replace" => Some(Ty::Primitive(crate::Primitive::String)),
            _ => None,
        },
        Ty::Path { path, args, .. } if path.len() == 1 => {
            let elem = args.first().cloned();
            let err = args.get(1).cloned();
            match (path[0].as_str(), method) {
                ("list", "len") => Some(Ty::Primitive(crate::Primitive::Int)),
                ("list", "push") => Some(Ty::Primitive(crate::Primitive::Void)),
                ("list", "at") => elem.clone(),
                ("list", "forEach") => Some(Ty::Primitive(crate::Primitive::Void)),
                // D-030: list<T>.map(fn(T): U) → list<U>.
                ("list", "map") => {
                    let u = lambda_return(0).unwrap_or(Ty::Unknown);
                    Some(Ty::Path {
                        path: vec!["list".to_string()],
                        args: vec![u],
                        nullable: false,
                    })
                }
                // list<T>.filter(fn(T): bool) → list<T>.
                ("list", "filter") => Some(recv_ty.clone()),
                // D-037: fold returns U (from init's static type
                // — arg[0]); any/all return bool; find returns
                // option<T>.
                ("list", "fold") => arg_static_ty(0),
                ("list", "any") | ("list", "all") => Some(Ty::Primitive(crate::Primitive::Bool)),
                ("list", "find") => Some(Ty::Path {
                    path: vec!["option".to_string()],
                    args: vec![elem.clone().unwrap_or(Ty::Unknown)],
                    nullable: false,
                }),
                // D-038: reduce → option<T>; findIndex → option<int>.
                ("list", "reduce") => Some(Ty::Path {
                    path: vec!["option".to_string()],
                    args: vec![elem.clone().unwrap_or(Ty::Unknown)],
                    nullable: false,
                }),
                ("list", "findIndex") => Some(Ty::Path {
                    path: vec!["option".to_string()],
                    args: vec![Ty::Primitive(crate::Primitive::Int)],
                    nullable: false,
                }),
                // D-043: take / drop → list<T>.
                ("list", "take") | ("list", "drop") => Some(recv_ty.clone()),
                // D-044: reverse / concat → list<T>; join → string.
                ("list", "reverse") | ("list", "concat") => Some(recv_ty.clone()),
                ("list", "join") => Some(Ty::Primitive(crate::Primitive::String)),
                // D-045: sort(fn(T,T):int) → list<T>.
                ("list", "sort") => Some(recv_ty.clone()),
                // D-046: flatMap(fn(T): list<U>) → list<U>.
                ("list", "flatMap") => {
                    // lambda returns list<U>; that IS the output type.
                    Some(lambda_return(0).unwrap_or(Ty::Unknown))
                }
                ("map", "len") => Some(Ty::Primitive(crate::Primitive::Int)),
                ("map", "has") => Some(Ty::Primitive(crate::Primitive::Bool)),
                ("map", "set") => Some(Ty::Primitive(crate::Primitive::Void)),
                ("map", "get") => {
                    // Returns option<V> where V = args[1].
                    let v = args.get(1).cloned().unwrap_or(Ty::Unknown);
                    Some(Ty::Path {
                        path: vec!["option".to_string()],
                        args: vec![v],
                        nullable: false,
                    })
                }
                // D-039: map iteration helpers.
                ("map", "keys") => Some(Ty::Path {
                    path: vec!["list".to_string()],
                    args: vec![Ty::Primitive(crate::Primitive::String)],
                    nullable: false,
                }),
                ("map", "values") => {
                    let v = args.get(1).cloned().unwrap_or(Ty::Unknown);
                    Some(Ty::Path {
                        path: vec!["list".to_string()],
                        args: vec![v],
                        nullable: false,
                    })
                }
                // D-040: map forEach.
                ("map", "forEach") => Some(Ty::Primitive(crate::Primitive::Void)),
                // D-031: set<string> methods.
                ("set", "len") => Some(Ty::Primitive(crate::Primitive::Int)),
                ("set", "add") | ("set", "has") | ("set", "remove") => {
                    Some(Ty::Primitive(crate::Primitive::Bool))
                }
                // D-040: set forEach.
                ("set", "forEach") => Some(Ty::Primitive(crate::Primitive::Void)),
                ("result", "isOk") | ("result", "isErr") => {
                    Some(Ty::Primitive(crate::Primitive::Bool))
                }
                ("result", "unwrapOr") => elem,
                // D-029: result<T,E>.map(fn(T): U) → result<U, E>.
                ("result", "map") => {
                    let u = lambda_return(0).unwrap_or(Ty::Unknown);
                    let e = err.unwrap_or(Ty::Unknown);
                    Some(Ty::Path {
                        path: vec!["result".to_string()],
                        args: vec![u, e],
                        nullable: false,
                    })
                }
                // andThen: closure already returns result<U, E>, so
                // the receiver's return type is just the closure's
                // return type.
                ("result", "andThen") => Some(lambda_return(0).unwrap_or(Ty::Unknown)),
                ("result", "unwrap") => elem,
                ("option", "isSome") | ("option", "isNone") => {
                    Some(Ty::Primitive(crate::Primitive::Bool))
                }
                ("option", "unwrapOr") => elem,
                ("option", "orElse") => Some(recv_ty.clone()),
                // D-029: option<T>.map(fn(T): U) → option<U>.
                ("option", "map") => {
                    let u = lambda_return(0).unwrap_or(Ty::Unknown);
                    Some(Ty::Path {
                        path: vec!["option".to_string()],
                        args: vec![u],
                        nullable: false,
                    })
                }
                // option.andThen: closure returns option<U>; pass
                // through verbatim.
                ("option", "andThen") => Some(lambda_return(0).unwrap_or(Ty::Unknown)),
                // option<T>.okOr(E) → result<T, E>; E from arg type.
                ("option", "okOr") => {
                    let e = arg_static_ty(0).unwrap_or(Ty::Unknown);
                    let t = elem.unwrap_or(Ty::Unknown);
                    Some(Ty::Path {
                        path: vec!["result".to_string()],
                        args: vec![t, e],
                        nullable: false,
                    })
                }
                ("option", "unwrap") => elem,
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
