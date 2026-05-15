// SPDX-License-Identifier: MIT
//! Inline tests for the tree-walking interpreter.

use crate::{run, RunOutput, Value};
use phc_parser::parse;
use phc_span::FileId;

fn run_src(src: &str) -> RunOutput {
    let parsed = parse(src, FileId(0));
    assert!(
        parsed.diagnostics.is_empty(),
        "parser diagnostics: {:?}",
        parsed.diagnostics
    );
    let file = parsed.file.expect("expected SourceFile");
    let resolved = phc_semantic::resolve(&file);
    let typed = phc_typecheck::typecheck(&file, &resolved);
    run(&file, &resolved, &typed)
}

#[test]
fn missing_main_is_a_runtime_error() {
    let out = run_src("pack a;");
    assert!(
        out.errors.iter().any(|e| e.message.contains("`main`")),
        "expected missing-main error, got {:?}",
        out.errors
    );
}

#[test]
fn empty_main_returns_void() {
    let out = run_src("pack a;\nfunction main(): void {}");
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    assert!(matches!(out.result, Some(Value::Void)));
    assert!(out.stdout.is_empty());
}

#[test]
fn main_can_print_a_plain_string() {
    let out = run_src(
        r#"pack a;
           function main(): void {
               Logger::info("hello");
           }"#,
    );
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    assert_eq!(out.stdout, vec!["hello"]);
}

#[test]
fn string_interpolation_renders_local_variable() {
    let out = run_src(
        r#"pack a;
           function main(): void {
               string $name = "PHC";
               Logger::info("Hello, {$name}!");
           }"#,
    );
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    assert_eq!(out.stdout, vec!["Hello, PHC!"]);
}

#[test]
fn multiple_logger_calls_each_emit_a_line() {
    let out = run_src(
        r#"pack a;
           function main(): void {
               Logger::info("one");
               Logger::info("two");
               Logger::info("three");
           }"#,
    );
    assert_eq!(out.stdout, vec!["one", "two", "three"]);
}

#[test]
fn return_int_propagates_to_run_output() {
    let out = run_src(
        r#"pack a;
           function main(): int { return 42; }"#,
    );
    assert!(matches!(out.result, Some(Value::Int(42))));
}

#[test]
fn integer_arithmetic_runs_through_main() {
    let out = run_src(
        r#"pack a;
           function main(): int { return 1 + 2 * 3 - 4; }"#,
    );
    assert!(matches!(out.result, Some(Value::Int(3))));
}

#[test]
fn flip_binding_with_reassign() {
    let out = run_src(
        r#"pack a;
           function main(): int {
               int $base = 10;
               flip int $score = 0;
               $score := $score + 1;
               $score := $score + $base;
               return $score;
           }"#,
    );
    assert!(matches!(out.result, Some(Value::Int(11))));
}

#[test]
fn if_branch_picks_then() {
    let out = run_src(
        r#"pack a;
           function main(): int {
               if (true) { return 1; } else { return 2; }
           }"#,
    );
    assert!(matches!(out.result, Some(Value::Int(1))));
}

#[test]
fn if_branch_picks_else() {
    let out = run_src(
        r#"pack a;
           function main(): int {
               if (false) { return 1; } else { return 2; }
           }"#,
    );
    assert!(matches!(out.result, Some(Value::Int(2))));
}

#[test]
fn while_loop_accumulates() {
    let out = run_src(
        r#"pack a;
           function main(): int {
               flip int $n = 0;
               flip int $sum = 0;
               while ($n < 10) {
                   $sum := $sum + $n;
                   $n := $n + 1;
               }
               return $sum;
           }"#,
    );
    assert!(matches!(out.result, Some(Value::Int(45))));
}

#[test]
fn break_exits_while_early() {
    let out = run_src(
        r#"pack a;
           function main(): int {
               flip int $n = 0;
               while (true) {
                   if ($n == 3) { break; }
                   $n := $n + 1;
               }
               return $n;
           }"#,
    );
    assert!(matches!(out.result, Some(Value::Int(3))));
}

#[test]
fn user_function_call_with_args() {
    let out = run_src(
        r#"pack a;
           function add(int $a, int $b): int { return $a + $b; }
           function main(): int { return add(3, 4); }"#,
    );
    assert!(matches!(out.result, Some(Value::Int(7))));
}

