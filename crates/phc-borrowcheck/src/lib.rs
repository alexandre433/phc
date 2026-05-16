// SPDX-License-Identifier: MIT
//! Borrow checking and mutability enforcement for PHC.
//!
//! MVP scope (D-005, D-005a, D-018):
//! - `:=` reassignment requires the target binding be declared `flip`.
//! - `$obj->field = expr;` member-assign requires the target field
//!   be declared `flip` on the receiver's class.
//! - `&flip $x` mutable borrow requires `$x` itself be declared
//!   `flip` (an immutable binding cannot be borrowed mutably).
//!
//! Out of scope for MVP — tracked as follow-ups:
//! - Aliasing rules (no shared+mutable simultaneously, at most one
//!   mutable borrow live).
//! - Lambda capture-mode inference and the matching `flip` check on
//!   captured outer bindings (D-016).
//! - Lifetime inference / outlives reasoning.
//!
//! The pass is read-only over the AST and resolver. It accumulates
//! diagnostics rather than aborting on the first failure.

mod check;

#[cfg(test)]
mod tests;

use phc_ast::SourceFile;
use phc_errors::Diagnostic;
use phc_semantic::Resolved;
use phc_typecheck::Typed;

/// Result of running the borrow checker over one source file.
#[derive(Debug, Default)]
pub struct Borrowed {
    pub diagnostics: Vec<Diagnostic>,
}

/// Entry point. The checker reads the resolver's symbol/uses tables
/// to map every `$name` reference back to its declaring binding, and
/// reads the typechecker's expression-type table to map member-assign
/// receivers back to their class so the field's `flip` flag can be
/// inspected.
pub fn borrowcheck(file: &SourceFile, resolved: &Resolved, typed: &Typed) -> Borrowed {
    check::run(file, resolved, typed)
}
