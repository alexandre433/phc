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
fn enum_static_access_yields_variant_value() {
    let out = run_src(
        r#"pack a;
           public enum Method { Get, Post }
           function main(): bool {
               return Method::Get == Method::Get;
           }"#,
    );
    assert!(matches!(out.result, Some(Value::Bool(true))));
}

#[test]
fn match_dispatch_on_enum_variant() {
    let out = run_src(
        r#"pack a;
           public enum Status { Ok, NotFound, Gone }
           function classify(Status $s): int {
               return match ($s) {
                   Status::Ok => 0,
                   Status::NotFound | Status::Gone => 404,
                   _ => -1,
               };
           }
           function main(): int {
               return classify(Status::NotFound);
           }"#,
    );
    assert!(matches!(out.result, Some(Value::Int(404))));
}

#[test]
fn match_var_pattern_binds_scrutinee() {
    let out = run_src(
        r#"pack a;
           function classify(int $code): int {
               return match ($code) {
                   $n if $n > 500 => 500,
                   _ => 0,
               };
           }
           function main(): int { return classify(750); }"#,
    );
    assert!(matches!(out.result, Some(Value::Int(500))));
}

#[test]
fn match_literal_pattern_routes_through_arms() {
    let out = run_src(
        r#"pack a;
           function name(int $n): string {
               return match ($n) {
                   1 => "one",
                   2 => "two",
                   _ => "many",
               };
           }
           function main(): string { return name(2); }"#,
    );
    let Some(Value::String(s)) = out.result else {
        panic!("expected String");
    };
    assert_eq!(s, "two");
}

#[test]
fn no_match_arm_is_a_runtime_error() {
    let out = run_src(
        r#"pack a;
           function f(int $n): int {
               return match ($n) {
                   1 => 100,
               };
           }
           function main(): int { return f(99); }"#,
    );
    assert!(out
        .errors
        .iter()
        .any(|e| e.message.contains("no match arm")));
}

#[test]
fn lambda_with_no_params_invoked_inline() {
    let out = run_src(
        r#"pack a;
           function main(): int {
               int $r = (() => 42)();
               return $r;
           }"#,
    );
    assert!(matches!(out.result, Some(Value::Int(42))));
}

#[test]
fn lambda_param_used_in_body() {
    let out = run_src(
        r#"pack a;
           function main(): int {
               int $r = ((int $n): int => $n * 3)(7);
               return $r;
           }"#,
    );
    assert!(matches!(out.result, Some(Value::Int(21))));
}

#[test]
fn lambda_captures_outer_binding_by_value() {
    let out = run_src(
        r#"pack a;
           function main(): int {
               int $factor = 5;
               int $r = ((int $n): int => $n * $factor)(4);
               return $r;
           }"#,
    );
    assert!(matches!(out.result, Some(Value::Int(20))));
}

#[test]
fn lambda_stored_in_local_then_called() {
    let out = run_src(
        r#"pack a;
           function main(): int {
               int $r = call_with_seven((int $n): int => $n + 1);
               return $r;
           }
           function call_with_seven(int $f): int { return $f(7); }"#,
    );
    assert!(matches!(out.result, Some(Value::Int(8))));
}

#[test]
fn lambda_block_body_with_return() {
    let out = run_src(
        r#"pack a;
           function main(): int {
               int $r = ((int $n): int => { return $n + 100; })(5);
               return $r;
           }"#,
    );
    assert!(matches!(out.result, Some(Value::Int(105))));
}

#[test]
fn result_ok_round_trip() {
    let out = run_src(
        r#"pack a;
           function main(): result<int, string> { return result::ok(42); }"#,
    );
    match out.result {
        Some(Value::ResultOk(v)) => assert!(matches!(*v, Value::Int(42))),
        other => panic!("expected ResultOk(42), got {other:?}"),
    }
}

#[test]
fn option_some_and_none_round_trip() {
    let out = run_src(
        r#"pack a;
           function main(): option<int> { return option::some(7); }"#,
    );
    assert!(matches!(out.result, Some(Value::OptionSome(_))));

    let out = run_src(
        r#"pack a;
           function main(): option<int> { return option::none; }"#,
    );
    assert!(matches!(out.result, Some(Value::OptionNone)));
}

