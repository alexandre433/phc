// SPDX-License-Identifier: MIT
//! Tree-walking interpreter for typed PHC programs.
//!
//! Walks the [`SourceFile`](phc_ast::SourceFile) directly using the
//! [`Resolved`](phc_semantic::Resolved) name-resolution table and
//! [`Typed`](phc_typecheck::Typed) signature table. Interim path to
//! a runnable PHC: lets the language be exercised end-to-end while
//! codegen + runtime are built in the background.
//!
//! Scope today: enough to run `examples/hello.phc`. Each follow-up
//! commit (I2..I7) adds one feature axis (control flow, classes,
//! enums, lambdas, result, async).
//!
//! Builtins surface the smallest stdlib stub the corpus needs. The
//! current set is `Logger::info(string)` → eprintln. More land
//! alongside the matching language feature.

use phc_ast::{
    BinOp, ClassDecl, ClassMember, ConstructDecl, EnumDecl, Expr, FunctionDecl, Item, LambdaBody,
    MatchArm, Param, Pattern, SourceFile, Stmt, StrPart, TraitDecl, UnaryOp,
};
use phc_semantic::{Resolved, Symbol, SymbolId, SymbolKind};
use phc_typecheck::Typed;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// Runtime value carried by the interpreter.
#[derive(Clone, Debug)]
pub enum Value {
    Void,
    Null,
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    /// Class instance. Field storage is shared (`Rc<RefCell<...>>`)
    /// so two bindings holding the same instance see each other's
    /// mutations — matching PHP's by-reference object semantics.
    Instance {
        class: String,
        fields: Rc<RefCell<HashMap<String, Value>>>,
    },
    /// `Type::Variant` value. Carries both the enum name and the
    /// variant name so equality only matches when both agree.
    EnumVariant {
        enum_name: String,
        variant: String,
    },
    /// Lambda value. Captures the env (by value) at the lambda's
    /// creation site so call-time evaluation sees the same set of
    /// outer bindings the body referenced. Mutable capture (D-016)
    /// is approximated by value-copy today; rewriting an outer
    /// `flip` binding from inside the lambda is not yet observable
    /// outside.
    Lambda(Rc<LambdaValue>),
    /// `result::ok(v)` — success branch carrying `v`.
    ResultOk(Box<Value>),
    /// `result::err(e)` — failure branch carrying `e`.
    ResultErr(Box<Value>),
    /// `option::some(v)`.
    OptionSome(Box<Value>),
    /// `option::none`.
    OptionNone,
    /// `list<T>` value (D-027). Reference-semantics: cloning a
    /// Value::List clones the Rc, not the Vec — two bindings hold
    /// the same backing storage and observe each other's pushes.
    /// CoW lands when the runtime grows refcounts.
    List(Rc<RefCell<Vec<Value>>>),
    /// `map<string, V>` value (D-028). Same reference-semantics as
    /// `List`. v0a only supports string keys; the storage type
    /// reflects that. Generic-key maps land when key hashing is
    /// speced.
    Map(Rc<RefCell<Vec<(String, Value)>>>),
    /// `set<string>` value (D-031). Same shape rules as Map; only
    /// the entry stores no payload.
    Set(Rc<RefCell<Vec<String>>>),
}

/// Owned lambda payload. Stored behind an Rc so cloning a Value is
/// cheap and lambda values can be passed around by reference.
#[derive(Debug)]
pub struct LambdaValue {
    pub params: Vec<Param>,
    pub body: LambdaBody,
    /// Snapshot of every binding visible at the lambda creation
    /// site, used to seed the call-time env.
    pub captures: HashMap<SymbolId, Value>,
}

impl Value {
    pub fn display(&self) -> String {
        match self {
            Value::Void => "void".to_string(),
            Value::Null => "null".to_string(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) => format!("{f}"),
            Value::Bool(b) => b.to_string(),
            Value::String(s) => s.clone(),
            Value::Instance { class, .. } => format!("<{class} instance>"),
            Value::EnumVariant { enum_name, variant } => format!("{enum_name}::{variant}"),
            Value::Lambda(_) => "<lambda>".to_string(),
            Value::ResultOk(v) => format!("result::ok({})", v.display()),
            Value::ResultErr(e) => format!("result::err({})", e.display()),
            Value::OptionSome(v) => format!("option::some({})", v.display()),
            Value::OptionNone => "option::none".to_string(),
            Value::List(items) => {
                let parts: Vec<String> = items.borrow().iter().map(|v| v.display()).collect();
                format!("[{}]", parts.join(", "))
            }
            Value::Map(entries) => {
                let parts: Vec<String> = entries
                    .borrow()
                    .iter()
                    .map(|(k, v)| format!("{k}: {}", v.display()))
                    .collect();
                format!("{{{}}}", parts.join(", "))
            }
            Value::Set(keys) => {
                let parts: Vec<String> = keys.borrow().iter().cloned().collect();
                format!("#{{{}}}", parts.join(", "))
            }
        }
    }
}

/// Out-of-band control flow result for statement evaluation.
enum Flow {
    Normal,
    Return(Value),
    Break,
    Continue,
}

/// Result of running a program. Captures everything stdout would
/// see plus any runtime errors.
#[derive(Debug, Default)]
pub struct RunOutput {
    /// Lines emitted by builtins like `Logger::info` (one per call).
    pub stdout: Vec<String>,
    /// The value returned by `main()`. `Value::Void` for `void`
    /// returns; `Value::Null` if `main` was not present.
    pub result: Option<Value>,
    /// Runtime errors encountered. The interpreter does not unwind
    /// past a panic today; the first error stops execution.
    pub errors: Vec<RuntimeError>,
}

/// One runtime error.
#[derive(Debug, Clone)]
pub struct RuntimeError {
    pub message: String,
}

/// Internal error type. Real failures are [`EvalError::Runtime`];
/// `?` propagation surfaces as [`EvalError::Propagate`] carrying the
/// failure value (`result::err(e)` or `option::none`) so the
/// enclosing function can short-circuit with the right return.
#[derive(Debug, Clone)]
enum EvalError {
    Runtime(RuntimeError),
    Propagate(Value),
}

impl From<RuntimeError> for EvalError {
    fn from(e: RuntimeError) -> Self {
        EvalError::Runtime(e)
    }
}

type EvalResult<T> = Result<T, EvalError>;

/// Build an `EvalError::Runtime` with the given message. Most call
/// sites use this rather than spelling the variant by hand.
fn rt(msg: impl Into<String>) -> EvalError {
    EvalError::Runtime(RuntimeError {
        message: msg.into(),
    })
}

/// Per-call frame. Keeps a flat name → value map for the
/// currently-executing function body.
#[derive(Default)]
struct Env {
    bindings: Vec<HashMap<SymbolId, Value>>,
}

impl Env {
    fn enter(&mut self) {
        self.bindings.push(HashMap::new());
    }

    fn leave(&mut self) {
        self.bindings.pop();
    }

    fn bind(&mut self, id: SymbolId, value: Value) {
        if let Some(top) = self.bindings.last_mut() {
            top.insert(id, value);
        }
    }

    fn lookup(&self, id: SymbolId) -> Option<&Value> {
        for scope in self.bindings.iter().rev() {
            if let Some(v) = scope.get(&id) {
                return Some(v);
            }
        }
        None
    }

    /// Flatten the visible bindings into a single map. Inner-frame
    /// values shadow outer-frame ones, matching `lookup` order.
    /// Used by lambda capture to snapshot the env at creation time.
    fn flatten(&self) -> HashMap<SymbolId, Value> {
        let mut out = HashMap::new();
        for scope in &self.bindings {
            for (id, v) in scope {
                out.insert(*id, v.clone());
            }
        }
        out
    }

    /// Overwrite an existing binding in whichever enclosing scope
    /// declared it. Used by `:=` reassignment so the write reaches
    /// the originally-declared `flip` binding instead of shadowing
    /// it in the current frame.
    fn assign(&mut self, id: SymbolId, value: Value) -> bool {
        for scope in self.bindings.iter_mut().rev() {
            if let std::collections::hash_map::Entry::Occupied(mut e) = scope.entry(id) {
                e.insert(value);
                return true;
            }
        }
        false
    }
}

