fn ex1() -> usize {
    1
}
fn ex2(x: usize) -> usize {
    x + 1
}

fn chooser(b: bool) -> impl Fn() -> usize {
    if b {
        ex1
    } else {
        || ex2(4)
    }
}

fn return_closure() -> impl Fn(i32) -> i32 {
    let x = 5;
    move |y| x + y
}

#[test]
fn test_closure_types() {
    println!("Closure examples");

    // Similar problem. What's the type of f1 and f2?
    // let _: fn(bool) -> fn() -> usize = chooser;
    let f1 = chooser(true);
    let f2 = chooser(false);
    assert_eq!(f1(), 1);
    assert_eq!(f2(), 5);

    // impl syntax not allowed in type annotations
    // but it's not a function pointer. What's the actual type?
    // let _: fn() -> impl Fn(i32) -> i32 = return_closure;
    // let _: impl Fn(i32) -> i32 = return_closure();
    let f = return_closure;
    assert_eq!(f()(10), 15);
}
