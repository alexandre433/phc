/* Real method-dispatch throughput. Count comes from stdin so -O2
 * can't constant-fold the loop, and `counter_inc` is noinline so gcc
 * emits a genuine call per iteration (matching PHC's @noinline,
 * D-052). The result is printed so the loop is observed. */
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>

typedef struct { int64_t value; } Counter;

__attribute__((noinline)) static void counter_inc(Counter* c) { c->value++; }
static int64_t counter_get(Counter* c) { return c->value; }

int main(void) {
    char buf[64];
    if (!fgets(buf, sizeof buf, stdin)) return 1;
    int64_t count = atoll(buf);
    Counter c = {0};
    for (int64_t i = 0; i < count; i++) counter_inc(&c);
    printf("counter = %lld\n", (long long)counter_get(&c));
    return 0;
}
