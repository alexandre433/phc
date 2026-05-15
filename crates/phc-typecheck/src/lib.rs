// SPDX-License-Identifier: MIT
//! Type inference and checking for PHC.
//!
//! This first slice introduces the [`Ty`] enum and a lowering from
//! AST [`TypeRef`](phc_ast::TypeRef) into structural [`Ty`] values.
//! Function signatures, expression inference, and statement checks
//! land in follow-up commits.

mod lower;

#[cfg(test)]
mod tests;

pub use lower::lower_type_ref;

use phc_ast::Ident;
use phc_span::Span;

/// One canonical primitive type from spec D-022 (provisional list).
///
/// Names mirror the spec's lowercase D-006a casing.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Primitive {
    Int,
    Float,
    Byte,
    Bytes,
    Bool,
    String,
    Void,
}

impl Primitive {
    /// Try to recognise a single-segment type name as a primitive.
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "int" => Primitive::Int,
            "float" => Primitive::Float,
            "byte" => Primitive::Byte,
            "bytes" => Primitive::Bytes,
            "bool" => Primitive::Bool,
            "string" => Primitive::String,
            "void" => Primitive::Void,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Primitive::Int => "int",
            Primitive::Float => "float",
            Primitive::Byte => "byte",
            Primitive::Bytes => "bytes",
            Primitive::Bool => "bool",
            Primitive::String => "string",
            Primitive::Void => "void",
        }
    }
}

/// Structural representation of a type as it appears in source.
///
/// A `Ty` is the typechecker's working representation; it is
/// independent of the syntax-tree [`TypeRef`](phc_ast::TypeRef) so
/// later passes (substitution during generic instantiation, the IR)
/// can manipulate types without re-walking AST shapes.
///
/// "Resolved" semantics — i.e. checking that `app.User` actually
/// names a class — is a separate pass that runs once the pack-level
/// scope lands. For now `Path` is preserved verbatim.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum Ty {
    /// A canonical primitive (`int`, `string`, `void`, ...).
    Primitive(Primitive),
    /// A user-defined or stdlib non-primitive type. The path is the
    /// dotted segment list from the source; `args` is the type
    /// argument list (empty when there are no `< ... >` after the
    /// path). `nullable` reflects the `?` suffix.
    Path {
        path: Vec<String>,
        args: Vec<Ty>,
        nullable: bool,
    },
    /// `?`-suffixed primitive (`int?`, `string?`, ...). Stored as
    /// its own variant so `int` and `int?` never compare equal even
    /// though they share an inner [`Primitive`].
    NullablePrimitive(Primitive),
    /// A reference to a generic type parameter currently in scope
    /// (`T`, `U`). Kept opaque until generic resolution lands.
    Generic { name: String },
    /// Used internally when lowering fails. Acts as a top-level
    /// "unknown" so downstream passes can keep working without
    /// repeatedly emitting the same diagnostic.
    Unknown,
}

impl Ty {
    /// Convenience: `int` primitive.
    pub fn int() -> Self {
        Ty::Primitive(Primitive::Int)
    }
    /// Convenience: `string` primitive.
    pub fn string() -> Self {
        Ty::Primitive(Primitive::String)
    }
    /// Convenience: `bool` primitive.
    pub fn bool() -> Self {
        Ty::Primitive(Primitive::Bool)
    }
    /// Convenience: `void` primitive.
    pub fn void() -> Self {
        Ty::Primitive(Primitive::Void)
    }

    /// Render to a spec-shaped textual form (`list<int>?`,
    /// `app.User`, ...). Used by diagnostics and snapshots.
    pub fn display(&self) -> String {
        let mut buf = String::new();
        self.fmt_into(&mut buf);
        buf
    }

    fn fmt_into(&self, out: &mut String) {
        match self {
            Ty::Primitive(p) => out.push_str(p.as_str()),
            Ty::NullablePrimitive(p) => {
                out.push_str(p.as_str());
                out.push('?');
            }
            Ty::Generic { name } => out.push_str(name),
            Ty::Unknown => out.push_str("<unknown>"),
            Ty::Path {
                path,
                args,
                nullable,
            } => {
                for (i, seg) in path.iter().enumerate() {
                    if i > 0 {
                        out.push('.');
                    }
                    out.push_str(seg);
                }
                if !args.is_empty() {
                    out.push('<');
                    for (i, a) in args.iter().enumerate() {
                        if i > 0 {
                            out.push_str(", ");
                        }
                        a.fmt_into(out);
                    }
                    out.push('>');
                }
                if *nullable {
                    out.push('?');
                }
            }
        }
    }
}

/// Span-tagged identifier, kept here for callers that want to round-
/// trip a name through the typechecker without depending on `phc-ast`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NamedTy {
    pub name: Ident,
    pub ty: Ty,
    pub span: Span,
}