#[test]
fn try_propagation_short_circuits_on_err() {
    let out = run_src(
        r#"pack a;
           function inner(): result<int, string> { return result::err("nope"); }
           function caller(): result<int, string> {
               int $v = inner()?;
               return result::ok($v + 1);
           }
           function main(): result<int, string> { return caller(); }"#,
    );
    match out.result {
        Some(Value::ResultErr(e)) => match *e {
            Value::String(s) => assert_eq!(s, "nope"),
            other => panic!("expected String inside ResultErr, got {other:?}"),
        },
        other => panic!("expected ResultErr, got {other:?}"),
    }
}

#[test]
fn try_propagation_unwraps_on_ok() {
    let out = run_src(
        r#"pack a;
           function inner(): result<int, string> { return result::ok(10); }
           function caller(): result<int, string> {
               int $v = inner()?;
               return result::ok($v + 1);
           }
           function main(): result<int, string> { return caller(); }"#,
    );
    match out.result {
        Some(Value::ResultOk(v)) => assert!(matches!(*v, Value::Int(11))),
        other => panic!("expected ResultOk(11), got {other:?}"),
    }
}

#[test]
fn string_to_int_returns_result() {
    let out = run_src(
        r#"pack a;
           function main(): result<int, string> {
               int $value = "42"->toInt()?;
               return result::ok($value);
           }"#,
    );
    match out.result {
        Some(Value::ResultOk(v)) => assert!(matches!(*v, Value::Int(42))),
        other => panic!("expected ResultOk(42), got {other:?}"),
    }
}

#[test]
fn string_to_int_propagates_err_for_garbage() {
    let out = run_src(
        r#"pack a;
           function main(): result<int, string> {
               int $value = "not-a-number"->toInt()?;
               return result::ok($value);
           }"#,
    );
    assert!(matches!(out.result, Some(Value::ResultErr(_))));
}

#[test]
fn option_some_unwrapped_via_try() {
    let out = run_src(
        r#"pack a;
           function get(): option<int> { return option::some(99); }
           function main(): option<int> {
               int $v = get()?;
               return option::some($v);
           }"#,
    );
    match out.result {
        Some(Value::OptionSome(v)) => assert!(matches!(*v, Value::Int(99))),
        other => panic!("expected OptionSome(99), got {other:?}"),
    }
}

#[test]
fn async_function_runs_synchronously() {
    let out = run_src(
        r#"pack a;
           async function compute(): int { return 21 + 21; }
           function main(): int { return await compute(); }"#,
    );
    assert!(matches!(out.result, Some(Value::Int(42))));
}

#[test]
fn await_passes_value_through() {
    let out = run_src(
        r#"pack a;
           function main(): int { return await 7; }"#,
    );
    assert!(matches!(out.result, Some(Value::Int(7))));
}

#[test]
fn http_get_stub_returns_result_ok_string() {
    let out = run_src(
        r#"pack a;
           async function fetch(string $url): result<bytes, string> {
               return await Http::get($url);
           }
           function main(): result<bytes, string> {
               return await fetch("https://example.com");
           }"#,
    );
    match out.result {
        Some(Value::ResultOk(v)) => match *v {
            Value::String(s) => assert_eq!(s, "<bytes from https://example.com>"),
            other => panic!("expected String inside ResultOk, got {other:?}"),
        },
        other => panic!("expected ResultOk, got {other:?}"),
    }
}

#[test]
fn hello_phc_example_runs_end_to_end() {
    let src = std::fs::read_to_string("../../examples/hello.phc").expect("read hello.phc");
    let out = run_src(&src);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    assert_eq!(out.stdout, vec!["Hello, PHC!"]);
}

