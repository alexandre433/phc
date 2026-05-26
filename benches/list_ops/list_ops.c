#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

static int cmp_int64(const void* a, const void* b) {
    int64_t x = *(const int64_t*)a;
    int64_t y = *(const int64_t*)b;
    return (x > y) - (x < y);
}

int main(void) {
    const int64_t N = 100000;
    int64_t* xs = malloc(N * sizeof(int64_t));
    for (int64_t i = 0; i < N; i++) xs[i] = N - i;
    qsort(xs, N, sizeof(int64_t), cmp_int64);
    int64_t sum = 0;
    for (int64_t i = 0; i < N; i++) sum += xs[i];
    free(xs);
    puts("done");
    return 0;
}
