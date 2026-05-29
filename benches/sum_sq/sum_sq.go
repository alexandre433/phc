package main

import "fmt"

func main() {
	var sum int64 = 0
	for i := int64(0); i < 100000000; i++ {
		sum += i * i
	}
	fmt.Printf("sum: %d\n", sum)
}