#[test]
fn string_methods_d025_run_in_interp() {
    let src = r#"pack demo;
function main(): void {
    string $s = "  Hello, PHC!  ";
    string $t = $s->trim();
    if ($t->len() == 11) { Logger::info("len ok"); }
    if ($t->contains("PHC")) { Logger::info("contains ok"); }
    if ($t->startsWith("Hello")) { Logger::info("starts ok"); }
    if ($t->endsWith("PHC!")) { Logger::info("ends ok"); }
    if ($t->upper() == "HELLO, PHC!") { Logger::info("upper ok"); }
    if ($t->lower() == "hello, phc!") { Logger::info("lower ok"); }
}
"#;
    let out = run_src(src);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let expected = vec![
        "len ok".to_string(),
        "contains ok".to_string(),
        "starts ok".to_string(),
        "ends ok".to_string(),
        "upper ok".to_string(),
        "lower ok".to_string(),
    ];
    assert_eq!(out.stdout, expected);
}

#[test]
fn list_d027_runs_in_interp() {
    let src = r#"pack demo;
function main(): void {
    list<int> $xs = list();
    $xs->push(10);
    $xs->push(20);
    $xs->push(30);
    if ($xs->len() == 3) { Logger::info("len ok"); }
    if ($xs->at(1) == 20) { Logger::info("at ok"); }
    if ($xs[0] == 10) { Logger::info("index ok"); }
    int $sum = 0;
    flip int $acc = 0;
    for (int $x in $xs) { $acc := $acc + $x; }
    if ($acc == 60) { Logger::info("for ok"); }
}
"#;
    let out = run_src(src);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let expected = vec![
        "len ok".to_string(),
        "at ok".to_string(),
        "index ok".to_string(),
        "for ok".to_string(),
    ];
    assert_eq!(out.stdout, expected);
}

#[test]
fn result_option_d026_runs_in_interp() {
    let src = r#"pack demo;
function main(): void {
    result<int, string> $ok = result::ok(42);
    result<int, string> $err = result::err("nope");
    if ($ok->isOk()) { Logger::info("ok-isOk"); }
    if ($err->isErr()) { Logger::info("err-isErr"); }
    if ($ok->unwrapOr(0) == 42) { Logger::info("ok-unwrap"); }
    if ($err->unwrapOr(99) == 99) { Logger::info("err-unwrap"); }

    option<int> $some = option::some(7);
    option<int> $none = option::none;
    if ($some->isSome()) { Logger::info("some-isSome"); }
    if ($none->isNone()) { Logger::info("none-isNone"); }
    if ($some->unwrapOr(0) == 7) { Logger::info("some-unwrap"); }
    if ($none->unwrapOr(99) == 99) { Logger::info("none-unwrap"); }
    if ($none->orElse(option::some(5))->unwrapOr(0) == 5) { Logger::info("orElse"); }
}
"#;
    let out = run_src(src);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let expected = vec![
        "ok-isOk".to_string(),
        "err-isErr".to_string(),
        "ok-unwrap".to_string(),
        "err-unwrap".to_string(),
        "some-isSome".to_string(),
        "none-isNone".to_string(),
        "some-unwrap".to_string(),
        "none-unwrap".to_string(),
        "orElse".to_string(),
    ];
    assert_eq!(out.stdout, expected);
}

#[test]
fn method_chain_inference_propagates_for_stdlib_chains() {
    // Verifies the typecheck patch: chained method calls on
    // stdlib types now resolve through the codegen e2e path
    // (interpreter doesn't need this — included as a sanity
    // check that nothing regressed).
    let src = r#"pack demo;
function main(): void {
    string $s = "  PHC ROCKS  ";
    if ($s->trim()->lower() == "phc rocks") { Logger::info("string chain"); }
    list<int> $xs = list();
    $xs->push(7);
    $xs->push(35);
    if ($xs->at(0) + $xs->at(1) == 42) { Logger::info("list arith"); }
    option<int> $a = option::none;
    option<int> $b = option::some(99);
    if ($a->orElse($b)->unwrapOr(0) == 99) { Logger::info("option chain"); }
}
"#;
    let out = run_src(src);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    assert_eq!(
        out.stdout,
        vec![
            "string chain".to_string(),
            "list arith".to_string(),
            "option chain".to_string()
        ]
    );
}

