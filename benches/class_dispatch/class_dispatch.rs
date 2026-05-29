// Count from stdin so the loop can't be constant-folded; `inc` is
// `#[inline(never)]` so each iteration is a genuine call, matching
// PHC's @noinline (D-052) and the C noinline build.
use std::io::Read;

struct Counter {
    value: i64,
}

impl Counter {
    fn new(start: i64) -> Counter {
        Counter { value: start }
    }
    #[inline(never)]
    fn inc(&mut self) {
        self.value += 1;
    }
    fn get(&self) -> i64 {
        self.value
    }
}

fn main() {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).unwrap();
    let count: i64 = input.trim().parse().unwrap_or(0);
    let mut c = Counter::new(0);
    let mut i: i64 = 0;
    while i < count {
        c.inc();
        i += 1;
    }
    println!("counter = {}", c.get());
}