/// Driver: resolve `main` in the file, evaluate its body, return
/// what came out.
/// Run a single `test "name" { ... }` body and report the outcome.
/// Used by the test framework (D-021) to drive each discovered
/// `Item::Test` independently. Any runtime error is the test's
/// failure; clean exit is a pass. Captured stdout flows through
/// the returned [`RunOutput`] so the runner can print or attach it
/// to a per-test report.
pub fn run_test_block(
    file: &SourceFile,
    resolved: &Resolved,
    typed: &Typed,
    test_body: &phc_ast::Block,
) -> RunOutput {
    let mut interp = Interp {
        file,
        resolved,
        typed,
        out: RunOutput::default(),
    };
    let mut env = Env::default();
    env.enter();
    let flow = interp.eval_block_body(&test_body.statements, &mut env);
    env.leave();
    match flow {
        Ok(Flow::Return(v)) => {
            interp.out.result = Some(v);
        }
        Ok(Flow::Normal) => {
            interp.out.result = Some(Value::Void);
        }
        Ok(Flow::Break) | Ok(Flow::Continue) => {
            interp.out.errors.push(RuntimeError {
                message: "`break`/`continue` outside loop".to_string(),
            });
        }
        Err(EvalError::Runtime(e)) => {
            interp.out.errors.push(e);
        }
        Err(EvalError::Propagate(v)) => {
            interp.out.result = Some(v);
        }
    }
    interp.out
}

pub fn run(file: &SourceFile, resolved: &Resolved, typed: &Typed) -> RunOutput {
    let mut out = RunOutput::default();
    let main_id = match resolved.top_level.get("main").copied() {
        Some(id) => id,
        None => {
            out.errors.push(RuntimeError {
                message: "no `main` function defined in this source file".to_string(),
            });
            return out;
        }
    };
    let main_decl = find_function(file, &resolved.symbols, main_id);
    let Some(main) = main_decl else {
        out.errors.push(RuntimeError {
            message: "`main` was registered but its body was not found".to_string(),
        });
        return out;
    };
    let mut interp = Interp {
        file,
        resolved,
        typed,
        out: RunOutput::default(),
    };
    let mut env = Env::default();
    env.enter();
    let flow = interp.eval_block_body(&main.body.statements, &mut env);
    env.leave();
    let result_value = match flow {
        Ok(Flow::Return(v)) => v,
        Ok(Flow::Normal) => Value::Void,
        Ok(Flow::Break) | Ok(Flow::Continue) => {
            interp.out.errors.push(RuntimeError {
                message: "`break`/`continue` outside loop".to_string(),
            });
            Value::Null
        }
        Err(EvalError::Runtime(e)) => {
            interp.out.errors.push(e);
            Value::Null
        }
        Err(EvalError::Propagate(v)) => {
            // `?` short-circuited from `main` itself; surface the
            // propagated value as the program's result.
            v
        }
    };
    interp.out.result = Some(result_value);
    interp.out
}

fn find_function<'a>(
    file: &'a SourceFile,
    symbols: &[Symbol],
    id: SymbolId,
) -> Option<&'a FunctionDecl> {
    let symbol = symbols.get(id.0 as usize)?;
    if symbol.kind != SymbolKind::Function {
        return None;
    }
    file.items.iter().find_map(|item| match item {
        Item::Function(f) if f.name.span == symbol.def_span => Some(f),
        _ => None,
    })
}

fn find_class<'a>(file: &'a SourceFile, name: &str) -> Option<&'a ClassDecl> {
    file.items.iter().find_map(|item| match item {
        Item::Class(c) if c.name.name == name => Some(c),
        _ => None,
    })
}

fn find_enum<'a>(file: &'a SourceFile, name: &str) -> Option<&'a EnumDecl> {
    file.items.iter().find_map(|item| match item {
        Item::Enum(e) if e.name.name == name => Some(e),
        _ => None,
    })
}

fn find_trait<'a>(file: &'a SourceFile, name: &str) -> Option<&'a TraitDecl> {
    file.items.iter().find_map(|item| match item {
        Item::Trait(t) if t.name.name == name => Some(t),
        _ => None,
    })
}

fn find_method_in_class<'a>(class: &'a ClassDecl, name: &str) -> Option<&'a FunctionDecl> {
    class.members.iter().find_map(|m| match m {
        ClassMember::Method(f) if f.name.name == name => Some(f),
        _ => None,
    })
}

fn find_constructor(class: &ClassDecl) -> Option<&ConstructDecl> {
    class.members.iter().find_map(|m| match m {
        ClassMember::Construct(c) => Some(c),
        _ => None,
    })
}

/// Search the trait `use Trait;` mixins on a class for a method.
/// Linear scan; the first matching trait wins. Conflict detection
/// (D-013: two traits with the same method = error) is the
/// resolver's job, so the interpreter simply returns the first.
fn find_method_in_traits<'a>(
    file: &'a SourceFile,
    class: &'a ClassDecl,
    name: &str,
) -> Option<&'a FunctionDecl> {
    for member in &class.members {
        if let ClassMember::TraitUse(trait_use) = member {
            let trait_name = trait_use.path.last()?.name.as_str();
            if let Some(t) = find_trait(file, trait_name) {
                if let Some(m) = t.methods.iter().find(|m| m.name.name == name) {
                    return Some(m);
                }
            }
        }
    }
    None
}

struct Interp<'a> {
    // file + typed are unused at the I1 frontier; they will drive
    // function call dispatch (I2) and method lookup (I3).
    #[allow(dead_code)]
    file: &'a SourceFile,
    resolved: &'a Resolved,
    #[allow(dead_code)]
    typed: &'a Typed,
    out: RunOutput,
}

impl<'a> Interp<'a> {
    fn eval_block_body(&mut self, stmts: &[Stmt], env: &mut Env) -> EvalResult<Flow> {
        for stmt in stmts {
            match self.eval_stmt(stmt, env)? {
                Flow::Normal => {}
                other => return Ok(other),
            }
        }
        Ok(Flow::Normal)
    }

    fn eval_block_scoped(&mut self, stmts: &[Stmt], env: &mut Env) -> EvalResult<Flow> {
        env.enter();
        let flow = self.eval_block_body(stmts, env);
        env.leave();
        flow
    }

    fn eval_stmt(&mut self, stmt: &Stmt, env: &mut Env) -> EvalResult<Flow> {
        match stmt {
            Stmt::Local(b) => {
                let value = self.eval_expr(&b.value, env)?;
                if let Some(sid) = self.symbol_at_def(b.name.span) {
                    env.bind(sid, value);
                }
                Ok(Flow::Normal)
            }
            Stmt::Reassign(r) => {
                let value = self.eval_expr(&r.value, env)?;
                self.do_assign(&r.lhs, value, env)?;
                Ok(Flow::Normal)
            }
            Stmt::MemberAssign(m) => {
                let value = self.eval_expr(&m.value, env)?;
                self.do_member_assign(&m.lhs, value, env)?;
                Ok(Flow::Normal)
            }
            Stmt::If(i) => {
                for (cond, blk) in &i.branches {
                    let cv = self.eval_expr(cond, env)?;
                    if truthy(&cv)? {
                        return self.eval_block_scoped(&blk.statements, env);
                    }
                }
                if let Some(else_blk) = &i.else_block {
                    return self.eval_block_scoped(&else_blk.statements, env);
                }
                Ok(Flow::Normal)
            }
            Stmt::While(w) => loop {
                let cv = self.eval_expr(&w.cond, env)?;
                if !truthy(&cv)? {
                    return Ok(Flow::Normal);
                }
                match self.eval_block_scoped(&w.body.statements, env)? {
                    Flow::Normal | Flow::Continue => continue,
                    Flow::Break => return Ok(Flow::Normal),
                    Flow::Return(v) => return Ok(Flow::Return(v)),
                }
            },
            Stmt::Break { .. } => Ok(Flow::Break),
            Stmt::Continue { .. } => Ok(Flow::Continue),
            Stmt::Expr(e) => {
                self.eval_expr(&e.expr, env)?;
                Ok(Flow::Normal)
            }
            Stmt::Return(r) => {
                let value = match &r.value {
                    Some(e) => self.eval_expr(e, env)?,
                    None => Value::Void,
                };
                Ok(Flow::Return(value))
            }
            Stmt::For(f) => {
                // v0a: only iterating a `list<T>` is supported.
                let iter_value = self.eval_expr(&f.iter, env)?;
                let list = match iter_value {
                    Value::List(l) => l,
                    other => {
                        return Err(rt(format!(
                            "`for` only iterates `list<T>` in v0, got `{}`",
                            other.display()
                        )))
                    }
                };
                let elem_sid = self.symbol_at_def(f.elem_name.span);
                let len = list.borrow().len();
                for i in 0..len {
                    env.enter();
                    if let Some(sid) = elem_sid {
                        let v = list.borrow()[i].clone();
                        env.bind(sid, v);
                    }
                    let flow = self.eval_block_body(&f.body.statements, env);
                    env.leave();
                    match flow? {
                        Flow::Normal | Flow::Continue => {}
                        Flow::Break => return Ok(Flow::Normal),
                        Flow::Return(v) => return Ok(Flow::Return(v)),
                    }
                }
                Ok(Flow::Normal)
            }
        }
    }

