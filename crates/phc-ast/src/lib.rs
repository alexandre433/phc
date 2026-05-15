// SPDX-License-Identifier: MIT
//! Abstract syntax tree node definitions for PHC.
//!
//! Mirrors the productions in `spec/grammar.ebnf`. Every node carries
//! a [`Span`] tied to the originating source so diagnostics and IDE
//! tooling can underline the exact bytes that produced it.
//!
//! Nodes are added in lockstep with the parser. Anything not yet
//! parsed deliberately has no AST yet — see TODO comments.

use phc_span::Span;

/// A single PHC source file.
///
/// Grammar: `SourceFile = PackDecl { UseDecl } { Item }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    pub pack: PackDecl,
    pub uses: Vec<UseDecl>,
    pub items: Vec<Item>,
    pub span: Span,
}

/// `pack a.b.c;` declaration at the top of every source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackDecl {
    pub path: PackPath,
    pub span: Span,
}

/// `use a.b.C;` or `use a.b.{C, D};` import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UseDecl {
    pub path: PackPath,
    /// `Some(names)` for grouped imports `{ A, B }`; `None` for the
    /// single-item `use a.b.C;` form, in which case the last segment
    /// of `path` names the imported item.
    pub group: Option<Vec<Ident>>,
    pub span: Span,
}

/// Dot-separated path, used by `pack` and `use` declarations and by
/// type references.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackPath {
    pub segments: Vec<Ident>,
    pub span: Span,
}

/// A single identifier with its source span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

/// A type reference appearing in source (parameter type, return
/// type, field type, generic argument, ...).
///
/// Grammar: `Type = TypePath [ TypeArgs ] [ Nullable ]`. The path
/// segments are dot-separated identifiers (`int`, `app.User`). Type
/// arguments are themselves `TypeRef`s, forming a tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeRef {
    pub path: Vec<Ident>,
    pub args: Vec<TypeRef>,
    pub nullable: bool,
    pub span: Span,
}

/// A generic parameter on a function, class, interface, or trait.
///
/// Grammar: `GenericParam = Identifier [ ":" BoundList ]` where
/// `BoundList = TypePath { "+" TypePath }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenericParam {
    pub name: Ident,
    /// Each bound is a `TypePath`; reusing `Vec<Ident>` keeps the
    /// shape uniform with [`TypeRef::path`].
    pub bounds: Vec<Vec<Ident>>,
    pub span: Span,
}

/// Top-level item: function, class, enum, interface, trait, or test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    Function(FunctionDecl),
    Class(ClassDecl),
    Enum(EnumDecl),
    Interface(InterfaceDecl),
    Trait(TraitDecl),
    Test(TestDecl),
}

/// `[public] class Name<T> [implements I, J] { ... }` (D-002, D-012).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassDecl {
    pub visibility: Visibility,
    pub name: Ident,
    pub generic_params: Vec<GenericParam>,
    pub implements: Vec<Vec<Ident>>,
    pub members: Vec<ClassMember>,
    pub span: Span,
}

/// One member inside a class body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClassMember {
    Construct(ConstructDecl),
    TraitUse(TraitUse),
    Field(FieldDecl),
    Method(FunctionDecl),
}

/// `construct(<params>) { ... }` — at most one per class in v0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstructDecl {
    pub params: Vec<ConstructParam>,
    pub body: Block,
    pub span: Span,
}

/// A constructor parameter, optionally promoted to a field by a
/// leading `public` (D-012).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstructParam {
    pub promoted: bool,
    pub borrow: Borrow,
    pub is_mut: bool,
    pub ty: TypeRef,
    pub name: Ident,
    pub span: Span,
}

/// `use <TypePath>;` inside a class body — mixes in a trait (D-013).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraitUse {
    pub path: Vec<Ident>,
    pub span: Span,
}

/// `[public] [flip] <Type> $<name> [= expr] [HookBlock] ;` (D-018).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDecl {
    pub visibility: Visibility,
    pub is_mut: bool,
    pub ty: TypeRef,
    pub name: Ident,
    pub default: Option<Expr>,
    pub hooks: Vec<PropertyHook>,
    pub span: Span,
}

/// One hook inside a [`FieldDecl::hooks`] list (D-018).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropertyHook {
    /// `get => expr;` short form.
    GetExpr { expr: Expr, span: Span },
    /// `get { ... }` block form.
    GetBlock { body: Block, span: Span },
    /// `set(Type $name) { ... }`.
    Set {
        param_ty: TypeRef,
        param_name: Ident,
        body: Block,
        span: Span,
    },
}

/// `[public] enum Name [: Backing] { Variant [= literal], ... }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumDecl {
    pub visibility: Visibility,
    pub name: Ident,
    pub backing: Option<TypeRef>,
    pub variants: Vec<EnumVariant>,
    pub span: Span,
}

