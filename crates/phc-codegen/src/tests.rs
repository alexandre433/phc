// SPDX-License-Identifier: MIT
//! Inline tests for the C emitter — string match against the
//! emitted source. End-to-end "compile + run + assert stdout" lives
//! in `phc-build`'s integration tests.

use crate::emit_c;
use phc_parser::parse;
use phc_span::FileId;

fn emit_for(src: &str) -> String {
    let parsed = parse(src, FileId(0));
    assert!(
        parsed.diagnostics.is_empty(),
        "parser diagnostics: {:?}",
        parsed.diagnostics
    );
    let file = parsed.file.expect("expected SourceFile");
    let resolved = phc_semantic::resolve(&file);
    let typed = phc_typecheck::typecheck(&file, &resolved);
    let out = emit_c(&file, &resolved, &typed);
    out.c_source
}

#[test]
fn empty_main_emits_void_function_and_entry() {
    let src = "pack a;\nfunction main(): void {}";
    let c = emit_for(src);
    assert!(c.contains("static void phc_main(void)"));
    assert!(c.contains("int main(int argc, char** argv)"));
    assert!(c.contains("phc_main();"));
}

#[test]
fn string_local_emits_runtime_call() {
    let src = r#"pack a;
        function main(): void {
            string $name = "PHC";
        }"#;
    let c = emit_for(src);
    assert!(c.contains("phc_string phc_var_name = "));
    assert!(c.contains("phc_string_lit(\"PHC\")"));
}

#[test]
fn logger_info_emits_phc_print() {
    let src = r#"pack a;
        function main(): void {
            Logger::info("hello");
        }"#;
    let c = emit_for(src);
    assert!(c.contains("phc_print("));
    assert!(c.contains("phc_string_lit(\"hello\")"));
}

#[test]
fn shared_borrow_arg_passes_operand_through() {
    // `&$u` is transparent in the C backend: the borrow operand is
    // emitted directly, not a `codegen TODO expr` panic. Member access
    // on a borrowed class param resolves to a struct-pointer `->`.
    let src = r#"pack a;
        public class User { construct(public string $name) {} }
        function read(&User $u): string { return $u->name; }
        function main(): void {
            User $u = User("A");
            Logger::info(read(&$u));
        }"#;
    let c = emit_for(src);
    assert!(
        !c.contains("codegen TODO expr"),
        "borrow arg should not hit the unsupported-expr stub:\n{c}"
    );
    // The borrow operand `$u` is forwarded verbatim to the call.
    assert!(c.contains("phc_read(phc_var_u)"));
    // Member access on the class-typed borrow param emits `->`.
    assert!(c.contains("(phc_var_u)->name"));
}

#[test]
fn string_interpolation_emits_concat_chain() {
    let src = r#"pack a;
        function main(): void {
            string $name = "PHC";
            Logger::info("Hello, {$name}!");
        }"#;
    let c = emit_for(src);
    // Three pieces concat-chained: "Hello, " + name + "!".
    assert!(c.contains("phc_concat2"));
    assert!(c.contains("phc_to_string(phc_var_name)"));
    assert!(c.contains("phc_string_lit(\"Hello, \")"));
    assert!(c.contains("phc_string_lit(\"!\")"));
}

#[test]
fn integer_literal_uses_int64() {
    let src = "pack a;\nfunction main(): void { int $x = 42; }";
    let c = emit_for(src);
    assert!(c.contains("int64_t phc_var_x = (int64_t)42"));
}

#[test]
fn user_function_call_dispatches_by_name() {
    let src = r#"pack a;
        function helper(): void {}
        function main(): void { helper(); }"#;
    let c = emit_for(src);
    assert!(c.contains("static void phc_helper(void)"));
    assert!(c.contains("phc_helper()"));
}

#[test]
fn function_with_int_args_and_return() {
    let src = r#"pack a;
        function add(int $a, int $b): int { return $a + $b; }
        function main(): void {}"#;
    let c = emit_for(src);
    assert!(c.contains("static int64_t phc_add(int64_t phc_var_a, int64_t phc_var_b)"));
    assert!(c.contains("return ((phc_var_a) + (phc_var_b));"));
}

