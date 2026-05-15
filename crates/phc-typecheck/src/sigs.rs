// SPDX-License-Identifier: MIT
//! Function-signature collection.
//!
//! Walks every top-level [`FunctionDecl`] and records its lowered
//! signature (parameter types + return type + generic parameter
//! names) in [`Typed::function_sigs`], keyed by the [`SymbolId`]
//! the resolver assigned.
//!
//! Class methods and trait methods are deferred until the resolver
//! produces SymbolIds for them — today only top-level items have
//! ids, so this pass restricts itself to free functions.

use phc_ast::{Borrow, FunctionDecl, Item, SourceFile};
use phc_semantic::{Resolved, SymbolId};

use crate::{lower::lower_type_ref, Ty, Typed};

/// One parameter as the typechecker sees it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParamSig {
    pub name: String,
    pub ty: Ty,
    pub borrow: Borrow,
}

/// Full signature of a free function.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FunctionSig {
    pub generic_params: Vec<String>,
    pub params: Vec<ParamSig>,
    pub return_ty: Ty,
}

pub fn collect_function_sigs(file: &SourceFile, resolved: &Resolved, typed: &mut Typed) {
    for item in &file.items {
        if let Item::Function(f) = item {
            let id = match resolved.top_level.get(&f.name.name).copied() {
                Some(id) => id,
                None => continue,
            };
            let sig = lower_function_sig(f);
            typed.function_sigs.insert(id, sig);
        }
    }
}

fn lower_function_sig(f: &FunctionDecl) -> FunctionSig {
    let generic_params = f
        .generic_params
        .iter()
        .map(|gp| gp.name.name.clone())
        .collect();
    let params = f
        .params
        .iter()
        .map(|p| ParamSig {
            name: p.name.name.clone(),
            ty: lower_type_ref(&p.ty),
            borrow: p.borrow,
        })
        .collect();
    FunctionSig {
        generic_params,
        params,
        return_ty: lower_type_ref(&f.return_type),
    }
}

#[allow(dead_code)]
fn _id_marker() -> Option<SymbolId> {
    None
}
