pub mod sub;

pub fn no_effect() {
    println!("no_effect");
}

pub fn has_direct_effect() {
    unsafe {
        libc::sysconf(57);
    }
}

pub fn has_indirect_effect() {
    println!("has_effect");
    sub::effect();
}