#[test]
fn if_else_chain_emits_c() {
    let src = r#"pack a;
        function main(): void {
            if (true) { return; } else { return; }
        }"#;
    let c = emit_for(src);
    assert!(c.contains("if (true)"));
    assert!(c.contains("} else {"));
}

#[test]
fn while_loop_emits_c() {
    let src = r#"pack a;
        function main(): void {
            while (true) { break; }
        }"#;
    let c = emit_for(src);
    assert!(c.contains("while (true)"));
    assert!(c.contains("break;"));
}

#[test]
fn reassign_emits_assignment() {
    let src = r#"pack a;
        function main(): void {
            flip int $x = 0;
            $x := $x + 1;
        }"#;
    let c = emit_for(src);
    assert!(c.contains("phc_var_x = ((phc_var_x) + ((int64_t)1));"));
}

#[test]
fn string_concat_emits_phc_concat2() {
    let src = r#"pack a;
        function greet(): string { return "hello, " + "world"; }
        function main(): void {}"#;
    let c = emit_for(src);
    assert!(c.contains("phc_concat2("));
    assert!(c.contains("phc_string_lit(\"hello, \")"));
    assert!(c.contains("phc_string_lit(\"world\")"));
}

#[test]
fn class_emits_struct_constructor_method() {
    let src = r#"pack a;
        public class Box {
            construct(public int $value) {}
            public function get(): int { return $this->value; }
        }
        function main(): void {}"#;
    let c = emit_for(src);
    assert!(c.contains("typedef struct phc_obj_Box phc_obj_Box;"));
    assert!(c.contains("struct phc_obj_Box {"));
    assert!(c.contains("int64_t value;"));
    assert!(c.contains("static phc_obj_Box* phc_construct_Box(int64_t phc_var_value)"));
    assert!(c.contains("static int64_t phc_method_Box_get(phc_obj_Box* phc_var_this)"));
    assert!(c.contains("(phc_var_this)->value"));
}

#[test]
fn class_construction_call_emits_phc_construct() {
    let src = r#"pack a;
        public class Box {
            construct(public int $value) {}
        }
        function main(): void {
            Box $b = Box(42);
        }"#;
    let c = emit_for(src);
    assert!(c.contains("phc_obj_Box* phc_var_b = phc_construct_Box((int64_t)42);"));
}

#[test]
fn member_assign_via_equals_emits_field_write() {
    let src = r#"pack a;
        public class Box {
            flip int $value = 0;
        }
        function main(): void {
            Box $b = Box();
            $b->value = 99;
        }"#;
    let c = emit_for(src);
    assert!(c.contains("(phc_var_b)->value = (int64_t)99;"));
}

#[test]
fn trait_use_emits_method_under_class_name() {
    let src = r#"pack a;
        public trait Doubled {
            function doubled(): int { return $this->value + $this->value; }
        }
        public class Box {
            use Doubled;
            construct(public int $value) {}
        }
        function main(): void {}"#;
    let c = emit_for(src);
    assert!(c.contains("static int64_t phc_method_Box_doubled(phc_obj_Box* phc_var_this)"));
}

#[test]
fn trait_mixin_dispatches_this_method_on_host_class() {
    // The trait body calls a method on `$this`. The trait's own
    // typecheck record can't see the host class, so without the
    // emitter's `current_class` tracking this falls to
    // "codegen TODO method receiver".
    let src = r#"pack a;
        public trait Loggable {
            function describe(): string { return $this->name(); }
        }
        public class User {
            use Loggable;
            construct(public string $value) {}
            public function name(): string { return $this->value; }
        }
        function main(): void {}"#;
    let c = emit_for(src);
    assert!(
        c.contains("phc_method_User_describe"),
        "trait method should be emitted under host class"
    );
    assert!(
        c.contains("phc_method_User_name(phc_var_this)"),
        "method call on $this in trait body should dispatch to host class:\n{c}"
    );
    assert!(
        !c.contains("codegen TODO method receiver"),
        "trait body should not fall through to TODO panic"
    );
}

