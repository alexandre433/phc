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

void phc_io_print(phc_string s) {
    if (s.len > 0) fwrite(s.data, 1, s.len, stdout);
    fflush(stdout);
}

void phc_io_println(phc_string s) {
    if (s.len > 0) fwrite(s.data, 1, s.len, stdout);
    fputc('\n', stdout);
    fflush(stdout);
}

void phc_io_eprint(phc_string s) {
    if (s.len > 0) fwrite(s.data, 1, s.len, stderr);
    fflush(stderr);
}

void phc_io_eprintln(phc_string s) {
    if (s.len > 0) fwrite(s.data, 1, s.len, stderr);
    fputc('\n', stderr);
    fflush(stderr);
}

phc_option phc_io_read_line(void) {
    phc_option out;
    size_t cap = 64;
    size_t len = 0;
    char* buf = (char*)malloc(cap);
    if (!buf) {
        fputs("phc runtime: out of memory reading stdin\n", stderr);
        abort();
    }
    int ch;
    while ((ch = fgetc(stdin)) != EOF) {
        if (ch == '\n') break;
        if (len + 1 >= cap) {
            cap *= 2;
            char* grown = (char*)realloc(buf, cap);
            if (!grown) {
                free(buf);
                fputs("phc runtime: out of memory growing readLine buffer\n", stderr);
                abort();
            }
            buf = grown;
        }
        buf[len++] = (char)ch;
    }
    if (ch == EOF && len == 0) {
        free(buf);
        out.kind = 1;
        out.some.i64 = 0;
        return out;
    }
    /* Trim trailing CR from CRLF on Windows. */
    if (len > 0 && buf[len - 1] == '\r') len--;
    buf[len] = '\0';
    phc_string s;
    s.len = len;
    s.data = buf;
    out.kind = 0;
    out.some.s = s;
    return out;
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

/* ===== Numeric stdlib (D-034) ===== */

#include <ctype.h>
#include <errno.h>
#include <limits.h>
#include <math.h>

static phc_result phc_parse_err(const char* msg) {
    phc_result r;
    r.kind = 1;
    r.err.s = phc_string_lit(msg);
    return r;
}

phc_result phc_int_parse(phc_string s) {
    if (s.len == 0) return phc_parse_err("empty input");
    /* Build a NUL-terminated copy and use strtoll. */
    char* tmp = phc_xmalloc(s.len + 1);
    memcpy(tmp, s.data, s.len);
    tmp[s.len] = '\0';
    /* Reject embedded NULs early so strtoll doesn't stop short. */
    if (strlen(tmp) != s.len) {
        free(tmp);
        return phc_parse_err("embedded NUL in input");
    }
    char* end = NULL;
    errno = 0;
    long long parsed = strtoll(tmp, &end, 10);
    if (end == tmp || *end != '\0') {
        free(tmp);
        return phc_parse_err("invalid integer literal");
    }
    if (errno == ERANGE) {
        free(tmp);
        return phc_parse_err("integer out of range");
    }
    free(tmp);
    phc_result r;
    r.kind = 0;
    r.ok.i64 = (int64_t)parsed;
    return r;
}

int64_t phc_int_min(int64_t a, int64_t b) { return a < b ? a : b; }
int64_t phc_int_max(int64_t a, int64_t b) { return a > b ? a : b; }
int64_t phc_int_abs(int64_t v) {
    /* Avoid UB on INT64_MIN: saturate to INT64_MAX. */
    if (v == INT64_MIN) return INT64_MAX;
    return v < 0 ? -v : v;
}

phc_result phc_float_parse(phc_string s) {
    if (s.len == 0) return phc_parse_err("empty input");
    char* tmp = phc_xmalloc(s.len + 1);
    memcpy(tmp, s.data, s.len);
    tmp[s.len] = '\0';
    if (strlen(tmp) != s.len) {
        free(tmp);
        return phc_parse_err("embedded NUL in input");
    }
    char* end = NULL;
    errno = 0;
    double parsed = strtod(tmp, &end);
    if (end == tmp || *end != '\0') {
        free(tmp);
        return phc_parse_err("invalid float literal");
    }
    if (errno == ERANGE) {
        free(tmp);
        return phc_parse_err("float out of range");
    }
    free(tmp);
    phc_result r;
    r.kind = 0;
    r.ok.f64 = parsed;
    return r;
}

double phc_float_min(double a, double b) { return a < b ? a : b; }
double phc_float_max(double a, double b) { return a > b ? a : b; }
double phc_float_abs(double v) { return v < 0.0 ? -v : v; }
bool   phc_float_is_nan(double v) { return v != v; }

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

/* ===== Result / Option ergonomic methods (D-026) ===== */

bool phc_result_is_ok(phc_result r) { return r.kind == 0; }
bool phc_result_is_err(phc_result r) { return r.kind != 0; }
bool phc_option_is_some(phc_option o) { return o.kind == 0; }
bool phc_option_is_none(phc_option o) { return o.kind != 0; }

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

/* ===== Map stdlib (D-028) ===== */

struct phc_map_entry {
    phc_string key;
    phc_payload value;
};

struct phc_map_s {
    size_t len;
    size_t cap;
    struct phc_map_entry* items;
};

phc_map phc_map_new(void) {
    phc_map m = (phc_map)phc_alloc(sizeof(struct phc_map_s));
    m->len = 0;
    m->cap = 0;
    m->items = NULL;
    return m;
}

static int phc_string_eq(phc_string a, phc_string b) {
    if (a.len != b.len) return 0;
    if (a.len == 0) return 1;
    return memcmp(a.data, b.data, a.len) == 0;
}

void phc_map_set(phc_map m, phc_string key, phc_payload v) {
    for (size_t i = 0; i < m->len; ++i) {
        if (phc_string_eq(m->items[i].key, key)) {
            m->items[i].value = v;
            return;
        }
    }
    if (m->len == m->cap) {
        size_t new_cap = m->cap == 0 ? 4 : m->cap * 2;
        struct phc_map_entry* grown =
            (struct phc_map_entry*)realloc(m->items, new_cap * sizeof(struct phc_map_entry));
        if (!grown) {
            fputs("phc runtime: out of memory growing map\n", stderr);
            abort();
        }
        m->items = grown;
        m->cap = new_cap;
    }
    m->items[m->len].key = key;
    m->items[m->len].value = v;
    m->len++;
}

phc_option phc_map_get(phc_map m, phc_string key) {
    for (size_t i = 0; i < m->len; ++i) {
        if (phc_string_eq(m->items[i].key, key)) {
            phc_option o;
            o.kind = 0;
            o.some = m->items[i].value;
            return o;
        }
    }
    phc_option o;
    o.kind = 1;
    o.some.i64 = 0;
    return o;
}

bool phc_map_has(phc_map m, phc_string key) {
    for (size_t i = 0; i < m->len; ++i) {
        if (phc_string_eq(m->items[i].key, key)) return true;
    }
    return false;
}

int64_t phc_map_len(phc_map m) {
    return (int64_t)m->len;
}

/* ===== Set stdlib (D-031) ===== */

struct phc_set_s {
    size_t len;
    size_t cap;
    phc_string* items;
};

phc_set phc_set_new(void) {
    phc_set s = (phc_set)phc_alloc(sizeof(struct phc_set_s));
    s->len = 0;
    s->cap = 0;
    s->items = NULL;
    return s;
}

bool phc_set_add(phc_set s, phc_string key) {
    for (size_t i = 0; i < s->len; ++i) {
        if (phc_string_eq(s->items[i], key)) return false;
    }
    if (s->len == s->cap) {
        size_t new_cap = s->cap == 0 ? 4 : s->cap * 2;
        phc_string* grown =
            (phc_string*)realloc(s->items, new_cap * sizeof(phc_string));
        if (!grown) {
            fputs("phc runtime: out of memory growing set\n", stderr);
            abort();
        }
        s->items = grown;
        s->cap = new_cap;
    }
    s->items[s->len++] = key;
    return true;
}

bool phc_set_has(phc_set s, phc_string key) {
    for (size_t i = 0; i < s->len; ++i) {
        if (phc_string_eq(s->items[i], key)) return true;
    }
    return false;
}

bool phc_set_remove(phc_set s, phc_string key) {
    for (size_t i = 0; i < s->len; ++i) {
        if (phc_string_eq(s->items[i], key)) {
            /* Compact by swapping the last entry into the hole. */
            s->items[i] = s->items[s->len - 1];
            s->len--;
            return true;
        }
    }
    return false;
}

int64_t phc_set_len(phc_set s) {
    return (int64_t)s->len;
}
