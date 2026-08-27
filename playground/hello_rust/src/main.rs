fn main() {
    println!("Hello, world!");
    println!("2 + 3 = {}", add(2, 3));
}

fn add(a: i32, b: i32) -> i32 {
    a + b
}