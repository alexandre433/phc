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