#[test]
fn d024_fn_type_stored_lambda_invocation() {
    // Lambdas now have a type — bind, pass through, return,
    // invoke. Interp side has no new code (Value::Lambda already
    // exists); test mostly guards that parse + typecheck + interp
    // accept the new `fn(...): R` annotation shape.
    let src = r#"pack demo;
function main(): void {
    fn(int): int $double = (int $n): int => $n * 2;
    int $r = $double(7);
    if ($r == 14) { Logger::info("stored lambda call"); }
    fn(int, int): int $add = (int $a, int $b): int => $a + $b;
    if ($add(20, 22) == 42) { Logger::info("two-arg call"); }
}
"#;
    let out = run_src(src);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    assert_eq!(
        out.stdout,
        vec!["stored lambda call".to_string(), "two-arg call".to_string()]
    );
}

#[test]
fn map_d028_runs_in_interp() {
    let src = r#"pack demo;
function main(): void {
    map<string, int> $m = map();
    $m->set("a", 1);
    $m->set("b", 2);
    $m->set("a", 10);
    if ($m->len() == 2) { Logger::info("len ok"); }
    if ($m->has("a")) { Logger::info("has ok"); }
    if ($m->get("a")->unwrapOr(0) == 10) { Logger::info("get ok"); }
    if ($m->get("missing")->isNone()) { Logger::info("missing ok"); }
}
"#;
    let out = run_src(src);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    assert_eq!(
        out.stdout,
        vec![
            "len ok".to_string(),
            "has ok".to_string(),
            "get ok".to_string(),
            "missing ok".to_string()
        ]
    );
}

#[test]
fn result_option_d029_closure_methods_run_in_interp() {
    let src = r#"pack demo;
function main(): void {
    result<int, string> $ok = result::ok(20);
    result<int, string> $err = result::err("nope");

    fn(int): int $double = (int $n): int => $n * 2;
    result<int, string> $mapped = $ok->map($double);
    if ($mapped->unwrapOr(0) == 40) { Logger::info("result.map ok"); }
    result<int, string> $mapErr = $err->map($double);
    if ($mapErr->isErr()) { Logger::info("result.map passthrough"); }

    fn(int): result<int, string> $half = (int $n): result<int, string> =>
        result::ok($n + 1);
    result<int, string> $chained = $ok->andThen($half);
    if ($chained->unwrapOr(0) == 21) { Logger::info("result.andThen ok"); }

    if ($ok->unwrap() == 20) { Logger::info("result.unwrap ok"); }

    option<int> $some = option::some(7);
    option<int> $none = option::none;

    fn(int): int $inc = (int $n): int => $n + 1;
    option<int> $mappedOpt = $some->map($inc);
    if ($mappedOpt->unwrapOr(0) == 8) { Logger::info("option.map ok"); }
    if ($none->map($inc)->isNone()) { Logger::info("option.map none"); }

    fn(int): option<int> $doubleSome = (int $n): option<int> =>
        option::some($n * 2);
    if ($some->andThen($doubleSome)->unwrapOr(0) == 14) {
        Logger::info("option.andThen ok");
    }

    result<int, string> $promoted = $some->okOr("missing");
    if ($promoted->unwrapOr(0) == 7) { Logger::info("option.okOr some"); }
    result<int, string> $demoted = $none->okOr("missing");
    if ($demoted->isErr()) { Logger::info("option.okOr none"); }

    if ($some->unwrap() == 7) { Logger::info("option.unwrap ok"); }
}
"#;
    let out = run_src(src);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let expected = vec![
        "result.map ok".to_string(),
        "result.map passthrough".to_string(),
        "result.andThen ok".to_string(),
        "result.unwrap ok".to_string(),
        "option.map ok".to_string(),
        "option.map none".to_string(),
        "option.andThen ok".to_string(),
        "option.okOr some".to_string(),
        "option.okOr none".to_string(),
        "option.unwrap ok".to_string(),
    ];
    assert_eq!(out.stdout, expected);
}