    /// Reassignment target: either a bare `$name` (env update) or a
    /// `->` chain rooted at one (writes the field directly, same as
    /// MemberAssign with `=`).
    fn do_assign(&mut self, lhs: &Expr, value: Value, env: &mut Env) -> EvalResult<()> {
        match lhs {
            Expr::Var { span, .. } => {
                let id = self
                    .resolved
                    .uses
                    .get(span)
                    .copied()
                    .ok_or_else(|| rt("unresolved variable on `:=` LHS".to_string()))?;
                if !env.assign(id, value) {
                    return Err(rt(
                        "tried to reassign a binding that was never declared".to_string()
                    ));
                }
                Ok(())
            }
            Expr::Member { .. } => self.do_member_assign(lhs, value, env),
            other => Err(rt(format!("`:=` LHS shape {other:?} is not supported"))),
        }
    }

    /// Member-chain assignment: walk the chain to the leaf field's
    /// owning instance, then mutate that field. The leaf is always
    /// the `field` of the outermost `Member`; the rest of the chain
    /// is read-only navigation.
    fn do_member_assign(&mut self, lhs: &Expr, value: Value, env: &mut Env) -> EvalResult<()> {
        let Expr::Member {
            receiver, field, ..
        } = lhs
        else {
            return Err(rt(format!(
                "member-assign LHS must be a `->` chain, got {lhs:?}"
            )));
        };
        let recv = self.eval_expr(receiver, env)?;
        match recv {
            Value::Instance { fields, .. } => {
                fields.borrow_mut().insert(field.name.clone(), value);
                Ok(())
            }
            other => Err(rt(format!(
                "cannot write field `{}` on non-instance value `{:?}`",
                field.name, other
            ))),
        }
    }

    fn eval_expr(&mut self, expr: &Expr, env: &mut Env) -> EvalResult<Value> {
        match expr {
            Expr::IntLit { text, .. } => parse_int(text),
            Expr::FloatLit { text, .. } => parse_float(text),
            Expr::BoolLit { value, .. } => Ok(Value::Bool(*value)),
            Expr::NullLit { .. } => Ok(Value::Null),
            Expr::StrLit { parts, .. } => self.eval_string_literal(parts, env),
            Expr::Var { span, .. } | Expr::This { span } => {
                let id = self
                    .resolved
                    .uses
                    .get(span)
                    .copied()
                    .ok_or_else(|| rt("unresolved variable at runtime".to_string()))?;
                env.lookup(id)
                    .cloned()
                    .ok_or_else(|| rt("variable used before initialisation".to_string()))
            }
            Expr::Member {
                receiver, field, ..
            } => {
                let recv = self.eval_expr(receiver, env)?;
                match recv {
                    Value::Instance { fields, .. } => fields
                        .borrow()
                        .get(&field.name)
                        .cloned()
                        .ok_or_else(|| rt(format!("no field `{}` on instance", field.name))),
                    other => Err(rt(format!(
                        "cannot read field `{}` on non-instance value `{:?}`",
                        field.name, other
                    ))),
                }
            }
            Expr::Call { callee, args, .. } => self.eval_call(callee, args, env),
            Expr::Paren { inner, .. } => self.eval_expr(inner, env),
            Expr::Unary { op, operand, .. } => {
                let v = self.eval_expr(operand, env)?;
                eval_unary(*op, v)
            }
            Expr::Borrow { operand, .. } => {
                // The interpreter ignores borrow modifiers; values
                // are copied at the boundary in tree-walk semantics.
                self.eval_expr(operand, env)
            }
            Expr::Try { value, .. } => {
                let v = self.eval_expr(value, env)?;
                match v {
                    Value::ResultOk(inner) => Ok(*inner),
                    Value::OptionSome(inner) => Ok(*inner),
                    fail @ (Value::ResultErr(_) | Value::OptionNone) => {
                        Err(EvalError::Propagate(fail))
                    }
                    other => Err(rt(format!(
                        "postfix `?` requires a result/option value, got {other:?}"
                    ))),
                }
            }
            Expr::Binary { op, lhs, rhs, .. } => {
                // Short-circuit for && / || before evaluating rhs.
                if matches!(op, BinOp::And | BinOp::Or) {
                    let l = self.eval_expr(lhs, env)?;
                    let lb = truthy(&l)?;
                    let short = matches!(op, BinOp::And) && !lb || matches!(op, BinOp::Or) && lb;
                    if short {
                        return Ok(Value::Bool(lb));
                    }
                    let r = self.eval_expr(rhs, env)?;
                    return Ok(Value::Bool(truthy(&r)?));
                }
                let l = self.eval_expr(lhs, env)?;
                let r = self.eval_expr(rhs, env)?;
                eval_binary(*op, l, r)
            }
            Expr::Cast { value, ty, .. } => {
                let v = self.eval_expr(value, env)?;
                eval_cast(v, ty)
            }
            Expr::Static { ty, member, .. } => {
                if let Expr::TypeName { name, .. } = ty.as_ref() {
                    if name.name == "option" && member.name == "none" {
                        return Ok(Value::OptionNone);
                    }
                    if let Some(enum_decl) = find_enum(self.file, &name.name) {
                        if enum_decl
                            .variants
                            .iter()
                            .any(|v| v.name.name == member.name)
                        {
                            return Ok(Value::EnumVariant {
                                enum_name: name.name.clone(),
                                variant: member.name.clone(),
                            });
                        }
                        return Err(rt(format!(
                            "no variant `{}` on enum `{}`",
                            member.name, name.name
                        )));
                    }
                }
                Err(rt(format!(
                    "static access `{ty:?}::{}` not yet supported",
                    member.name
                )))
            }
            Expr::Match {
                scrutinee, arms, ..
            } => {
                let value = self.eval_expr(scrutinee, env)?;
                self.eval_match(&value, arms, env)
            }
            Expr::Lambda { params, body, .. } => Ok(Value::Lambda(Rc::new(LambdaValue {
                params: params.clone(),
                body: body.clone(),
                captures: env.flatten(),
            }))),
            Expr::TypeName { name, .. } => {
                // A bare type name in expression position is rare —
                // usually it's the head of a Static or Call. Reject
                // anything that reaches eval_expr standalone so
                // misuses are loud rather than silent.
                Err(rt(format!(
                    "type name `{}` cannot be used as a value here",
                    name.name
                )))
            }
            Expr::Index { target, index, .. } => {
                let target_v = self.eval_expr(target, env)?;
                let index_v = self.eval_expr(index, env)?;
                let i = match index_v {
                    Value::Int(i) => i,
                    other => {
                        return Err(rt(format!(
                            "list index must be `int`, got `{}`",
                            other.display()
                        )))
                    }
                };
                match target_v {
                    Value::List(items) => {
                        let v = items.borrow();
                        if i < 0 || (i as usize) >= v.len() {
                            return Err(rt(format!(
                                "list index {i} out of bounds (len {})",
                                v.len()
                            )));
                        }
                        Ok(v[i as usize].clone())
                    }
                    other => Err(rt(format!(
                        "indexing only supported on `list<T>` in v0, got `{}`",
                        other.display()
                    ))),
                }
            }
        }
    }

