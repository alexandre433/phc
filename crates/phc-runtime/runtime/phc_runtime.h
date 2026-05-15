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

/* === Constructors === */
phc_string phc_string_lit(const char* s);
phc_string phc_string_owned(const char* s, size_t len);
phc_value  phc_null(void);
phc_value  phc_void(void);

/* === IO === */
void phc_print(phc_string s);
void phc_panic(const char* msg);

/* === Allocation === */
/* Allocate `size` bytes zeroed. Used by class instance constructors
 * emitted by phc-codegen. Aborts the process on out-of-memory. */
void* phc_alloc(size_t size);

/* === String ops === */
phc_string phc_concat2(phc_string a, phc_string b);

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
