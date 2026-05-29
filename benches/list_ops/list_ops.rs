// 100K-element list: build descending, sort, fold-sum. The sum is
// printed so the fold isn't dead-code-eliminated.
fn main() {
    let n: i64 = 100_000;
    let mut xs: Vec<i64> = (0..n).map(|i| n - i).collect();
    xs.sort();
    let sum: i64 = xs.iter().sum();
    println!("sum = {}", sum);
}
