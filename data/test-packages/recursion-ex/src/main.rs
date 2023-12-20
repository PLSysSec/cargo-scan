use std::process::Command;

fn effect1() {
    let _ = Command::new("ls").output();
}

fn effect2() {
    let _ = Command::new("echo hi").output();
}

fn f() {
    effect1();
    g();
}

fn g() {
    effect2();
    f();
}

fn h() {
    effect1();
    h();
}

fn main() {
    f();
    g();
    h();
}