    fn eval_string_literal(&mut self, parts: &[StrPart], env: &mut Env) -> EvalResult<Value> {
        let mut buf = String::new();
        for part in parts {
            match part {
                StrPart::Text(t) => buf.push_str(t),
                StrPart::Expr(e) => {
                    let v = self.eval_expr(e, env)?;
                    buf.push_str(&v.display());
                }
            }
        }
        Ok(Value::String(buf))
    }

    fn eval_call(&mut self, callee: &Expr, args: &[Expr], env: &mut Env) -> EvalResult<Value> {
        // Builtin static calls: Logger::info, result::ok / err,
        // option::some / none.
        if let Expr::Static { ty, member, .. } = callee {
            if let Expr::TypeName { name, .. } = ty.as_ref() {
                if let Some(value) = self.try_static_builtin(&name.name, &member.name, args, env)? {
                    return Ok(value);
                }
            }
        }
        // Method call: `$obj->name(args)` parses as
        // `Call { callee: Member { receiver, field }, ... }`.
        if let Expr::Member {
            receiver, field, ..
        } = callee
        {
            let recv = self.eval_expr(receiver, env)?;
            // Builtin method on a string: `$str->toInt()` plus the
            // D-025 stdlib surface (len/contains/starts/ends/trim/
            // upper/lower).
            if let Value::String(s) = &recv {
                if let Some(value) = self.try_eval_string_method(s, &field.name, args, env)? {
                    return Ok(value);
                }
            }
            // D-027 list method dispatch.
            if let Value::List(items) = &recv {
                if let Some(value) =
                    self.try_eval_list_method(items.clone(), &field.name, args, env)?
                {
                    return Ok(value);
                }
            }
            // D-028 map method dispatch.
            if let Value::Map(entries) = &recv {
                if let Some(value) =
                    self.try_eval_map_method(entries.clone(), &field.name, args, env)?
                {
                    return Ok(value);
                }
            }
            // D-031 set method dispatch.
            if let Value::Set(keys) = &recv {
                if let Some(value) =
                    self.try_eval_set_method(keys.clone(), &field.name, args, env)?
                {
                    return Ok(value);
                }
            }
            // D-026 result/option method dispatch.
            if matches!(
                recv,
                Value::ResultOk(_) | Value::ResultErr(_) | Value::OptionSome(_) | Value::OptionNone
            ) {
                if let Some(value) =
                    self.try_eval_result_option_method(&recv, &field.name, args, env)?
                {
                    return Ok(value);
                }
            }
            return self.invoke_method(recv, &field.name, args, env);
        }
        // Stdlib `list()` constructor (D-027). Reserved name —
        // checked before user-function dispatch so a user
        // `function list(): void {}` cannot shadow it silently.
        if let Expr::TypeName { name, .. } = callee {
            if name.name == "list" && args.is_empty() {
                return Ok(Value::List(Rc::new(RefCell::new(Vec::new()))));
            }
            // Same precedence rule for `map()` (D-028).
            if name.name == "map" && args.is_empty() {
                return Ok(Value::Map(Rc::new(RefCell::new(Vec::new()))));
            }
            // Same precedence rule for `set()` (D-031).
            if name.name == "set" && args.is_empty() {
                return Ok(Value::Set(Rc::new(RefCell::new(Vec::new()))));
            }
        }
        // Class construction or free function: `name(args)` parses
        // as `Call { callee: TypeName(name), ... }`.
        if let Expr::TypeName { name, .. } = callee {
            if let Some(id) = self.resolved.top_level.get(&name.name).copied() {
                let kind = self.resolved.symbol(id).kind;
                if kind == SymbolKind::Class {
                    return self.construct_class(&name.name, args, env);
                }
                if let Some(decl) = find_function(self.file, &self.resolved.symbols, id) {
                    return self.invoke_function(decl, args, env);
                }
            }
        }
        // Otherwise: evaluate the callee. If it produces a Lambda
        // value, invoke it. Covers `$f(args)` and `($f)(args)` and
        // immediately-invoked lambdas.
        let callee_value = self.eval_expr(callee, env)?;
        if let Value::Lambda(lam) = callee_value {
            return self.invoke_lambda(&lam, args, env);
        }
        Err(rt(format!(
            "call form not yet supported by this interpreter: {callee:?}"
        )))
    }

    fn invoke_lambda(
        &mut self,
        lam: &LambdaValue,
        args: &[Expr],
        caller_env: &mut Env,
    ) -> EvalResult<Value> {
        if args.len() != lam.params.len() {
            return Err(rt(format!(
                "lambda expects {} arguments, got {}",
                lam.params.len(),
                args.len()
            )));
        }
        let mut arg_values = Vec::with_capacity(args.len());
        for a in args {
            arg_values.push(self.eval_expr(a, caller_env)?);
        }
        let mut env = Env::default();
        env.enter();
        // Seed the call-time env with the captured snapshot so the
        // lambda body sees the outer bindings it referenced.
        for (id, value) in &lam.captures {
            env.bind(*id, value.clone());
        }
        for (param, value) in lam.params.iter().zip(arg_values) {
            if let Some(sid) = self.symbol_at_def(param.name.span) {
                env.bind(sid, value);
            }
        }
        let result = match &lam.body {
            LambdaBody::Expr(e) => self.eval_expr(e, &mut env),
            LambdaBody::Block(b) => {
                let inner = self.eval_block_body(&b.statements, &mut env);
                match inner {
                    Ok(Flow::Return(v)) => Ok(v),
                    Ok(Flow::Normal) => Ok(Value::Void),
                    Ok(Flow::Break) | Ok(Flow::Continue) => {
                        Err(rt("`break`/`continue` escaped a lambda body".to_string()))
                    }
                    // Lambdas catch `?` propagation the same way
                    // free functions do — the propagation value
                    // becomes the lambda's return.
                    Err(EvalError::Propagate(v)) => Ok(v),
                    Err(e) => Err(e),
                }
            }
        };
        env.leave();
        // A `?` inside a single-expression lambda body also winds
        // back here; convert to the propagation value.
        match result {
            Err(EvalError::Propagate(v)) => Ok(v),
            other => other,
        }
    }

    /// Recognise `Type::method(args)` calls that map to interpreter
    /// builtins. Returns `Some(value)` when the call was handled,
    /// `None` to let the regular dispatch continue.
    fn try_static_builtin(
        &mut self,
        type_name: &str,
        method: &str,
        args: &[Expr],
        env: &mut Env,
    ) -> EvalResult<Option<Value>> {
        match (type_name, method) {
            ("Logger", "info") => {
                let mut rendered = String::new();
                for a in args {
                    let v = self.eval_expr(a, env)?;
                    rendered.push_str(&v.display());
                }
                self.out.stdout.push(rendered);
                Ok(Some(Value::Void))
            }
            // D-032 io namespace. The interpreter routes every io::*
            // print variant into `self.out.stdout` (its single
            // captured stream) so existing harnesses see the output
            // — eprint variants are conceptually stderr but the
            // interp only owns one stream today; keep the strings
            // visible rather than silently dropping them.
            ("io", "print") | ("io", "eprint") => {
                let v = self.eval_expr(&args[0], env)?;
                self.out.stdout.push(v.display());
                Ok(Some(Value::Void))
            }
            ("io", "println") | ("io", "eprintln") => {
                let v = self.eval_expr(&args[0], env)?;
                self.out.stdout.push(v.display());
                Ok(Some(Value::Void))
            }
            ("io", "readLine") => {
                if !args.is_empty() {
                    return Err(rt("io::readLine takes no arguments".to_string()));
                }
                // The tree-walking interpreter has no real stdin
                // — returning `option::none` is the honest signal
                // that reads under `phc run` are stubbed. Compiled
                // binaries via `phc build` read real stdin through
                // the runtime helper.
                Ok(Some(Value::OptionNone))
            }
            ("result", "ok") => {
                let v = single_arg(args, "result::ok", self, env)?;
                Ok(Some(Value::ResultOk(Box::new(v))))
            }
            ("result", "err") => {
                let v = single_arg(args, "result::err", self, env)?;
                Ok(Some(Value::ResultErr(Box::new(v))))
            }
            ("option", "some") => {
                let v = single_arg(args, "option::some", self, env)?;
                Ok(Some(Value::OptionSome(Box::new(v))))
            }
            ("option", "none") => {
                if !args.is_empty() {
                    return Err(rt("option::none takes no arguments".to_string()));
                }
                Ok(Some(Value::OptionNone))
            }
            // Stub: `Http::get(url)` always succeeds with a fake
            // payload built from the URL. Lets the async examples
            // run end-to-end without a real HTTP client. A faithful
            // implementation is Phase 5+ runtime work.
            ("Http", "get") => {
                let v = single_arg(args, "Http::get", self, env)?;
                let url = match &v {
                    Value::String(s) => s.clone(),
                    other => other.display(),
                };
                Ok(Some(Value::ResultOk(Box::new(Value::String(format!(
                    "<bytes from {url}>"
                ))))))
            }
            _ => Ok(None),
        }
    }