#[test]
fn list_d030_closure_methods_run_in_interp() {
    let src = r#"pack demo;
function main(): void {
    list<int> $xs = list();
    $xs->push(1);
    $xs->push(2);
    $xs->push(3);

    fn(int): int $dbl = (int $n): int => $n * 2;
    list<int> $doubled = $xs->map($dbl);
    if ($doubled->len() == 3 && $doubled->at(2) == 6) { Logger::info("map ok"); }

    fn(int): bool $even = (int $n): bool => ($n + ($n / 2 * -2)) == 0;
    list<int> $evens = $xs->filter($even);
    if ($evens->len() == 1 && $evens->at(0) == 2) { Logger::info("filter ok"); }

    flip int $sum = 0;
    fn(int): void $add = (int $n): void => Logger::info("v");
    $xs->forEach($add);
    Logger::info("forEach done");
}
"#;
    let out = run_src(src);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let expected = vec![
        "map ok".to_string(),
        "filter ok".to_string(),
        "v".to_string(),
        "v".to_string(),
        "v".to_string(),
        "forEach done".to_string(),
    ];
    assert_eq!(out.stdout, expected);
}

#[test]
fn set_d031_runs_in_interp() {
    let src = r#"pack demo;
function main(): void {
    set<string> $s = set();
    if ($s->add("ada")) { Logger::info("add new"); }
    if (!$s->add("ada")) { Logger::info("add dup"); }
    $s->add("alan");
    if ($s->len() == 2) { Logger::info("len ok"); }
    if ($s->has("ada")) { Logger::info("has ok"); }
    if (!$s->has("grace")) { Logger::info("miss ok"); }
    if ($s->remove("ada")) { Logger::info("remove ok"); }
    if (!$s->has("ada")) { Logger::info("post-remove miss"); }
    if ($s->len() == 1) { Logger::info("len post-remove"); }
}
"#;
    let out = run_src(src);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    let expected = vec![
        "add new".to_string(),
        "add dup".to_string(),
        "len ok".to_string(),
        "has ok".to_string(),
        "miss ok".to_string(),
        "remove ok".to_string(),
        "post-remove miss".to_string(),
        "len post-remove".to_string(),
    ];
    assert_eq!(out.stdout, expected);
}

#[test]
fn io_d032_runs_in_interp() {
    let src = r#"pack demo;
function main(): void {
    io::println("hello");
    io::eprintln("ohno");
    io::print("no-newline ");
    io::println("rest");
    // readLine returns option::none in interp (no stdin); just
    // exercise the call.
    option<string> $line = io::readLine();
    if ($line->isNone()) { io::println("eof ok"); }
}
"#;
    let out = run_src(src);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    assert_eq!(
        out.stdout,
        vec![
            "hello".to_string(),
            "ohno".to_string(),
            "no-newline ".to_string(),
            "rest".to_string(),
            "eof ok".to_string(),
        ]
    );
}

#[test]
fn assert_d033_passing_cases() {
    let src = r#"pack demo;
function main(): void {
    assert::eq(2 + 3, 5);
    assert::neq(1, 2);
    assert::isTrue(true);
    assert::isFalse(false);
    assert::eq("phc", "phc");
    io::println("done");
}
"#;
    let out = run_src(src);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    assert_eq!(out.stdout, vec!["done".to_string()]);
}

#[test]
fn assert_d033_eq_failure_surfaces_as_runtime_error() {
    let src = r#"pack demo;
function main(): void {
    assert::eq(1, 2);
}
"#;
    let out = run_src(src);
    assert!(
        out.errors
            .iter()
            .any(|e| e.message.contains("assertion failed: assert::eq")),
        "expected eq assertion error, got {:?}",
        out.errors
    );
}

#[test]
fn assert_d033_fail_with_message() {
    let src = r#"pack demo;
function main(): void {
    assert::fail("boom");
}
"#;
    let out = run_src(src);
    assert!(
        out.errors.iter().any(|e| e.message.contains("boom")),
        "expected fail message, got {:?}",
        out.errors
    );
}