/// One variant of an [`EnumDecl`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumVariant {
    pub name: Ident,
    pub value: Option<Expr>,
    pub span: Span,
}

/// `[public] interface Name<T> { <MethodSig>... }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceDecl {
    pub visibility: Visibility,
    pub name: Ident,
    pub generic_params: Vec<GenericParam>,
    pub methods: Vec<MethodSig>,
    pub span: Span,
}

/// A method signature inside an [`InterfaceDecl`] — same shape as a
/// [`FunctionDecl`] header but with no body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodSig {
    pub name: Ident,
    pub generic_params: Vec<GenericParam>,
    pub params: Vec<Param>,
    pub return_type: TypeRef,
    pub span: Span,
}

/// `[public] trait Name<T> { <FunctionDecl>... }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraitDecl {
    pub visibility: Visibility,
    pub name: Ident,
    pub generic_params: Vec<GenericParam>,
    pub methods: Vec<FunctionDecl>,
    pub span: Span,
}

/// `test "name" { ... }` — provisional surface (D-021).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestDecl {
    pub name: Vec<StrPart>,
    pub body: Block,
    pub span: Span,
}

/// `[public] [async] function name<T>(p1, p2): RetType { ... }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionDecl {
    pub visibility: Visibility,
    pub is_async: bool,
    pub name: Ident,
    pub generic_params: Vec<GenericParam>,
    pub params: Vec<Param>,
    pub return_type: TypeRef,
    pub body: Block,
    pub span: Span,
}

/// Visibility marker (D-008). Only two levels in v0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    /// Pack-scoped — visible to every file in the same pack only.
    Default,
    /// Cross-pack — visible to any pack that imports the item.
    Public,
}

/// A parameter on a function or lambda.
///
/// Grammar: `Param = [ BorrowMod ] Type VarRef`. The `$` sigil on
/// the variable name is consumed by the lexer; the parser stores
/// only the bare identifier in [`Self::name`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    pub borrow: Borrow,
    pub ty: TypeRef,
    pub name: Ident,
    pub span: Span,
}

/// Borrow modifier on a parameter or expression operand (D-005).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Borrow {
    /// No leading `&`. Owned-by-value at the call boundary.
    None,
    /// Leading `&` only — shared (read) borrow.
    Shared,
    /// Leading `&flip` — mutable (exclusive) borrow.
    Mutable,
}

/// A `{ ... }` block.
///
/// Statements land in P5 alongside the rest of the statement
/// grammar. For now a `Block` is purely the brace pair plus its
/// span, which is enough to round-trip a function declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub statements: Vec<Stmt>,
    pub span: Span,
}

/// A statement inside a [`Block`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt {
    Local(LocalBinding),
    Reassign(ReassignStmt),
    /// `<member-chain> = <expr>;` — D-005a member-field write.
    MemberAssign(MemberAssignStmt),
    If(IfStmt),
    While(WhileStmt),
    For(ForStmt),
    Return(ReturnStmt),
    Break {
        span: Span,
    },
    Continue {
        span: Span,
    },
    Expr(ExprStmt),
}

/// `[flip] <Type> $<name> = <expr>;` (D-005, D-010).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalBinding {
    pub is_mut: bool,
    pub ty: TypeRef,
    pub name: Ident,
    pub value: Expr,
    pub span: Span,
}

/// `<lhs> := <expr>;` (D-005). The parser validates that `lhs` is a
/// [`Expr::Var`] or a chain of [`Expr::Member`] rooted at one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReassignStmt {
    pub lhs: Expr,
    pub value: Expr,
    pub span: Span,
}

/// `<member-chain> = <expr>;` (D-005a). The LHS must be an
/// [`Expr::Member`] rooted at a [`Expr::Var`] or [`Expr::This`];
/// bare `$x = ...` (no `->`) is rejected with a diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberAssignStmt {
    pub lhs: Expr,
    pub value: Expr,
    pub span: Span,
}

/// `if (cond) { ... } { else if (cond) { ... } } [ else { ... } ]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IfStmt {
    /// Each entry is one `(condition, block)`. The first is the
    /// leading `if`; the rest are `else if` clauses in source order.
    pub branches: Vec<(Expr, Block)>,
    pub else_block: Option<Block>,
    pub span: Span,
}

/// `while (cond) { ... }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhileStmt {
    pub cond: Expr,
    pub body: Block,
    pub span: Span,
}

/// `for (Type $name in iter) { ... }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForStmt {
    pub elem_ty: TypeRef,
    pub elem_name: Ident,
    pub iter: Expr,
    pub body: Block,
    pub span: Span,
}