    /// D-025 + the prior `toInt` builtin. Returns `Ok(Some(value))`
    /// when the method matched, `Ok(None)` so the caller falls
    /// through to the general method dispatch path.
    fn try_eval_string_method(
        &mut self,
        s: &str,
        method: &str,
        args: &[Expr],
        env: &mut Env,
    ) -> EvalResult<Option<Value>> {
        // toInt is the legacy builtin; keep it on the same path so
        // there is one place that owns string-method dispatch.
        if method == "toInt" && args.is_empty() {
            return Ok(Some(string_to_int(s)));
        }
        let arity_check = |expected: usize| -> EvalResult<()> {
            if args.len() != expected {
                return Err(rt(format!(
                    "string method `{method}` takes {expected} argument(s), got {}",
                    args.len()
                )));
            }
            Ok(())
        };
        let needle_arg = |this: &mut Self, env: &mut Env| -> EvalResult<String> {
            let v = this.eval_expr(&args[0], env)?;
            match v {
                Value::String(s) => Ok(s),
                other => Err(rt(format!(
                    "string method `{method}` expects a `string` argument, got `{}`",
                    other.display()
                ))),
            }
        };
        match method {
            "len" => {
                arity_check(0)?;
                Ok(Some(Value::Int(s.len() as i64)))
            }
            "contains" => {
                arity_check(1)?;
                let needle = needle_arg(self, env)?;
                Ok(Some(Value::Bool(s.contains(&needle))))
            }
            "startsWith" => {
                arity_check(1)?;
                let prefix = needle_arg(self, env)?;
                Ok(Some(Value::Bool(s.starts_with(&prefix))))
            }
            "endsWith" => {
                arity_check(1)?;
                let suffix = needle_arg(self, env)?;
                Ok(Some(Value::Bool(s.ends_with(&suffix))))
            }
            "trim" => {
                arity_check(0)?;
                Ok(Some(Value::String(s.trim().to_string())))
            }
            "upper" => {
                arity_check(0)?;
                // ASCII-only fold to match the C runtime; Unicode
                // case folding lands when the runtime grows it.
                let out: String = s
                    .chars()
                    .map(|c| {
                        if c.is_ascii_lowercase() {
                            c.to_ascii_uppercase()
                        } else {
                            c
                        }
                    })
                    .collect();
                Ok(Some(Value::String(out)))
            }
            "lower" => {
                arity_check(0)?;
                let out: String = s
                    .chars()
                    .map(|c| {
                        if c.is_ascii_uppercase() {
                            c.to_ascii_lowercase()
                        } else {
                            c
                        }
                    })
                    .collect();
                Ok(Some(Value::String(out)))
            }
            _ => Ok(None),
        }
    }

    /// D-028 map method dispatch. v0a only supports string keys.
    /// D-031 set method dispatch. v0a only supports string keys.
    fn try_eval_set_method(
        &mut self,
        keys: Rc<RefCell<Vec<String>>>,
        method: &str,
        args: &[Expr],
        env: &mut Env,
    ) -> EvalResult<Option<Value>> {
        let arity_check = |expected: usize| -> EvalResult<()> {
            if args.len() != expected {
                return Err(rt(format!(
                    "set method `{method}` takes {expected} argument(s), got {}",
                    args.len()
                )));
            }
            Ok(())
        };
        let key_arg = |this: &mut Self, env: &mut Env| -> EvalResult<String> {
            let v = this.eval_expr(&args[0], env)?;
            match v {
                Value::String(s) => Ok(s),
                other => Err(rt(format!(
                    "set method `{method}` expects a `string` key, got `{}`",
                    other.display()
                ))),
            }
        };
        match method {
            "len" => {
                arity_check(0)?;
                Ok(Some(Value::Int(keys.borrow().len() as i64)))
            }
            "has" => {
                arity_check(1)?;
                let k = key_arg(self, env)?;
                Ok(Some(Value::Bool(keys.borrow().iter().any(|x| x == &k))))
            }
            "add" => {
                arity_check(1)?;
                let k = key_arg(self, env)?;
                let mut v = keys.borrow_mut();
                if v.iter().any(|x| x == &k) {
                    return Ok(Some(Value::Bool(false)));
                }
                v.push(k);
                Ok(Some(Value::Bool(true)))
            }
            "remove" => {
                arity_check(1)?;
                let k = key_arg(self, env)?;
                let mut v = keys.borrow_mut();
                if let Some(pos) = v.iter().position(|x| x == &k) {
                    v.swap_remove(pos);
                    Ok(Some(Value::Bool(true)))
                } else {
                    Ok(Some(Value::Bool(false)))
                }
            }
            _ => Ok(None),
        }
    }

    fn try_eval_map_method(
        &mut self,
        entries: Rc<RefCell<Vec<(String, Value)>>>,
        method: &str,
        args: &[Expr],
        env: &mut Env,
    ) -> EvalResult<Option<Value>> {
        let arity_check = |expected: usize| -> EvalResult<()> {
            if args.len() != expected {
                return Err(rt(format!(
                    "map method `{method}` takes {expected} argument(s), got {}",
                    args.len()
                )));
            }
            Ok(())
        };
        let key_arg = |this: &mut Self, env: &mut Env| -> EvalResult<String> {
            let v = this.eval_expr(&args[0], env)?;
            match v {
                Value::String(s) => Ok(s),
                other => Err(rt(format!(
                    "map method `{method}` expects a `string` key, got `{}`",
                    other.display()
                ))),
            }
        };
        match method {
            "len" => {
                arity_check(0)?;
                Ok(Some(Value::Int(entries.borrow().len() as i64)))
            }
            "has" => {
                arity_check(1)?;
                let key = key_arg(self, env)?;
                Ok(Some(Value::Bool(
                    entries.borrow().iter().any(|(k, _)| k == &key),
                )))
            }
            "get" => {
                arity_check(1)?;
                let key = key_arg(self, env)?;
                let hit = entries.borrow().iter().find(|(k, _)| k == &key).cloned();
                match hit {
                    Some((_, v)) => Ok(Some(Value::OptionSome(Box::new(v)))),
                    None => Ok(Some(Value::OptionNone)),
                }
            }
            "set" => {
                if args.len() != 2 {
                    return Err(rt(format!(
                        "map method `set` takes 2 arguments, got {}",
                        args.len()
                    )));
                }
                let key = key_arg(self, env)?;
                let value = self.eval_expr(&args[1], env)?;
                let mut e = entries.borrow_mut();
                if let Some(pair) = e.iter_mut().find(|(k, _)| k == &key) {
                    pair.1 = value;
                } else {
                    e.push((key, value));
                }
                Ok(Some(Value::Void))
            }
            _ => Ok(None),
        }
    }

