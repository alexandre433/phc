#include <stdio.h>
#include <stdint.h>

typedef struct { int64_t value; } Counter;

static inline void counter_inc(Counter* c) { c->value++; }
static inline int64_t counter_get(Counter* c) { return c->value; }

int main(void) {
    Counter c = {0};
    for (int64_t i = 0; i < 10000000; i++) counter_inc(&c);
    puts("done");
    return 0;
}