/// `return [<expr>];`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReturnStmt {
    pub value: Option<Expr>,
    pub span: Span,
}

/// `<expr>;` — value discarded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExprStmt {
    pub expr: Expr,
    pub span: Span,
}

/// Expression node. Mirrors every Expression production in the EBNF
/// (precedence is encoded in how the parser nests these, not in the
/// enum itself). Match expressions and lambdas live here as well
/// once P7 lands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    /// Integer literal text exactly as it appeared in source. The
    /// parser does not interpret the value; the typechecker does.
    IntLit {
        text: String,
        span: Span,
    },
    FloatLit {
        text: String,
        span: Span,
    },
    BoolLit {
        value: bool,
        span: Span,
    },
    NullLit {
        span: Span,
    },
    /// Double-quoted string literal split into resolved text chunks
    /// and parsed interpolation expressions (D-017).
    StrLit {
        parts: Vec<StrPart>,
        span: Span,
    },
    /// `$this`.
    This {
        span: Span,
    },
    /// `$name` (variable / parameter / field reference).
    Var {
        name: Ident,
        span: Span,
    },
    /// A bare type name appearing in expression position. Acts as
    /// the head of a static-access chain (`Status::Ok`).
    TypeName {
        name: Ident,
        span: Span,
    },
    /// `(expr)` — parenthesised grouping, kept in the AST so spans
    /// and pretty-printing round-trip the source.
    Paren {
        inner: Box<Expr>,
        span: Span,
    },
    /// `receiver->field` (instance member access, D-023).
    Member {
        receiver: Box<Expr>,
        field: Ident,
        span: Span,
    },
    /// `Type::member` (static / type-level access, D-023).
    Static {
        ty: Box<Expr>,
        member: Ident,
        span: Span,
    },
    /// `callee(arg, ...)`.
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
        span: Span,
    },
    /// `target[index]`.
    Index {
        target: Box<Expr>,
        index: Box<Expr>,
        span: Span,
    },
    /// `value?` — postfix Result/Option propagation (D-006a').
    /// On success yields the inner payload; on failure short-circuits
    /// the enclosing function with the failure variant.
    Try {
        value: Box<Expr>,
        span: Span,
    },
    /// Unary prefix operator (`!`, `-`, `await`).
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
        span: Span,
    },
    /// Borrow prefix on an operand (`&$x`, `&flip $x`).
    Borrow {
        kind: Borrow,
        operand: Box<Expr>,
        span: Span,
    },
    /// `value as Type` (D-019; total casts only).
    Cast {
        value: Box<Expr>,
        ty: TypeRef,
        span: Span,
    },
    /// Binary infix operator (every level from `*` through `??`).
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
        span: Span,
    },
    /// `match (scrutinee) { arm, ... }` (D-015).
    Match {
        scrutinee: Box<Expr>,
        arms: Vec<MatchArm>,
        span: Span,
    },
    /// `(params) [: T] => body` (D-016).
    Lambda {
        params: Vec<Param>,
        return_type: Option<TypeRef>,
        body: LambdaBody,
        span: Span,
    },
}

/// One arm of a [`Expr::Match`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub guard: Option<Expr>,
    pub body: Expr,
    pub span: Span,
}

/// A pattern in a `match` arm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pattern {
    Wildcard {
        span: Span,
    },
    /// Literal pattern. Stored as the literal `Expr` for span and
    /// payload reuse; the parser only places literal-shaped exprs
    /// here.
    Literal(Box<Expr>),
    /// `$name` pattern that binds the scrutinee to a fresh name.
    Var {
        name: Ident,
        span: Span,
    },
    /// `Type::Variant` pattern matching one enum case.
    EnumVariant {
        ty: Ident,
        variant: Ident,
        span: Span,
    },
    /// `pat1 | pat2 | ...` — at least two atoms.
    Or {
        atoms: Vec<Pattern>,
        span: Span,
    },
}

/// Body of a [`Expr::Lambda`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LambdaBody {
    /// `(params) => expr` — single expression body.
    Expr(Box<Expr>),
    /// `(params) => { stmts }` — block body, may use `return`.
    Block(Block),
}

/// A piece of a string literal (D-017).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StrPart {
    /// Literal text with all escapes already resolved.
    Text(String),
    /// Embedded `{ expr }` interpolation, parsed into an [`Expr`].
    Expr(Expr),
}

/// Unary prefix operators that are *not* borrow modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Not,
    Neg,
    Await,
}

/// Binary infix operators, ordered top-of-table → bottom-of-table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Mul,
    Div,
    Rem,
    Add,
    Sub,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Neq,
    And,
    Or,
    NullCoalesce,
}
