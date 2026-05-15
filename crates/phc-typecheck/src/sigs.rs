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

use phc_ast::{Borrow, ClassMember, FunctionDecl, Item, MethodSig as AstMethodSig, SourceFile};
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
        match item {
            Item::Function(f) => {
                if let Some(id) = resolved.top_level.get(&f.name.name).copied() {
                    typed.function_sigs.insert(id, lower_function_sig(f));
                }
            }
            Item::Class(c) => {
                let Some(class_id) = resolved.top_level.get(&c.name.name).copied() else {
                    continue;
                };
                let Some(members) = resolved.members_of.get(&class_id) else {
                    continue;
                };
                let class_ty = Ty::Path {
                    path: vec![c.name.name.clone()],
                    args: Vec::new(),
                    nullable: false,
                };
                // members_of skips TraitUse; iterate AST members in
                // lockstep, advancing a member-id cursor only when
                // the AST entry produced a symbol.
                let mut idx = 0;
                for member in &c.members {
                    match member {
                        ClassMember::Method(m) => {
                            if let Some(&id) = members.get(idx) {
                                typed.function_sigs.insert(id, lower_function_sig(m));
                            }
                            idx += 1;
                        }
                        ClassMember::Construct(con) => {
                            if let Some(&id) = members.get(idx) {
                                let params = con
                                    .params
                                    .iter()
                                    .map(|p| ParamSig {
                                        name: p.name.name.clone(),
                                        ty: lower_type_ref(&p.ty),
                                        borrow: p.borrow,
                                    })
                                    .collect();
                                typed.function_sigs.insert(
                                    id,
                                    FunctionSig {
                                        generic_params: Vec::new(),
                                        params,
                                        return_ty: class_ty.clone(),
                                    },
                                );
                            }
                            idx += 1;
                        }
                        ClassMember::Field(_) => {
                            idx += 1;
                        }
                        ClassMember::TraitUse(_) => {
                            // Not in members_of; do not advance.
                        }
                    }
                }
            }
            Item::Trait(t) => {
                let Some(trait_id) = resolved.top_level.get(&t.name.name).copied() else {
                    continue;
                };
                let Some(members) = resolved.members_of.get(&trait_id) else {
                    continue;
                };
                for (i, m) in t.methods.iter().enumerate() {
                    if let Some(&id) = members.get(i) {
                        typed.function_sigs.insert(id, lower_function_sig(m));
                    }
                }
            }
            Item::Interface(iface) => {
                let Some(iface_id) = resolved.top_level.get(&iface.name.name).copied() else {
                    continue;
                };
                let Some(members) = resolved.members_of.get(&iface_id) else {
                    continue;
                };
                for (i, sig) in iface.methods.iter().enumerate() {
                    if let Some(&id) = members.get(i) {
                        typed.function_sigs.insert(id, lower_method_sig(sig));
                    }
                }
            }
            Item::Enum(_) | Item::Test(_) => {}
        }
    }
}

fn lower_method_sig(sig: &AstMethodSig) -> FunctionSig {
    FunctionSig {
        generic_params: sig
            .generic_params
            .iter()
            .map(|gp| gp.name.name.clone())
            .collect(),
        params: sig
            .params
            .iter()
            .map(|p| ParamSig {
                name: p.name.name.clone(),
                ty: lower_type_ref(&p.ty),
                borrow: p.borrow,
            })
            .collect(),
        return_ty: lower_type_ref(&sig.return_type),
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
