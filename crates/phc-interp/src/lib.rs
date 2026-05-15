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
    BinOp, ClassDecl, ClassMember, ConstructDecl, Expr, FunctionDecl, Item, SourceFile, Stmt,
    StrPart, TraitDecl, UnaryOp,
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
        Err(e) => {
            interp.out.errors.push(e);
            Value::Null
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
    fn eval_block_body(&mut self, stmts: &[Stmt], env: &mut Env) -> Result<Flow, RuntimeError> {
        for stmt in stmts {
            match self.eval_stmt(stmt, env)? {
                Flow::Normal => {}
                other => return Ok(other),
            }
        }
        Ok(Flow::Normal)
    }

    fn eval_block_scoped(&mut self, stmts: &[Stmt], env: &mut Env) -> Result<Flow, RuntimeError> {
        env.enter();
        let flow = self.eval_block_body(stmts, env);
        env.leave();
        flow
    }

    fn eval_stmt(&mut self, stmt: &Stmt, env: &mut Env) -> Result<Flow, RuntimeError> {
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
            other => Err(RuntimeError {
                message: format!(
                    "statement {other:?} is not yet supported by this interpreter \
                     (For / MemberAssign land in I3+)"
                ),
            }),
        }
    }

    /// Reassignment target: either a bare `$name` (env update) or a
    /// `->` chain rooted at one (writes the field directly, same as
    /// MemberAssign with `=`).
    fn do_assign(&mut self, lhs: &Expr, value: Value, env: &mut Env) -> Result<(), RuntimeError> {
        match lhs {
            Expr::Var { span, .. } => {
                let id = self
                    .resolved
                    .uses
                    .get(span)
                    .copied()
                    .ok_or_else(|| RuntimeError {
                        message: "unresolved variable on `:=` LHS".to_string(),
                    })?;
                if !env.assign(id, value) {
                    return Err(RuntimeError {
                        message: "tried to reassign a binding that was never declared".to_string(),
                    });
                }
                Ok(())
            }
            Expr::Member { .. } => self.do_member_assign(lhs, value, env),
            other => Err(RuntimeError {
                message: format!("`:=` LHS shape {other:?} is not supported"),
            }),
        }
    }

    /// Member-chain assignment: walk the chain to the leaf field's
    /// owning instance, then mutate that field. The leaf is always
    /// the `field` of the outermost `Member`; the rest of the chain
    /// is read-only navigation.
    fn do_member_assign(
        &mut self,
        lhs: &Expr,
        value: Value,
        env: &mut Env,
    ) -> Result<(), RuntimeError> {
        let Expr::Member {
            receiver, field, ..
        } = lhs
        else {
            return Err(RuntimeError {
                message: format!("member-assign LHS must be a `->` chain, got {lhs:?}"),
            });
        };
        let recv = self.eval_expr(receiver, env)?;
        match recv {
            Value::Instance { fields, .. } => {
                fields.borrow_mut().insert(field.name.clone(), value);
                Ok(())
            }
            other => Err(RuntimeError {
                message: format!(
                    "cannot write field `{}` on non-instance value `{:?}`",
                    field.name, other
                ),
            }),
        }
    }

    fn eval_expr(&mut self, expr: &Expr, env: &mut Env) -> Result<Value, RuntimeError> {
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
                    .ok_or_else(|| RuntimeError {
                        message: "unresolved variable at runtime".to_string(),
                    })?;
                env.lookup(id).cloned().ok_or_else(|| RuntimeError {
                    message: "variable used before initialisation".to_string(),
                })
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
                        .ok_or_else(|| RuntimeError {
                            message: format!("no field `{}` on instance", field.name),
                        }),
                    other => Err(RuntimeError {
                        message: format!(
                            "cannot read field `{}` on non-instance value `{:?}`",
                            field.name, other
                        ),
                    }),
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
            other => Err(RuntimeError {
                message: format!(
                    "expression {other:?} is not yet supported by this interpreter \
                     (Member / Static / Index / Try / Match / Lambda / TypeName / This land in I3+)"
                ),
            }),
        }
    }

    fn eval_string_literal(
        &mut self,
        parts: &[StrPart],
        env: &mut Env,
    ) -> Result<Value, RuntimeError> {
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

    fn eval_call(
        &mut self,
        callee: &Expr,
        args: &[Expr],
        env: &mut Env,
    ) -> Result<Value, RuntimeError> {
        // Builtin: `Logger::info(string)` — emit one stdout line.
        if let Expr::Static { ty, member, .. } = callee {
            if let Expr::TypeName { name, .. } = ty.as_ref() {
                if name.name == "Logger" && member.name == "info" {
                    let mut rendered = String::new();
                    for a in args {
                        let v = self.eval_expr(a, env)?;
                        rendered.push_str(&v.display());
                    }
                    self.out.stdout.push(rendered);
                    return Ok(Value::Void);
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
            return self.invoke_method(recv, &field.name, args, env);
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
        Err(RuntimeError {
            message: format!("call form not yet supported by this interpreter: {callee:?}"),
        })
    }

    fn invoke_method(
        &mut self,
        receiver: Value,
        method_name: &str,
        args: &[Expr],
        caller_env: &mut Env,
    ) -> Result<Value, RuntimeError> {
        let class_name = match &receiver {
            Value::Instance { class, .. } => class.clone(),
            other => {
                return Err(RuntimeError {
                    message: format!("cannot call method `{method_name}` on `{other:?}`"),
                });
            }
        };
        let class_decl = find_class(self.file, &class_name).ok_or_else(|| RuntimeError {
            message: format!("class `{class_name}` not found in this source file"),
        })?;
        let method = find_method_in_class(class_decl, method_name)
            .or_else(|| find_method_in_traits(self.file, class_decl, method_name));
        let Some(method) = method else {
            return Err(RuntimeError {
                message: format!("no method `{method_name}` on class `{class_name}`"),
            });
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
    ) -> Result<Value, RuntimeError> {
        let class_decl = find_class(self.file, class_name).ok_or_else(|| RuntimeError {
            message: format!("class `{class_name}` not found in this source file"),
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
            return Err(RuntimeError {
                message: format!(
                    "class `{class_name}` has no constructor but received {} arguments",
                    args.len()
                ),
            });
        }
        Ok(instance)
    }

    fn run_constructor(
        &mut self,
        ctor: &ConstructDecl,
        arg_values: Vec<Value>,
        this: Value,
        this_span: phc_span::Span,
    ) -> Result<(), RuntimeError> {
        if arg_values.len() != ctor.params.len() {
            return Err(RuntimeError {
                message: format!(
                    "constructor expects {} arguments, got {}",
                    ctor.params.len(),
                    arg_values.len()
                ),
            });
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
        let flow = self.eval_block_body(&ctor.body.statements, &mut env)?;
        env.leave();
        match flow {
            Flow::Normal | Flow::Return(_) => Ok(()),
            Flow::Break | Flow::Continue => Err(RuntimeError {
                message: "`break`/`continue` escaped a constructor body".to_string(),
            }),
        }
    }

    fn run_function_with_this(
        &mut self,
        decl: &FunctionDecl,
        arg_values: Vec<Value>,
        this: Option<Value>,
    ) -> Result<Value, RuntimeError> {
        if arg_values.len() != decl.params.len() {
            return Err(RuntimeError {
                message: format!(
                    "`{}` expects {} arguments, got {}",
                    decl.name.name,
                    decl.params.len(),
                    arg_values.len()
                ),
            });
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
        let flow = self.eval_block_body(&decl.body.statements, &mut env)?;
        env.leave();
        Ok(match flow {
            Flow::Return(v) => v,
            Flow::Normal => Value::Void,
            Flow::Break | Flow::Continue => {
                return Err(RuntimeError {
                    message: "`break`/`continue` escaped a function body".to_string(),
                })
            }
        })
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
    ) -> Result<Value, RuntimeError> {
        let mut arg_values = Vec::with_capacity(args.len());
        for a in args {
            arg_values.push(self.eval_expr(a, caller_env)?);
        }
        self.run_function_with_this(decl, arg_values, None)
    }

    fn symbol_at_def(&self, span: phc_span::Span) -> Option<SymbolId> {
        self.resolved
            .symbols
            .iter()
            .find(|s| s.def_span == span)
            .map(|s| s.id)
    }
}

fn truthy(v: &Value) -> Result<bool, RuntimeError> {
    match v {
        Value::Bool(b) => Ok(*b),
        other => Err(RuntimeError {
            message: format!("expected bool in condition, got {other:?}"),
        }),
    }
}

fn eval_unary(op: UnaryOp, v: Value) -> Result<Value, RuntimeError> {
    match (op, v) {
        (UnaryOp::Not, Value::Bool(b)) => Ok(Value::Bool(!b)),
        (UnaryOp::Neg, Value::Int(i)) => Ok(Value::Int(-i)),
        (UnaryOp::Neg, Value::Float(f)) => Ok(Value::Float(-f)),
        (UnaryOp::Await, v) => Ok(v),
        (op, v) => Err(RuntimeError {
            message: format!("unary {op:?} not defined for {v:?}"),
        }),
    }
}

fn eval_binary(op: BinOp, l: Value, r: Value) -> Result<Value, RuntimeError> {
    use BinOp::*;
    use Value::*;
    let mismatched = || RuntimeError {
        message: format!("binary {op:?} not defined for `{l:?}` and `{r:?}`"),
    };
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
            (Int(_), Int(0)) => Err(RuntimeError {
                message: "integer division by zero".to_string(),
            }),
            (Int(a), Int(b)) => Ok(Int(a / b)),
            (Float(a), Float(b)) => Ok(Float(a / b)),
            _ => Err(mismatched()),
        },
        Rem => match (l.clone(), r.clone()) {
            (Int(_), Int(0)) => Err(RuntimeError {
                message: "integer remainder by zero".to_string(),
            }),
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

fn bool_cmp(
    l: &Value,
    r: &Value,
    pred: impl Fn(std::cmp::Ordering) -> bool,
) -> Result<Value, RuntimeError> {
    let ord = match (l, r) {
        (Value::Int(a), Value::Int(b)) => a.cmp(b),
        (Value::Float(a), Value::Float(b)) => a.partial_cmp(b).ok_or_else(|| RuntimeError {
            message: "NaN comparison".to_string(),
        })?,
        (Value::String(a), Value::String(b)) => a.cmp(b),
        (Value::Bool(a), Value::Bool(b)) => a.cmp(b),
        _ => {
            return Err(RuntimeError {
                message: format!("cannot order `{l:?}` and `{r:?}`"),
            })
        }
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
        _ => false,
    }
}

fn eval_cast(v: Value, ty: &phc_ast::TypeRef) -> Result<Value, RuntimeError> {
    let target = ty.path.last().map(|i| i.name.as_str()).unwrap_or("");
    match (v.clone(), target) {
        (Value::Int(i), "float") => Ok(Value::Float(i as f64)),
        (Value::Int(i), "string") => Ok(Value::String(i.to_string())),
        (Value::Float(f), "string") => Ok(Value::String(f.to_string())),
        (Value::Bool(b), "string") => Ok(Value::String(b.to_string())),
        (v, _) => Ok(v),
    }
}

fn parse_int(text: &str) -> Result<Value, RuntimeError> {
    let cleaned: String = text.chars().filter(|c| *c != '_').collect();
    cleaned
        .parse::<i64>()
        .map(Value::Int)
        .map_err(|e| RuntimeError {
            message: format!("invalid integer literal `{text}`: {e}"),
        })
}

fn parse_float(text: &str) -> Result<Value, RuntimeError> {
    let cleaned: String = text.chars().filter(|c| *c != '_').collect();
    cleaned
        .parse::<f64>()
        .map(Value::Float)
        .map_err(|e| RuntimeError {
            message: format!("invalid float literal `{text}`: {e}"),
        })
}

#[cfg(test)]
mod tests;
