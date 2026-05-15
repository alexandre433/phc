/* SPDX-License-Identifier: MIT */
#include "phc_runtime.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static char* phc_xmalloc(size_t n) {
    char* p = (char*)malloc(n);
    if (!p) {
        fputs("phc runtime: out of memory\n", stderr);
        abort();
    }
    return p;
}

void* phc_alloc(size_t size) {
    void* p = calloc(1, size ? size : 1);
    if (!p) {
        fputs("phc runtime: out of memory\n", stderr);
        abort();
    }
    return p;
}

phc_string phc_string_owned(const char* s, size_t len) {
    char* buf = phc_xmalloc(len + 1);
    if (len > 0) memcpy(buf, s, len);
    buf[len] = '\0';
    phc_string out;
    out.len = len;
    out.data = buf;
    return out;
}

phc_string phc_string_lit(const char* s) {
    return phc_string_owned(s, strlen(s));
}

phc_value phc_null(void) {
    phc_value v;
    v.kind = 0;
    return v;
}

phc_value phc_void(void) {
    phc_value v;
    v.kind = 1;
    return v;
}

void phc_print(phc_string s) {
    /* fwrite handles embedded NULs correctly even though our
     * strings are NUL-terminated. */
    if (s.len > 0) fwrite(s.data, 1, s.len, stdout);
    fputc('\n', stdout);
    fflush(stdout);
}

void phc_panic(const char* msg) {
    fputs("phc panic: ", stderr);
    fputs(msg ? msg : "(unknown)", stderr);
    fputc('\n', stderr);
    fflush(stderr);
    abort();
}

phc_string phc_concat2(phc_string a, phc_string b) {
    size_t total = a.len + b.len;
    char* buf = phc_xmalloc(total + 1);
    if (a.len) memcpy(buf, a.data, a.len);
    if (b.len) memcpy(buf + a.len, b.data, b.len);
    buf[total] = '\0';
    phc_string out;
    out.len = total;
    out.data = buf;
    return out;
}

phc_string phc_to_string_int64(int64_t v) {
    char tmp[32];
    int n = snprintf(tmp, sizeof(tmp), "%lld", (long long)v);
    if (n < 0) n = 0;
    return phc_string_owned(tmp, (size_t)n);
}

phc_string phc_to_string_double(double v) {
    char tmp[64];
    int n = snprintf(tmp, sizeof(tmp), "%g", v);
    if (n < 0) n = 0;
    return phc_string_owned(tmp, (size_t)n);
}

phc_string phc_to_string_bool(bool v) {
    return phc_string_lit(v ? "true" : "false");
}

phc_string phc_to_string_id(phc_string s) {
    /* Pass-through; ownership is shallow-copied. The interpreter
     * concat path always allocates fresh, so this is safe to share
     * for the hello-world workload. A reference-counted strategy
     * lands when the runtime grows real CoW. */
    return s;
}
