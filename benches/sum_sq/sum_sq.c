/* Sum of i*i for i in [0, 100_000_000). Plain int64 loop with one
 * imul + two adds per iteration. Reference handwritten C used as
 * the lower bound the PHC version is benchmarked against. */
#include <stdio.h>

int main(void) {
    long long sum = 0;
    for (long long i = 0; i < 100000000LL; i++) sum += i * i;
    printf("sum: %lld\n", sum);
    return 0;
}