#[test]
fn string_toint_with_try_emits_result_propagation() {
    // `$raw->toInt()` returns result<int, parseError>; the trailing
    // `?` must early-return on the err variant and unwrap the int
    // on ok. Regressions here previously fell to either
    // "codegen TODO method receiver" or "codegen TODO ? on unknown type".
    let src = r#"pack a;
        public function parsePort(string $raw): result<int, parseError> {
            int $value = $raw->toInt()?;
            return result::ok($value);
        }
        function main(): void {}"#;
    let c = emit_for(src);
    assert!(
        c.contains("phc_int_parse(phc_var_raw)"),
        "toInt should route to phc_int_parse:\n{c}"
    );
    assert!(
        c.contains("phc_result __phc_try_") && c.contains(".ok.i64;"),
        "postfix `?` should emit result-propagation statement-expression:\n{c}"
    );
    assert!(!c.contains("codegen TODO"), "no TODO panics:\n{c}");
}

#[test]
fn d047_string_extras_emit_correct_c_calls() {
    let src = r#"pack a;
        public function demo(string $s): void {
            list<string> $parts = $s->split(",");
            string $rep = $s->repeat(3);
            option<int> $idx = $s->indexOf("x");
            string $replaced = $s->replace("a", "b");
        }
        function main(): void {}"#;
    let c = emit_for(src);
    assert!(
        c.contains("phc_str_split(phc_var_s,"),
        "split should call phc_str_split:\n{c}"
    );
    assert!(
        c.contains("phc_str_repeat(phc_var_s,"),
        "repeat should call phc_str_repeat:\n{c}"
    );
    assert!(
        c.contains("phc_str_index_of(phc_var_s,"),
        "indexOf should call phc_str_index_of:\n{c}"
    );
    assert!(
        c.contains("phc_str_replace(phc_var_s,"),
        "replace should call phc_str_replace:\n{c}"
    );
    assert!(!c.contains("codegen TODO"), "no TODO panics:\n{c}");
}

#[test]
fn generic_free_fn_monomorphizes_per_concrete_arg_tuple() {
    // `max<T>` is generic; only one call site (`max(3, 7)`) so a
    // single int64-specialised instance should emerge. The
    // unsubstituted `phc_max` symbol must NOT appear in the output —
    // the C compiler would reject its `phc_value` placeholder params.
    let src = r#"pack a;
        public function max<T>(T $a, T $b): T {
            if ($a > $b) { return $a; }
            return $b;
        }
        public function demo(): int {
            return max(3, 7);
        }
        function main(): void {}"#;
    let c = emit_for(src);
    assert!(
        c.contains("static int64_t phc_max__int(int64_t phc_var_a, int64_t phc_var_b)"),
        "int64 instance signature missing:\n{c}"
    );
    assert!(
        c.contains("phc_max__int((int64_t)3, (int64_t)7)"),
        "call site should dispatch to the mono instance:\n{c}"
    );
    assert!(
        !c.contains("phc_value phc_max"),
        "unsubstituted generic must not be emitted:\n{c}"
    );
}

#[test]
fn generic_free_fn_nullable_primitive_arg_emits_valid_ctype() {
    // Regression: ty_to_typeref must handle NullablePrimitive so a
    // generic called with `int?` maps to int64_t, not `phc_value`.
    let src = r#"pack a;
        public function wrap<T>(T $val): T { return $val; }
        public function demo(int? $x): int? { return wrap($x); }
        function main(): void {}"#;
    let c = emit_for(src);
    assert!(
        c.contains("phc_wrap__int_opt"),
        "int_opt mono instance should be emitted:\n{c}"
    );
    assert!(
        !c.contains("phc_value phc_wrap__int_opt"),
        "nullable int arg must not produce phc_value param:\n{c}"
    );
}

