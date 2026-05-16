/* SPDX-License-Identifier: MIT
 * PHC runtime header. C11. Linked into every binary `phc build`
 * produces while the LLVM-based runtime is built out in parallel.
 */
#ifndef PHC_RUNTIME_H
#define PHC_RUNTIME_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Owned, length-prefixed UTF-8 string. `data` is a malloc'd buffer
 * with a NUL byte at `data[len]` so it can also be passed to plain
 * C string APIs. Size 0 strings have data != NULL pointing at a
 * single NUL byte. */
typedef struct {
    size_t len;
    char* data;
} phc_string;

/* Opaque value cell. v0 only uses it for `null` / `void`. */
typedef struct {
    int kind; /* 0 = null, 1 = void */
} phc_value;

/* Generic payload union shared by phc_result and phc_option. The
 * codegen emits the right .<member> access at each call site
 * based on the static T it knows. */
typedef union {
    int64_t i64;
    double  f64;
    int     b; /* C `bool` accessed via `int` to avoid alignment surprises */
    phc_string s;
    void*   ptr; /* instances, lambdas, anything boxed */
} phc_payload;

/* Tagged success/failure cell for `result<T, E>`. kind == 0 → ok. */
typedef struct {
    int kind;
    phc_payload ok;
    phc_payload err;
} phc_result;

/* Tagged optional cell for `option<T>`. kind == 0 → some. */
typedef struct {
    int kind;
    phc_payload some;
} phc_option;

/* === Result / Option ergonomic methods (D-026) ===
 * Predicates only — `unwrapOr` is dispatched at the call site by
 * picking the right phc_payload union member from the static T,
 * so it does not need a runtime helper. */
bool phc_result_is_ok(phc_result r);
bool phc_result_is_err(phc_result r);
bool phc_option_is_some(phc_option o);
bool phc_option_is_none(phc_option o);

/* List value (D-027). Heap-allocated; the C-level type is an opaque
 * pointer so passing a `phc_list` around is a handle copy, not a
 * deep copy. Two handles to the same list see each other's
 * mutations — explicit reference semantics for v0; CoW lands when
 * the runtime grows refcounts.
 *
 * Memory note: `push` of a `phc_string` shallow-copies the struct
 * (the data pointer is shared with the caller). Safe today because
 * v0 never frees; revisit once the runtime owns lifetimes. */
struct phc_list_s;
typedef struct phc_list_s* phc_list;

phc_list    phc_list_new(void);
void        phc_list_push(phc_list l, phc_payload v);
phc_payload phc_list_at(phc_list l, int64_t i);  /* aborts on OOB */
int64_t     phc_list_len(phc_list l);

/* Map value (D-028). String-keyed only in v0a; linear-scan
 * storage (Vec<(key, payload)>). Heap-allocated handle, same
 * reference-semantics carve-out as list<T>: mutation through
 * `set` updates the shared backing storage. Hash-based storage
 * lands when generic-key hashing is speced. */
struct phc_map_s;
typedef struct phc_map_s* phc_map;

phc_map     phc_map_new(void);
void        phc_map_set(phc_map m, phc_string key, phc_payload v);
phc_option  phc_map_get(phc_map m, phc_string key);
bool        phc_map_has(phc_map m, phc_string key);
int64_t     phc_map_len(phc_map m);

/* Set value (D-031). String-only in v0a, same reference-semantics
 * + linear-scan storage as `phc_map`. Distinct C type so codegen
 * cannot accidentally cross-cast a `phc_map` and a `phc_set`. */
struct phc_set_s;
typedef struct phc_set_s* phc_set;

phc_set     phc_set_new(void);
bool        phc_set_add(phc_set s, phc_string key);  /* true on insert, false if already present */
bool        phc_set_has(phc_set s, phc_string key);
bool        phc_set_remove(phc_set s, phc_string key); /* true if removed */
int64_t     phc_set_len(phc_set s);

/* Lambda value: function pointer + heap-alloc'd capture environment.
 * The codegen emits `fn` as the lifted-body symbol's address and
 * `env` as a malloc'd struct holding every variable the body
 * captures (NULL when the body captures nothing). Call sites cast
 * `fn` to the precise `<ret>(*)(void*, args...)` shape they expect;
 * the env is dispatched on inside the lifted body via a cast back
 * to its specific struct type. C5a only emits inline-invoked
 * lambdas — storing or passing one as a value is blocked on the
 * function-type syntax decision (D-024). */
typedef struct {
    void* fn;
    void* env;
} phc_lambda;

/* === Constructors === */
phc_string phc_string_lit(const char* s);
phc_string phc_string_owned(const char* s, size_t len);
phc_value  phc_null(void);
phc_value  phc_void(void);

/* === IO === */
void phc_print(phc_string s);
void phc_panic(const char* msg);

/* D-032 — io stdlib namespace. `phc_print` (above) is kept as the
 * Logger::info-backing helper that writes to stdout with a
 * trailing newline; the io::* surface adds the no-newline and
 * stderr variants, plus a readLine that returns a `phc_option`
 * carrying the line on success and kind=1 on EOF. */
void phc_io_print(phc_string s);     /* stdout, no trailing newline */
void phc_io_println(phc_string s);   /* stdout, trailing newline */
void phc_io_eprint(phc_string s);    /* stderr, no trailing newline */
void phc_io_eprintln(phc_string s);  /* stderr, trailing newline */
phc_option phc_io_read_line(void);   /* phc_option<string>; some=line (no trailing \n), kind=1 on EOF */

/* === Allocation === */
/* Allocate `size` bytes zeroed. Used by class instance constructors
 * emitted by phc-codegen. Aborts the process on out-of-memory. */
void* phc_alloc(size_t size);

/* === String ops === */
phc_string phc_concat2(phc_string a, phc_string b);

/* === String stdlib (D-025) ===
 * v0 surface: byte-oriented, ASCII-correct case fold + trim.
 * Multi-byte UTF-8 awareness lands when the runtime grows real
 * Unicode tables (deferred). Every method that returns a `phc_string`
 * allocates a fresh buffer; the caller owns it. */
bool       phc_str_eq(phc_string a, phc_string b);
int64_t    phc_str_len(phc_string s);
bool       phc_str_contains(phc_string s, phc_string needle);
bool       phc_str_starts_with(phc_string s, phc_string prefix);
bool       phc_str_ends_with(phc_string s, phc_string suffix);
phc_string phc_str_trim(phc_string s);
phc_string phc_str_upper(phc_string s);
phc_string phc_str_lower(phc_string s);

/* === Numeric stdlib (D-034) === */
phc_result phc_int_parse(phc_string s);     /* result<int, parseError> */
int64_t    phc_int_min(int64_t a, int64_t b);
int64_t    phc_int_max(int64_t a, int64_t b);
int64_t    phc_int_abs(int64_t v);

phc_result phc_float_parse(phc_string s);   /* result<float, parseError> */
double     phc_float_min(double a, double b);
double     phc_float_max(double a, double b);
double     phc_float_abs(double v);
bool       phc_float_is_nan(double v);

/* === Conversion to phc_string === */
phc_string phc_to_string_int64(int64_t v);
phc_string phc_to_string_double(double v);
phc_string phc_to_string_bool(bool v);
phc_string phc_to_string_id(phc_string s);

#define phc_to_string(x) _Generic((x),                     \
    int64_t:    phc_to_string_int64,                       \
    double:     phc_to_string_double,                      \
    bool:       phc_to_string_bool,                        \
    phc_string: phc_to_string_id                           \
)(x)

#ifdef __cplusplus
}
#endif

#endif /* PHC_RUNTIME_H */
