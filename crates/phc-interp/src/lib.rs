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

use phc_ast::{Expr, FunctionDecl, Item, SourceFile, Stmt, StrPart};
use phc_semantic::{Resolved, Symbol, SymbolId, SymbolKind};
use phc_typecheck::Typed;
use std::collections::HashMap;

/// Runtime value carried by the interpreter.
#[derive(Clone, Debug)]
pub enum Value {
    Void,
    Null,
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
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
        }
    }
}

/// Out-of-band control flow result for statement evaluation.
enum Flow {
    Normal,
    Return(Value),
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
                Flow::Return(v) => return Ok(Flow::Return(v)),
            }
        }
        Ok(Flow::Normal)
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
                     (only Local / Expr / Return ship in this commit)"
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
            Expr::Var { span, .. } => {
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
            Expr::Call { callee, args, .. } => self.eval_call(callee, args, env),
            Expr::Paren { inner, .. } => self.eval_expr(inner, env),
            other => Err(RuntimeError {
                message: format!(
                    "expression {other:?} is not yet supported by this interpreter \
                     (literals / Var / Call / StrLit / Paren ship in this commit)"
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
        Err(RuntimeError {
            message: format!(
                "call form {callee:?} is not yet supported by this interpreter; \
                 only the `Logger::info` builtin ships in this commit"
            ),
        })
    }

    fn symbol_at_def(&self, span: phc_span::Span) -> Option<SymbolId> {
        self.resolved
            .symbols
            .iter()
            .find(|s| s.def_span == span)
            .map(|s| s.id)
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