    /// D-026 Result/Option method dispatch. Receiver is one of the
    /// four tagged-union Value variants; method picks the predicate
    /// or the unwrap/orElse branch.
    fn try_eval_result_option_method(
        &mut self,
        recv: &Value,
        method: &str,
        args: &[Expr],
        env: &mut Env,
    ) -> EvalResult<Option<Value>> {
        let arity_check = |expected: usize| -> EvalResult<()> {
            if args.len() != expected {
                return Err(rt(format!(
                    "method `{method}` takes {expected} argument(s), got {}",
                    args.len()
                )));
            }
            Ok(())
        };
        match (recv, method) {
            (Value::ResultOk(_), "isOk") | (Value::ResultErr(_), "isOk") => {
                arity_check(0)?;
                Ok(Some(Value::Bool(matches!(recv, Value::ResultOk(_)))))
            }
            (Value::ResultOk(_), "isErr") | (Value::ResultErr(_), "isErr") => {
                arity_check(0)?;
                Ok(Some(Value::Bool(matches!(recv, Value::ResultErr(_)))))
            }
            (Value::ResultOk(v), "unwrapOr") => {
                arity_check(1)?;
                let _ = self.eval_expr(&args[0], env)?;
                Ok(Some((**v).clone()))
            }
            (Value::ResultErr(_), "unwrapOr") => {
                arity_check(1)?;
                Ok(Some(self.eval_expr(&args[0], env)?))
            }
            (Value::OptionSome(_), "isSome") | (Value::OptionNone, "isSome") => {
                arity_check(0)?;
                Ok(Some(Value::Bool(matches!(recv, Value::OptionSome(_)))))
            }
            (Value::OptionSome(_), "isNone") | (Value::OptionNone, "isNone") => {
                arity_check(0)?;
                Ok(Some(Value::Bool(matches!(recv, Value::OptionNone))))
            }
            (Value::OptionSome(v), "unwrapOr") => {
                arity_check(1)?;
                let _ = self.eval_expr(&args[0], env)?;
                Ok(Some((**v).clone()))
            }
            (Value::OptionNone, "unwrapOr") => {
                arity_check(1)?;
                Ok(Some(self.eval_expr(&args[0], env)?))
            }
            (Value::OptionSome(_), "orElse") => {
                arity_check(1)?;
                let _ = self.eval_expr(&args[0], env)?;
                Ok(Some(recv.clone()))
            }
            (Value::OptionNone, "orElse") => {
                arity_check(1)?;
                Ok(Some(self.eval_expr(&args[0], env)?))
            }
            // D-029 closure forms. Each one evaluates the callback
            // expression (must yield a Value::Lambda) and invokes
            // it with the unwrapped payload.
            (Value::ResultOk(v), "map") => {
                arity_check(1)?;
                let lam = self.eval_lambda_arg(&args[0], env)?;
                let mapped = self.invoke_lambda_with(&lam, vec![(**v).clone()])?;
                Ok(Some(Value::ResultOk(Box::new(mapped))))
            }
            (Value::ResultErr(_), "map") => {
                arity_check(1)?;
                // Evaluate the callback (preserves any side effects
                // a user might rely on), then pass the err through.
                let _ = self.eval_lambda_arg(&args[0], env)?;
                Ok(Some(recv.clone()))
            }
            (Value::ResultOk(v), "andThen") => {
                arity_check(1)?;
                let lam = self.eval_lambda_arg(&args[0], env)?;
                Ok(Some(self.invoke_lambda_with(&lam, vec![(**v).clone()])?))
            }
            (Value::ResultErr(_), "andThen") => {
                arity_check(1)?;
                let _ = self.eval_lambda_arg(&args[0], env)?;
                Ok(Some(recv.clone()))
            }
            (Value::ResultOk(v), "unwrap") => {
                arity_check(0)?;
                Ok(Some((**v).clone()))
            }
            (Value::ResultErr(e), "unwrap") => {
                arity_check(0)?;
                Err(rt(format!("unwrap on result::err({})", e.display())))
            }
            (Value::OptionSome(v), "map") => {
                arity_check(1)?;
                let lam = self.eval_lambda_arg(&args[0], env)?;
                let mapped = self.invoke_lambda_with(&lam, vec![(**v).clone()])?;
                Ok(Some(Value::OptionSome(Box::new(mapped))))
            }
            (Value::OptionNone, "map") => {
                arity_check(1)?;
                let _ = self.eval_lambda_arg(&args[0], env)?;
                Ok(Some(Value::OptionNone))
            }
            (Value::OptionSome(v), "andThen") => {
                arity_check(1)?;
                let lam = self.eval_lambda_arg(&args[0], env)?;
                Ok(Some(self.invoke_lambda_with(&lam, vec![(**v).clone()])?))
            }
            (Value::OptionNone, "andThen") => {
                arity_check(1)?;
                let _ = self.eval_lambda_arg(&args[0], env)?;
                Ok(Some(Value::OptionNone))
            }
            (Value::OptionSome(v), "okOr") => {
                arity_check(1)?;
                let _ = self.eval_expr(&args[0], env)?;
                Ok(Some(Value::ResultOk(v.clone())))
            }
            (Value::OptionNone, "okOr") => {
                arity_check(1)?;
                let e = self.eval_expr(&args[0], env)?;
                Ok(Some(Value::ResultErr(Box::new(e))))
            }
            (Value::OptionSome(v), "unwrap") => {
                arity_check(0)?;
                Ok(Some((**v).clone()))
            }
            (Value::OptionNone, "unwrap") => {
                arity_check(0)?;
                Err(rt("unwrap on option::none".to_string()))
            }
            _ => Ok(None),
        }
    }

    /// Evaluate an expression that must produce a `Value::Lambda`
    /// (used by D-029 closure methods). Returns the Rc-shared
    /// LambdaValue or a runtime error if the value is not a lambda.
    fn eval_lambda_arg(&mut self, expr: &Expr, env: &mut Env) -> EvalResult<Rc<LambdaValue>> {
        match self.eval_expr(expr, env)? {
            Value::Lambda(lam) => Ok(lam),
            other => Err(rt(format!(
                "expected a `fn(...): R` callback, got `{}`",
                other.display()
            ))),
        }
    }

    /// Invoke a lambda with already-evaluated argument values
    /// instead of an `[Expr]` slice. The existing `invoke_lambda`
    /// re-evaluates from AST nodes; this variant lets the D-029
    /// methods pass the unwrapped payload straight through.
    fn invoke_lambda_with(
        &mut self,
        lam: &LambdaValue,
        arg_values: Vec<Value>,
    ) -> EvalResult<Value> {
        if arg_values.len() != lam.params.len() {
            return Err(rt(format!(
                "lambda expects {} arguments, got {}",
                lam.params.len(),
                arg_values.len()
            )));
        }
        let mut env = Env::default();
        env.enter();
        for (id, value) in &lam.captures {
            env.bind(*id, value.clone());
        }
        for (param, value) in lam.params.iter().zip(arg_values) {
            if let Some(sid) = self.symbol_at_def(param.name.span) {
                env.bind(sid, value);
            }
        }
        let result = match &lam.body {
            LambdaBody::Expr(e) => self.eval_expr(e, &mut env),
            LambdaBody::Block(b) => {
                let inner = self.eval_block_body(&b.statements, &mut env);
                match inner {
                    Ok(Flow::Return(v)) => Ok(v),
                    Ok(Flow::Normal) => Ok(Value::Void),
                    Ok(Flow::Break) | Ok(Flow::Continue) => {
                        Err(rt("`break`/`continue` escaped a lambda body".to_string()))
                    }
                    Err(EvalError::Propagate(v)) => Ok(v),
                    Err(e) => Err(e),
                }
            }
        };
        env.leave();
        result
    }