#[test]
fn numeric_d034_runs_in_interp() {
    let src = r#"pack demo;
function main(): void {
    result<int, parseError> $parsed = int::parse("42");
    assert::eq($parsed->unwrapOr(0), 42);
    result<int, parseError> $bad = int::parse("nope");
    assert::isTrue($bad->isErr());

    assert::eq(int::min(7, 3), 3);
    assert::eq(int::max(7, 3), 7);
    assert::eq(int::abs(-9), 9);

    result<float, parseError> $f = float::parse("2.5");
    assert::eq($f->unwrapOr(0.0), 2.5);
    assert::eq(float::min(1.5, 0.25), 0.25);
    assert::eq(float::max(1.5, 0.25), 1.5);
    assert::eq(float::abs(-3.5), 3.5);
    assert::isFalse(float::isNaN(1.0));

    io::println("numeric ok");
}
"#;
    let out = run_src(src);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    assert_eq!(out.stdout, vec!["numeric ok".to_string()]);
}

#[test]
fn list_d037_fold_any_all_find_run_in_interp() {
    let src = r#"pack demo;
function main(): void {
    list<int> $xs = list();
    $xs->push(1);
    $xs->push(2);
    $xs->push(3);
    $xs->push(4);

    fn(int, int): int $sum = (int $acc, int $x): int => $acc + $x;
    assert::eq($xs->fold(0, $sum), 10);

    fn(int): bool $isEven = (int $n): bool => (($n + ($n / 2 * -2)) == 0);
    assert::isTrue($xs->any($isEven));

    fn(int): bool $isPositive = (int $n): bool => $n > 0;
    assert::isTrue($xs->all($isPositive));

    fn(int): bool $eqThree = (int $n): bool => $n == 3;
    option<int> $found = $xs->find($eqThree);
    assert::eq($found->unwrapOr(0), 3);

    fn(int): bool $eqHundred = (int $n): bool => $n == 100;
    option<int> $miss = $xs->find($eqHundred);
    assert::isTrue($miss->isNone());

    io::println("d037 ok");
}
"#;
    let out = run_src(src);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    assert_eq!(out.stdout, vec!["d037 ok".to_string()]);
}

#[test]
fn assert_d041_approx_eq_passes_for_close_floats() {
    let src = r#"pack demo;
function main(): void {
    assert::approxEq(1.0, 1.0);
    assert::approxEq(0.1 + 0.2, 0.30000000000000004);
    assert::approxEq(0, 0);
    io::println("approxEq ok");
}
"#;
    let out = run_src(src);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    assert_eq!(out.stdout, vec!["approxEq ok".to_string()]);
}

#[test]
fn assert_d041_approx_eq_fails_for_distant_floats() {
    let src = r#"pack demo;
function main(): void {
    assert::approxEq(1.0, 2.0);
}
"#;
    let out = run_src(src);
    assert!(
        out.errors
            .iter()
            .any(|e| e.message.contains("assert::approxEq")),
        "expected approxEq failure, got {:?}",
        out.errors
    );
}

#[test]
fn assert_d041_throws_passes_when_lambda_panics() {
    let src = r#"pack demo;
function main(): void {
    assert::throws(((): void => {
        list<int> $xs = list();
        int $_ = $xs->at(0);
    }));
    io::println("throws ok");
}
"#;
    let out = run_src(src);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    assert_eq!(out.stdout, vec!["throws ok".to_string()]);
}

#[test]
fn assert_d041_throws_fails_when_lambda_does_not_panic() {
    let src = r#"pack demo;
function main(): void {
    assert::throws(((): void => {
        int $x = 1;
    }));
}
"#;
    let out = run_src(src);
    assert!(
        out.errors
            .iter()
            .any(|e| e.message.contains("assert::throws")),
        "expected throws failure, got {:?}",
        out.errors
    );
}

