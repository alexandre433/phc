package main

// Count from stdin so the loop can't be constant-folded; `inc` is
// //go:noinline so each iteration is a genuine call, matching PHC's
// @noinline (D-052) and the C/Rust noinline builds.
import (
	"bufio"
	"fmt"
	"os"
	"strconv"
	"strings"
)

type Counter struct{ value int64 }

//go:noinline
func (c *Counter) inc() { c.value++ }

func (c *Counter) get() int64 { return c.value }

func main() {
	reader := bufio.NewReader(os.Stdin)
	line, _ := reader.ReadString('\n')
	count, _ := strconv.ParseInt(strings.TrimSpace(line), 10, 64)
	c := &Counter{value: 0}
	var i int64 = 0
	for i < count {
		c.inc()
		i++
	}
	fmt.Printf("counter = %d\n", c.get())
}