    /// D-027 list method dispatch. Returns Ok(Some) when the method
    /// matched; Ok(None) so the caller falls through to the general
    /// (no-such-method) error path.
    fn try_eval_list_method(
        &mut self,
        items: Rc<RefCell<Vec<Value>>>,
        method: &str,
        args: &[Expr],
        env: &mut Env,
    ) -> EvalResult<Option<Value>> {
        let arity_check = |expected: usize| -> EvalResult<()> {
            if args.len() != expected {
                return Err(rt(format!(
                    "list method `{method}` takes {expected} argument(s), got {}",
                    args.len()
                )));
            }
            Ok(())
        };
        match method {
            "len" => {
                arity_check(0)?;
                Ok(Some(Value::Int(items.borrow().len() as i64)))
            }
            "push" => {
                arity_check(1)?;
                let v = self.eval_expr(&args[0], env)?;
                items.borrow_mut().push(v);
                Ok(Some(Value::Void))
            }
            "at" => {
                arity_check(1)?;
                let iv = self.eval_expr(&args[0], env)?;
                let i = match iv {
                    Value::Int(i) => i,
                    other => {
                        return Err(rt(format!(
                            "list `at` index must be `int`, got `{}`",
                            other.display()
                        )))
                    }
                };
                let v = items.borrow();
                if i < 0 || (i as usize) >= v.len() {
                    return Err(rt(format!(
                        "list index {i} out of bounds (len {})",
                        v.len()
                    )));
                }
                Ok(Some(v[i as usize].clone()))
            }
            // D-030 closure forms. Each invokes a user lambda on
            // every element in insertion order.
            "forEach" => {
                arity_check(1)?;
                let lam = self.eval_lambda_arg(&args[0], env)?;
                let snapshot: Vec<Value> = items.borrow().clone();
                for v in snapshot {
                    self.invoke_lambda_with(&lam, vec![v])?;
                }
                Ok(Some(Value::Void))
            }
            "map" => {
                arity_check(1)?;
                let lam = self.eval_lambda_arg(&args[0], env)?;
                let snapshot: Vec<Value> = items.borrow().clone();
                let mut out = Vec::with_capacity(snapshot.len());
                for v in snapshot {
                    out.push(self.invoke_lambda_with(&lam, vec![v])?);
                }
                Ok(Some(Value::List(Rc::new(RefCell::new(out)))))
            }
            "filter" => {
                arity_check(1)?;
                let lam = self.eval_lambda_arg(&args[0], env)?;
                let snapshot: Vec<Value> = items.borrow().clone();
                let mut out = Vec::new();
                for v in snapshot {
                    let keep = self.invoke_lambda_with(&lam, vec![v.clone()])?;
                    match keep {
                        Value::Bool(true) => out.push(v),
                        Value::Bool(false) => {}
                        other => {
                            return Err(rt(format!(
                                "list `filter` predicate must return `bool`, got `{}`",
                                other.display()
                            )))
                        }
                    }
                }
                Ok(Some(Value::List(Rc::new(RefCell::new(out)))))
            }
            _ => Ok(None),
        }
    }

    fn invoke_method(
        &mut self,
        receiver: Value,
        method_name: &str,
        args: &[Expr],
        caller_env: &mut Env,
    ) -> EvalResult<Value> {
        let class_name = match &receiver {
            Value::Instance { class, .. } => class.clone(),
            other => {
                return Err(rt(format!(
                    "cannot call method `{method_name}` on `{other:?}`"
                )));
            }
        };
        let class_decl = find_class(self.file, &class_name).ok_or_else(|| {
            rt(format!(
                "class `{class_name}` not found in this source file"
            ))
        })?;
        let method = find_method_in_class(class_decl, method_name)
            .or_else(|| find_method_in_traits(self.file, class_decl, method_name));
        let Some(method) = method else {
            return Err(rt(format!(
                "no method `{method_name}` on class `{class_name}`"
            )));
        };
        let mut arg_values = Vec::with_capacity(args.len());
        for a in args {
            arg_values.push(self.eval_expr(a, caller_env)?);
        }
        self.run_function_with_this(method, arg_values, Some(receiver))
    }

    fn construct_class(
        &mut self,
        class_name: &str,
        args: &[Expr],
        caller_env: &mut Env,
    ) -> EvalResult<Value> {
        let class_decl = find_class(self.file, class_name).ok_or_else(|| {
            rt(format!(
                "class `{class_name}` not found in this source file"
            ))
        })?;
        // Initialise field storage with each field's declared default
        // (or Value::Null for fields with no default).
        let mut fields: HashMap<String, Value> = HashMap::new();
        for member in &class_decl.members {
            if let ClassMember::Field(field) = member {
                let value = match &field.default {
                    Some(expr) => self.eval_expr(expr, caller_env)?,
                    None => Value::Null,
                };
                fields.insert(field.name.name.clone(), value);
            }
        }
        let instance = Value::Instance {
            class: class_name.to_string(),
            fields: Rc::new(RefCell::new(fields)),
        };
        // Run the constructor (if any). Constructor parameters with
        // `public` are promoted to fields after binding.
        if let Some(ctor) = find_constructor(class_decl) {
            let mut arg_values = Vec::with_capacity(args.len());
            for a in args {
                arg_values.push(self.eval_expr(a, caller_env)?);
            }
            // Resolver records `$this` def_span at the class name
            // (not the `construct` keyword), so pass class.name.span.
            self.run_constructor(ctor, arg_values, instance.clone(), class_decl.name.span)?;
        } else if !args.is_empty() {
            return Err(rt(format!(
                "class `{class_name}` has no constructor but received {} arguments",
                args.len()
            )));
        }
        Ok(instance)
    }

    fn run_constructor(
        &mut self,
        ctor: &ConstructDecl,
        arg_values: Vec<Value>,
        this: Value,
        this_span: phc_span::Span,
    ) -> EvalResult<()> {
        if arg_values.len() != ctor.params.len() {
            return Err(rt(format!(
                "constructor expects {} arguments, got {}",
                ctor.params.len(),
                arg_values.len()
            )));
        }
        let mut env = Env::default();
        env.enter();
        // Bind $this synthetic + every parameter.
        if let Value::Instance { fields, .. } = &this {
            for (param, value) in ctor.params.iter().zip(arg_values) {
                if param.promoted {
                    fields
                        .borrow_mut()
                        .insert(param.name.name.clone(), value.clone());
                }
                if let Some(sid) = self.symbol_at_def(param.name.span) {
                    env.bind(sid, value);
                }
            }
            self.bind_this(&mut env, this.clone(), this_span);
        }
        let result = self.eval_block_body(&ctor.body.statements, &mut env);
        env.leave();
        match result {
            Ok(Flow::Normal) | Ok(Flow::Return(_)) => Ok(()),
            Ok(Flow::Break) | Ok(Flow::Continue) => Err(rt(
                "`break`/`continue` escaped a constructor body".to_string(),
            )),
            // A `?` from a constructor body has nowhere to go; the
            // constructor must return the instance, not a propagated
            // value. Surface as a runtime error.
            Err(EvalError::Propagate(v)) => Err(rt(format!(
                "`?` propagated `{}` out of a constructor body",
                v.display()
            ))),
            Err(e) => Err(e),
        }
    }

    fn run_function_with_this(
        &mut self,
        decl: &FunctionDecl,
        arg_values: Vec<Value>,
        this: Option<Value>,
    ) -> EvalResult<Value> {
        if arg_values.len() != decl.params.len() {
            return Err(rt(format!(
                "`{}` expects {} arguments, got {}",
                decl.name.name,
                decl.params.len(),
                arg_values.len()
            )));
        }
        let mut env = Env::default();
        env.enter();
        for (param, value) in decl.params.iter().zip(arg_values) {
            if let Some(sid) = self.symbol_at_def(param.name.span) {
                env.bind(sid, value);
            }
        }
        if let Some(this_value) = this {
            self.bind_this(&mut env, this_value, decl.name.span);
        }
        let result = self.eval_block_body(&decl.body.statements, &mut env);
        env.leave();
        match result {
            Ok(Flow::Return(v)) => Ok(v),
            Ok(Flow::Normal) => Ok(Value::Void),
            Ok(Flow::Break) | Ok(Flow::Continue) => {
                Err(rt("`break`/`continue` escaped a function body".to_string()))
            }
            // `?` propagation short-circuits the enclosing function:
            // the propagation value becomes the function's return.
            Err(EvalError::Propagate(v)) => Ok(v),
            Err(e) => Err(e),
        }
    }

    /// Bind `$this` in the given env. The synthetic `$this` symbol
    /// is whichever Value-kind symbol the resolver introduced with
    /// def_span equal to the surrounding decl span (constructor /
    /// method / hook). We scan for that symbol so we hit the same
    /// id resolved.uses already references.
    fn bind_this(&self, env: &mut Env, this: Value, host_span: phc_span::Span) {
        for sym in &self.resolved.symbols {
            if sym.name == "this" && sym.kind == SymbolKind::Value && sym.def_span == host_span {
                env.bind(sym.id, this);
                return;
            }
        }
    }

    fn invoke_function(
        &mut self,
        decl: &FunctionDecl,
        args: &[Expr],
        caller_env: &mut Env,
    ) -> EvalResult<Value> {
        let mut arg_values = Vec::with_capacity(args.len());
        for a in args {
            arg_values.push(self.eval_expr(a, caller_env)?);
        }
        self.run_function_with_this(decl, arg_values, None)
    }

