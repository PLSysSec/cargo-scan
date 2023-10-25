pub mod sub;

pub fn no_effect() {
    println!("no_effect");
}

pub fn has_effect() {
    println!("has_effect");
    sub::effect();
}
