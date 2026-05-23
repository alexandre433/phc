/* Naive doubly-recursive fib(35). Measures function-call overhead
 * more than arithmetic. Reference handwritten C used as the lower
 * bound the PHC version is benchmarked against. */
#include <stdio.h>

long long fib(long long n) {
    if (n < 2) return n;
    return fib(n - 1) + fib(n - 2);
}

int main(void) {
    printf("fib(35) = %lld\n", fib(35));
    return 0;
}