#[test]
fn d042_tostring_used_in_string_interpolation() {
    let src = r#"pack demo;

public class Point {
    construct(public int $x, public int $y) {}
    public function toString(): string {
        return "({$this->x}, {$this->y})";
    }
}

function main(): void {
    Point $p = Point(3, 4);
    string $s = "point is {$p}";
    assert::eq($s, "point is (3, 4)");
    io::println("toString ok");
}
"#;
    let out = run_src(src);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    assert_eq!(out.stdout, vec!["toString ok".to_string()]);
}

#[test]
fn d042_class_without_tostring_falls_back_to_default_display() {
    let src = r#"pack demo;

public class Bare {
    construct(public int $x) {}
}

function main(): void {
    Bare $b = Bare(7);
    string $s = "val is {$b}";
    // No toString — falls back to "<Bare instance>".
    assert::eq($s, "val is <Bare instance>");
    io::println("fallback ok");
}
"#;
    let out = run_src(src);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    assert_eq!(out.stdout, vec!["fallback ok".to_string()]);
}

#[test]
fn d043_take_returns_first_n_elements() {
    let src = r#"pack demo;
function main(): void {
    list<int> $xs = list();
    $xs->push(1);
    $xs->push(2);
    $xs->push(3);
    $xs->push(4);
    $xs->push(5);
    list<int> $t = $xs->take(3);
    assert::eq($t->len(), 3);
    assert::eq($t->at(0), 1);
    assert::eq($t->at(2), 3);
    list<int> $t0 = $xs->take(0);
    assert::eq($t0->len(), 0);
    list<int> $tall = $xs->take(99);
    assert::eq($tall->len(), 5);
    io::println("take ok");
}
"#;
    let out = run_src(src);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    assert_eq!(out.stdout, vec!["take ok".to_string()]);
}

#[test]
fn d043_drop_returns_elements_after_n() {
    let src = r#"pack demo;
function main(): void {
    list<int> $xs = list();
    $xs->push(10);
    $xs->push(20);
    $xs->push(30);
    $xs->push(40);
    $xs->push(50);
    list<int> $d = $xs->drop(2);
    assert::eq($d->len(), 3);
    assert::eq($d->at(0), 30);
    assert::eq($d->at(2), 50);
    list<int> $dall = $xs->drop(0);
    assert::eq($dall->len(), 5);
    list<int> $dempty = $xs->drop(99);
    assert::eq($dempty->len(), 0);
    io::println("drop ok");
}
"#;
    let out = run_src(src);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    assert_eq!(out.stdout, vec!["drop ok".to_string()]);
}

#[test]
fn d043_take_drop_compose() {
    let src = r#"pack demo;
function main(): void {
    list<int> $xs = list();
    $xs->push(1);
    $xs->push(2);
    $xs->push(3);
    $xs->push(4);
    $xs->push(5);
    $xs->push(6);
    list<int> $mid = $xs->drop(2)->take(2);
    assert::eq($mid->len(), 2);
    assert::eq($mid->at(0), 3);
    assert::eq($mid->at(1), 4);
    io::println("compose ok");
}
"#;
    let out = run_src(src);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    assert_eq!(out.stdout, vec!["compose ok".to_string()]);
}

#[test]
fn d044_reverse_reverses_list() {
    let src = r#"pack demo;
function main(): void {
    list<int> $xs = list();
    $xs->push(1);
    $xs->push(2);
    $xs->push(3);
    list<int> $r = $xs->reverse();
    assert::eq($r->len(), 3);
    assert::eq($r->at(0), 3);
    assert::eq($r->at(2), 1);
    list<int> $empty = list();
    assert::eq($empty->reverse()->len(), 0);
    io::println("reverse ok");
}
"#;
    let out = run_src(src);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    assert_eq!(out.stdout, vec!["reverse ok".to_string()]);
}

#[test]
fn d044_concat_appends_lists() {
    let src = r#"pack demo;
function main(): void {
    list<int> $xs = list();
    $xs->push(1);
    $xs->push(2);
    list<int> $ys = list();
    $ys->push(3);
    $ys->push(4);
    list<int> $z = $xs->concat($ys);
    assert::eq($z->len(), 4);
    assert::eq($z->at(0), 1);
    assert::eq($z->at(3), 4);
    // sources unchanged
    assert::eq($xs->len(), 2);
    assert::eq($ys->len(), 2);
    io::println("concat ok");
}
"#;
    let out = run_src(src);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    assert_eq!(out.stdout, vec!["concat ok".to_string()]);
}

