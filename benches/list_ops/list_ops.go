package main

import (
	"fmt"
	"sort"
)

func main() {
	const n = 100000
	xs := make([]int64, n)
	for i := int64(0); i < n; i++ {
		xs[i] = n - i
	}
	sort.Slice(xs, func(a, b int) bool { return xs[a] < xs[b] })
	var sum int64 = 0
	for _, x := range xs {
		sum += x
	}
	fmt.Printf("sum = %d\n", sum)
}
