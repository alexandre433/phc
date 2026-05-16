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

/* ===== String stdlib (D-025) ===== */

bool phc_str_eq(phc_string a, phc_string b) {
    if (a.len != b.len) return false;
    if (a.len == 0) return true;
    return memcmp(a.data, b.data, a.len) == 0;
}

int64_t phc_str_len(phc_string s) {
    return (int64_t)s.len;
}

bool phc_str_contains(phc_string s, phc_string needle) {
    if (needle.len == 0) return true;
    if (needle.len > s.len) return false;
    for (size_t i = 0; i + needle.len <= s.len; ++i) {
        if (memcmp(s.data + i, needle.data, needle.len) == 0) return true;
    }
    return false;
}

bool phc_str_starts_with(phc_string s, phc_string prefix) {
    if (prefix.len > s.len) return false;
    return memcmp(s.data, prefix.data, prefix.len) == 0;
}

bool phc_str_ends_with(phc_string s, phc_string suffix) {
    if (suffix.len > s.len) return false;
    return memcmp(s.data + (s.len - suffix.len), suffix.data, suffix.len) == 0;
}

static int phc_is_ws(unsigned char c) {
    return c == ' ' || c == '\t' || c == '\n' || c == '\r' || c == '\v' || c == '\f';
}

phc_string phc_str_trim(phc_string s) {
    size_t lo = 0;
    while (lo < s.len && phc_is_ws((unsigned char)s.data[lo])) ++lo;
    size_t hi = s.len;
    while (hi > lo && phc_is_ws((unsigned char)s.data[hi - 1])) --hi;
    return phc_string_owned(s.data + lo, hi - lo);
}

phc_string phc_str_upper(phc_string s) {
    phc_string out = phc_string_owned(s.data, s.len);
    for (size_t i = 0; i < out.len; ++i) {
        unsigned char c = (unsigned char)out.data[i];
        if (c >= 'a' && c <= 'z') out.data[i] = (char)(c - 32);
    }
    return out;
}

phc_string phc_str_lower(phc_string s) {
    phc_string out = phc_string_owned(s.data, s.len);
    for (size_t i = 0; i < out.len; ++i) {
        unsigned char c = (unsigned char)out.data[i];
        if (c >= 'A' && c <= 'Z') out.data[i] = (char)(c + 32);
    }
    return out;
}

/* ===== List stdlib (D-027) ===== */

struct phc_list_s {
    size_t len;
    size_t cap;
    phc_payload* items;
};

phc_list phc_list_new(void) {
    phc_list l = (phc_list)phc_alloc(sizeof(struct phc_list_s));
    l->len = 0;
    l->cap = 0;
    l->items = NULL;
    return l;
}

void phc_list_push(phc_list l, phc_payload v) {
    if (l->len == l->cap) {
        size_t new_cap = l->cap == 0 ? 4 : l->cap * 2;
        phc_payload* grown = (phc_payload*)realloc(l->items, new_cap * sizeof(phc_payload));
        if (!grown) {
            fputs("phc runtime: out of memory growing list\n", stderr);
            abort();
        }
        l->items = grown;
        l->cap = new_cap;
    }
    l->items[l->len++] = v;
}

phc_payload phc_list_at(phc_list l, int64_t i) {
    if (i < 0 || (size_t)i >= l->len) {
        phc_panic("list index out of bounds");
    }
    return l->items[(size_t)i];
}

int64_t phc_list_len(phc_list l) {
    return (int64_t)l->len;
}