#[test]
fn d044_join_concatenates_strings_with_separator() {
    let src = r#"pack demo;
function main(): void {
    list<string> $xs = list();
    $xs->push("a");
    $xs->push("b");
    $xs->push("c");
    string $r = $xs->join(", ");
    assert::eq($r, "a, b, c");
    string $r2 = $xs->join("");
    assert::eq($r2, "abc");
    list<string> $empty = list();
    assert::eq($empty->join("-"), "");
    io::println("join ok");
}
"#;
    let out = run_src(src);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    assert_eq!(out.stdout, vec!["join ok".to_string()]);
}

#[test]
fn d047_string_split_produces_list() {
    let src = r#"pack demo;
function main(): void {
    list<string> $parts = "a,b,c"->split(",");
    assert::eq($parts->len(), 3);
    assert::eq($parts->at(0), "a");
    assert::eq($parts->at(1), "b");
    assert::eq($parts->at(2), "c");
    list<string> $no_match = "hello"->split(",");
    assert::eq($no_match->len(), 1);
    assert::eq($no_match->at(0), "hello");
    list<string> $multi = "ab--cd--ef"->split("--");
    assert::eq($multi->len(), 3);
    io::println("split ok");
}
"#;
    let out = run_src(src);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    assert_eq!(out.stdout, vec!["split ok".to_string()]);
}

#[test]
fn d047_string_repeat_produces_repeated_string() {
    let src = r#"pack demo;
function main(): void {
    assert::eq("ab"->repeat(3), "ababab");
    assert::eq("x"->repeat(1), "x");
    assert::eq("hello"->repeat(0), "");
    io::println("repeat ok");
}
"#;
    let out = run_src(src);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    assert_eq!(out.stdout, vec!["repeat ok".to_string()]);
}

#[test]
fn d047_string_index_of_returns_option() {
    let src = r#"pack demo;
function main(): void {
    option<int> $found = "hello world"->indexOf("world");
    assert::eq($found->isSome(), true);
    assert::eq($found->unwrapOr(-1), 6);
    option<int> $missing = "hello"->indexOf("xyz");
    assert::eq($missing->isNone(), true);
    assert::eq("abcabc"->indexOf("b")->unwrapOr(-1), 1);
    io::println("indexOf ok");
}
"#;
    let out = run_src(src);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    assert_eq!(out.stdout, vec!["indexOf ok".to_string()]);
}

#[test]
fn d047_string_replace_replaces_all_occurrences() {
    let src = r#"pack demo;
function main(): void {
    assert::eq("aabbaa"->replace("a", "x"), "xxbbxx");
    assert::eq("hello"->replace("l", "r"), "herro");
    assert::eq("abc"->replace("xyz", "Q"), "abc");
    assert::eq("abc"->replace("", "Q"), "abc");
    io::println("replace ok");
}
"#;
    let out = run_src(src);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    assert_eq!(out.stdout, vec!["replace ok".to_string()]);
}

#[test]
fn d048_float_math_and_conversions() {
    let src = r#"pack demo;
function main(): void {
    assert::approxEq(float::sqrt(4.0), 2.0);
    assert::approxEq(float::floor(2.9), 2.0);
    assert::approxEq(float::ceil(2.1), 3.0);
    assert::approxEq(float::round(2.5), 3.0);
    assert::approxEq(float::pow(2.0, 10.0), 1024.0);
    assert::eq(float::toInt(3.9), 3);
    assert::eq(float::toInt(-3.9), -3);
    assert::approxEq(int::toFloat(7), 7.0);
    assert::eq(int::pow(2, 10), 1024);
    assert::eq(int::pow(3, 0), 1);
    io::println("math ok");
}
"#;
    let out = run_src(src);
    assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
    assert_eq!(out.stdout, vec!["math ok".to_string()]);
}
