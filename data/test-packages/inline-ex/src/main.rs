
#[inline(always)]
fn inlined() {
    println!("Hello from inlined function!");
}

#[inline(never)]
fn not_inlined() {
    println!("Hello from not-inlined function!");
}

fn main() {
    inlined();
    not_inlined();
}
