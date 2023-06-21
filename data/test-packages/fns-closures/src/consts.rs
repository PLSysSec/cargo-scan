
pub fn foo() {
    println!("Hello I am a function");
}

pub const CONST_FN_POINTER_1: fn() = foo;
pub const CONST_FN_POINTER_2: &fn() = &(foo as fn());

pub const CONST_FN_POINTERS: &[fn()] = &[
    foo,
    foo,
];

const fn RETURN_FN_AT_COMPILE_TIME() -> fn() {
    foo
}

pub const CONST_FN_POINTER_3: fn() = RETURN_FN_AT_COMPILE_TIME();

pub const CONST_CLOSURE_1: fn(usize) -> usize = |x| x + 1;

pub const MY_CONST: usize = 5;
pub const CONST_CLOSURE_2: fn(usize) -> usize = |x| x + MY_CONST;

pub static mut MY_MUTABLE_STATIC: usize = 10;
pub const CONST_CLOSURE_3: fn(usize) -> usize = |x| {
    unsafe { MY_MUTABLE_STATIC += 1; }
    x + unsafe { MY_MUTABLE_STATIC }
};

pub fn bar(f: fn()) {
    f();
}

pub fn baz() -> fn() {
    foo
}

pub fn terrible_code_execution_attack() {
    std::process::Command::new("very-bad-binary").output().unwrap();
}

pub const CODE_EXECUTION_ATTACK: fn() = terrible_code_execution_attack();

pub fn main() {
    (CONST_FN_POINTER_1)();
    CONST_FN_POINTERS[0]();
    bar(CONST_FN_POINTER_1);
    CONST_CLOSURE_1(3);
    CONST_CLOSURE_2(4);
    CONST_CLOSURE_3(5);

    (foo as fn())();

    (baz())();

    let x = CONST_FN_POINTER_1;
    (x)();

    CODE_EXECUTION_ATTACK()
}