#[test]
fn bytes_primitive_lowers_to_phc_bytes() {
    let src = r#"pack a;
        public function take(bytes $b): bytes { return $b; }
        function main(): void {}"#;
    let c = emit_for(src);
    assert!(
        c.contains("phc_bytes phc_take(phc_bytes phc_var_b)"),
        "bytes should map to phc_bytes, not phc_value:\n{c}"
    );
}

#[test]
fn async_function_emits_runtime_panic_stub() {
    // D-003 leaves async semantics for a future slice. Compiled
    // mode must not silently produce broken C — it should emit a
    // signature + a panic stub that fails loud at runtime.
    let src = r#"pack a;
        async function fetch(string $url): result<int, parseError> {
            return result::ok(1);
        }
        function main(): void {}"#;
    let c = emit_for(src);
    assert!(
        c.contains("static phc_result phc_fetch(phc_string phc_var_url)"),
        "async signature should still emit:\n{c}"
    );
    assert!(
        c.contains("async function `fetch` not yet implemented"),
        "async body should be a panic stub:\n{c}"
    );
    assert!(
        c.contains("return (phc_result){0};"),
        "unreachable trailing return should be the zero-init form:\n{c}"
    );
}

#[test]
fn generic_free_fn_specialises_at_float_and_int_in_one_unit() {
    let src = r#"pack a;
        public function pick<T>(T $a, T $b): T {
            if ($a > $b) { return $a; }
            return $b;
        }
        public function demo(): void {
            int $i = pick(3, 7);
            float $f = pick(1.5, 2.5);
        }
        function main(): void {}"#;
    let c = emit_for(src);
    assert!(
        c.contains("phc_pick__int(int64_t"),
        "int specialisation missing:\n{c}"
    );
    assert!(
        c.contains("phc_pick__float(double"),
        "float specialisation missing:\n{c}"
    );
    // Each specialisation appears twice — once as a forward decl
    // and once as a body. The test mostly cares that distinct
    // C symbols exist for the two type tuples; checking each
    // appears ≥ 2 times catches a missing-emit regression.
    assert!(
        c.matches("static double phc_pick__float").count() >= 2,
        "float specialisation should have both forward decl and body:\n{c}"
    );
    assert!(
        c.matches("static int64_t phc_pick__int").count() >= 2,
        "int specialisation should have both forward decl and body:\n{c}"
    );
}

#[test]
fn generic_free_fn_with_two_type_params_concatenates_mangling() {
    let src = r#"pack a;
        public function pair<A, B>(A $a, B $b): A { return $a; }
        public function demo(): int {
            return pair(7, true);
        }
        function main(): void {}"#;
    let c = emit_for(src);
    assert!(
        c.contains("phc_pair__int_bool"),
        "two-param mangle should join with `_`:\n{c}"
    );
}

#[test]
fn generic_free_fn_with_no_call_sites_emits_nothing() {
    // Generic fn declared but never called — emitter should skip
    // both the unsubstituted source and any specialisation.
    let src = r#"pack a;
        public function noop<T>(T $a): T { return $a; }
        function main(): void {}"#;
    let c = emit_for(src);
    assert!(
        !c.contains("phc_noop"),
        "uncalled generic must not emit:\n{c}"
    );
}

#[test]
fn generic_free_fn_specialises_at_string_type() {
    let src = r#"pack a;
        public function identity<T>(T $a): T { return $a; }
        public function demo(): string {
            return identity("hello");
        }
        function main(): void {}"#;
    let c = emit_for(src);
    assert!(
        c.contains("phc_identity__string(phc_string"),
        "string specialisation missing:\n{c}"
    );
}

#[test]
fn forward_decls_let_main_call_helper_declared_later() {
    let src = r#"pack a;
        function main(): void { helper(); }
        function helper(): void {}"#;
    let c = emit_for(src);
    // Both forward decls present before any body.
    let main_decl_pos = c.find("static void phc_main(void);").unwrap();
    let helper_decl_pos = c.find("static void phc_helper(void);").unwrap();
    let first_body_pos = c.find("static void phc_main(void) {").unwrap();
    assert!(main_decl_pos < first_body_pos);
    assert!(helper_decl_pos < first_body_pos);
}