#[test]
fn nested_user_function_calls() {
    let out = run_src(
        r#"pack a;
           function add(int $a, int $b): int { return $a + $b; }
           function main(): int { return add(add(1, 2), add(3, 4)); }"#,
    );
    assert!(matches!(out.result, Some(Value::Int(10))));
}

#[test]
fn comparison_and_logical_ops() {
    let out = run_src(
        r#"pack a;
           function main(): bool { return 1 < 2 && 3 > 2 || false; }"#,
    );
    assert!(matches!(out.result, Some(Value::Bool(true))));
}

#[test]
fn short_circuit_does_not_evaluate_rhs() {
    // If `||` short-circuits properly, the divide-by-zero on the
    // right is never evaluated and there is no runtime error.
    let out = run_src(
        r#"pack a;
           function main(): bool { return true || (1 / 0) == 0; }"#,
    );
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    assert!(matches!(out.result, Some(Value::Bool(true))));
}

#[test]
fn string_concat_with_plus() {
    let out = run_src(
        r#"pack a;
           function main(): string { return "hello, " + "world"; }"#,
    );
    let Some(Value::String(s)) = out.result else {
        panic!("expected String, got {:?}", out.result);
    };
    assert_eq!(s, "hello, world");
}

#[test]
fn null_coalesce_returns_rhs_when_lhs_null() {
    let out = run_src(
        r#"pack a;
           function main(): int { return null ?? 7; }"#,
    );
    assert!(matches!(out.result, Some(Value::Int(7))));
}

#[test]
fn class_construction_and_field_read() {
    let out = run_src(
        r#"pack a;
           public class Box {
               int $value = 7;
           }
           function main(): int {
               Box $b = Box();
               return $b->value;
           }"#,
    );
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    assert!(matches!(out.result, Some(Value::Int(7))));
}

#[test]
fn constructor_promoted_param_becomes_field() {
    let out = run_src(
        r#"pack a;
           public class Box {
               construct(public int $value) {}
           }
           function main(): int {
               Box $b = Box(42);
               return $b->value;
           }"#,
    );
    assert!(matches!(out.result, Some(Value::Int(42))));
}

#[test]
fn constructor_body_can_assign_field_via_member_assign() {
    let out = run_src(
        r#"pack a;
           public class Box {
               int $value = 0;
               construct(int $seed) {
                   $this->value = $seed * 2;
               }
           }
           function main(): int {
               Box $b = Box(5);
               return $b->value;
           }"#,
    );
    assert!(matches!(out.result, Some(Value::Int(10))));
}

#[test]
fn method_dispatch_returns_field() {
    let out = run_src(
        r#"pack a;
           public class Box {
               construct(public int $value) {}
               public function get(): int { return $this->value; }
           }
           function main(): int {
               Box $b = Box(3);
               return $b->get();
           }"#,
    );
    assert!(matches!(out.result, Some(Value::Int(3))));
}

#[test]
fn member_assign_via_equals_mutates_instance() {
    let out = run_src(
        r#"pack a;
           public class Box {
               flip int $value = 0;
           }
           function main(): int {
               Box $b = Box();
               $b->value = 99;
               return $b->value;
           }"#,
    );
    assert!(matches!(out.result, Some(Value::Int(99))));
}

#[test]
fn instance_aliasing_shares_mutation() {
    // PHP-style by-reference object semantics: $b and $a name the
    // same instance, so a write through $a is visible through $b.
    let out = run_src(
        r#"pack a;
           public class Box {
               flip int $value = 0;
           }
           function main(): int {
               Box $b = Box();
               Box $a = $b;
               $a->value = 5;
               return $b->value;
           }"#,
    );
    assert!(matches!(out.result, Some(Value::Int(5))));
}

#[test]
fn trait_method_mixin_resolves() {
    let out = run_src(
        r#"pack a;
           public trait Doubled {
               function doubled(): int { return $this->value + $this->value; }
           }
           public class Box {
               use Doubled;
               construct(public int $value) {}
           }
           function main(): int {
               Box $b = Box(3);
               return $b->doubled();
           }"#,
    );
    assert!(matches!(out.result, Some(Value::Int(6))));
}

#[test]
fn hello_phc_example_runs_end_to_end() {
    let src = std::fs::read_to_string("../../examples/hello.phc").expect("read hello.phc");
    let out = run_src(&src);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    assert_eq!(out.stdout, vec!["Hello, PHC!"]);
}
