// SPDX-License-Identifier: MIT
//! Lower an AST [`TypeRef`] into a structural [`Ty`].
//!
//! Performs purely shape-level translation:
//! - Single-segment path that names a [`Primitive`] becomes
//!   `Ty::Primitive` (or `Ty::NullablePrimitive` if `?`-suffixed).
//! - Anything else becomes `Ty::Path { path, args, nullable }`,
//!   preserving generic arguments and the nullable suffix verbatim.
//!
//! Resolving whether `Path { path: ["app", "User"], ... }` actually
//! names a known class is the next pass; the structural lowering
//! deliberately stays unaware so it never blocks on missing
//! definitions.

use phc_ast::TypeRef;

use crate::{Primitive, Ty};

/// Lower an AST [`TypeRef`] into a structural [`Ty`].
///
/// Function types written `fn(P1, ..., Pn): R` (D-024) lower to a
/// `Ty::Path` keyed by `"fn"` with `args = [R, P1, ..., Pn]` —
/// return type first so consumers (codegen, hover, method-return
/// inference) can pluck it without scanning the whole vec. This is
/// an additive convention that piggybacks on the existing Ty shape;
/// dedicated `Ty::Function` is a future refactor.
pub fn lower_type_ref(tr: &TypeRef) -> Ty {
    if let Some(ret) = &tr.fn_return {
        let mut args = Vec::with_capacity(tr.args.len() + 1);
        args.push(lower_type_ref(ret));
        for p in &tr.args {
            args.push(lower_type_ref(p));
        }
        return Ty::Path {
            path: vec!["fn".to_string()],
            args,
            nullable: tr.nullable,
        };
    }
    if tr.path.len() == 1 && tr.args.is_empty() {
        if let Some(p) = Primitive::from_name(&tr.path[0].name) {
            return if tr.nullable {
                Ty::NullablePrimitive(p)
            } else {
                Ty::Primitive(p)
            };
        }
    }
    Ty::Path {
        path: tr.path.iter().map(|i| i.name.clone()).collect(),
        args: tr.args.iter().map(lower_type_ref).collect(),
        nullable: tr.nullable,
    }
}
