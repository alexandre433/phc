// Wrapping arithmetic matches the C/PHC i64 overflow (sum of squares
// to 1e8 overflows i64 the same way in all three).
fn main() {
    let mut sum: i64 = 0;
    let mut i: i64 = 0;
    while i < 100_000_000 {
        sum = sum.wrapping_add(i.wrapping_mul(i));
        i += 1;
    }
    println!("sum: {}", sum);
}
