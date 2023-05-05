unsafe fn foo() {
    let x: isize = 3;
    let y: *const isize = &x;
    unsafe {
        assert_eq!(*y, x);
    }
}

fn bar() -> unsafe fn() {
    foo
}

#[test]
fn test_fn_pointer() {
    let f = bar();
    unsafe {
        println!("Calling unsafe function...");
        f();
    }
}