    fn eval_match(
        &mut self,
        scrutinee: &Value,
        arms: &[MatchArm],
        env: &mut Env,
    ) -> EvalResult<Value> {
        for arm in arms {
            env.enter();
            let matched = self.try_pattern(&arm.pattern, scrutinee, env)?;
            if matched {
                let guard_pass = match &arm.guard {
                    Some(g) => {
                        let v = self.eval_expr(g, env)?;
                        truthy(&v)?
                    }
                    None => true,
                };
                if guard_pass {
                    let result = self.eval_expr(&arm.body, env);
                    env.leave();
                    return result;
                }
            }
            env.leave();
        }
        Err(rt(format!("no match arm for value `{scrutinee:?}`")))
    }

    /// Returns true when the pattern matches; if matching introduces
    /// a binding (Var pattern), the binding is added to the current
    /// env frame.
    fn try_pattern(&mut self, pat: &Pattern, value: &Value, env: &mut Env) -> EvalResult<bool> {
        match pat {
            Pattern::Wildcard { .. } => Ok(true),
            Pattern::Literal(expr) => {
                let lit = self.eval_expr(expr, env)?;
                Ok(values_equal(&lit, value))
            }
            Pattern::Var { name, .. } => {
                if let Some(sid) = self.symbol_at_def(name.span) {
                    env.bind(sid, value.clone());
                }
                Ok(true)
            }
            Pattern::EnumVariant { ty, variant, .. } => match value {
                Value::EnumVariant {
                    enum_name,
                    variant: vname,
                } => Ok(enum_name == &ty.name && vname == &variant.name),
                _ => Ok(false),
            },
            Pattern::Or { atoms, .. } => {
                for atom in atoms {
                    if self.try_pattern(atom, value, env)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
        }
    }

    fn symbol_at_def(&self, span: phc_span::Span) -> Option<SymbolId> {
        self.resolved
            .symbols
            .iter()
            .find(|s| s.def_span == span)
            .map(|s| s.id)
    }
}

fn truthy(v: &Value) -> EvalResult<bool> {
    match v {
        Value::Bool(b) => Ok(*b),
        other => Err(rt(format!("expected bool in condition, got {other:?}"))),
    }
}

fn eval_unary(op: UnaryOp, v: Value) -> EvalResult<Value> {
    match (op, v) {
        (UnaryOp::Not, Value::Bool(b)) => Ok(Value::Bool(!b)),
        (UnaryOp::Neg, Value::Int(i)) => Ok(Value::Int(-i)),
        (UnaryOp::Neg, Value::Float(f)) => Ok(Value::Float(-f)),
        (UnaryOp::Await, v) => Ok(v),
        (op, v) => Err(rt(format!("unary {op:?} not defined for {v:?}"))),
    }
}

fn eval_binary(op: BinOp, l: Value, r: Value) -> EvalResult<Value> {
    use BinOp::*;
    use Value::*;
    let mismatched = || rt(format!("binary {op:?} not defined for `{l:?}` and `{r:?}`"));
    match op {
        Add => match (l.clone(), r.clone()) {
            (Int(a), Int(b)) => Ok(Int(a + b)),
            (Float(a), Float(b)) => Ok(Float(a + b)),
            (String(a), String(b)) => Ok(String(a + &b)),
            _ => Err(mismatched()),
        },
        Sub => match (l.clone(), r.clone()) {
            (Int(a), Int(b)) => Ok(Int(a - b)),
            (Float(a), Float(b)) => Ok(Float(a - b)),
            _ => Err(mismatched()),
        },
        Mul => match (l.clone(), r.clone()) {
            (Int(a), Int(b)) => Ok(Int(a * b)),
            (Float(a), Float(b)) => Ok(Float(a * b)),
            _ => Err(mismatched()),
        },
        Div => match (l.clone(), r.clone()) {
            (Int(_), Int(0)) => Err(rt("integer division by zero".to_string())),
            (Int(a), Int(b)) => Ok(Int(a / b)),
            (Float(a), Float(b)) => Ok(Float(a / b)),
            _ => Err(mismatched()),
        },
        Rem => match (l.clone(), r.clone()) {
            (Int(_), Int(0)) => Err(rt("integer remainder by zero".to_string())),
            (Int(a), Int(b)) => Ok(Int(a % b)),
            (Float(a), Float(b)) => Ok(Float(a % b)),
            _ => Err(mismatched()),
        },
        Lt => bool_cmp(&l, &r, |o| o == std::cmp::Ordering::Less),
        Le => bool_cmp(&l, &r, |o| o != std::cmp::Ordering::Greater),
        Gt => bool_cmp(&l, &r, |o| o == std::cmp::Ordering::Greater),
        Ge => bool_cmp(&l, &r, |o| o != std::cmp::Ordering::Less),
        Eq => Ok(Bool(values_equal(&l, &r))),
        Neq => Ok(Bool(!values_equal(&l, &r))),
        // Logical short-circuit handled in eval_expr; this arm is
        // unreachable but covers the match.
        And | Or => unreachable!("&& / || are short-circuited above"),
        NullCoalesce => match l {
            Null => Ok(r),
            other => Ok(other),
        },
    }
}

fn bool_cmp(l: &Value, r: &Value, pred: impl Fn(std::cmp::Ordering) -> bool) -> EvalResult<Value> {
    let ord = match (l, r) {
        (Value::Int(a), Value::Int(b)) => a.cmp(b),
        (Value::Float(a), Value::Float(b)) => a
            .partial_cmp(b)
            .ok_or_else(|| rt("NaN comparison".to_string()))?,
        (Value::String(a), Value::String(b)) => a.cmp(b),
        (Value::Bool(a), Value::Bool(b)) => a.cmp(b),
        _ => return Err(rt(format!("cannot order `{l:?}` and `{r:?}`"))),
    };
    Ok(Value::Bool(pred(ord)))
}

fn values_equal(l: &Value, r: &Value) -> bool {
    match (l, r) {
        (Value::Int(a), Value::Int(b)) => a == b,
        (Value::Float(a), Value::Float(b)) => a == b,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::String(a), Value::String(b)) => a == b,
        (Value::Null, Value::Null) => true,
        (Value::Void, Value::Void) => true,
        (
            Value::EnumVariant {
                enum_name: en_l,
                variant: v_l,
            },
            Value::EnumVariant {
                enum_name: en_r,
                variant: v_r,
            },
        ) => en_l == en_r && v_l == v_r,
        _ => false,
    }
}

fn eval_cast(v: Value, ty: &phc_ast::TypeRef) -> EvalResult<Value> {
    let target = ty.path.last().map(|i| i.name.as_str()).unwrap_or("");
    match (v.clone(), target) {
        (Value::Int(i), "float") => Ok(Value::Float(i as f64)),
        (Value::Int(i), "string") => Ok(Value::String(i.to_string())),
        (Value::Float(f), "string") => Ok(Value::String(f.to_string())),
        (Value::Bool(b), "string") => Ok(Value::String(b.to_string())),
        (v, _) => Ok(v),
    }
}

fn single_arg(
    args: &[Expr],
    name: &str,
    interp: &mut Interp<'_>,
    env: &mut Env,
) -> EvalResult<Value> {
    if args.len() != 1 {
        return Err(rt(format!(
            "{name} expects exactly one argument, got {}",
            args.len()
        )));
    }
    interp.eval_expr(&args[0], env)
}

fn string_to_int(s: &str) -> Value {
    let cleaned: String = s.chars().filter(|c| *c != '_').collect();
    match cleaned.parse::<i64>() {
        Ok(n) => Value::ResultOk(Box::new(Value::Int(n))),
        Err(_) => Value::ResultErr(Box::new(Value::String(format!(
            "could not parse `{s}` as int"
        )))),
    }
}

fn parse_int(text: &str) -> EvalResult<Value> {
    let cleaned: String = text.chars().filter(|c| *c != '_').collect();
    cleaned
        .parse::<i64>()
        .map(Value::Int)
        .map_err(|e| rt(format!("invalid integer literal `{text}`: {e}")))
}

fn parse_float(text: &str) -> EvalResult<Value> {
    let cleaned: String = text.chars().filter(|c| *c != '_').collect();
    cleaned
        .parse::<f64>()
        .map(Value::Float)
        .map_err(|e| rt(format!("invalid float literal `{text}`: {e}")))
}

#[cfg(test)]
mod tests;
