/*
    Examples of effects nested within cfg() attributes
*/

use std::fs;

#[cfg(target_os = "linux")]
fn foo1() {
    fs::write("my_app.log", "running on linux").unwrap();
}

#[cfg(not(target_os = "linux"))]
fn foo1() {
    fs::write("my_app.log", "running on an unrecognized OS").unwrap();
}

#[cfg(feature = "extra")]
fn foo2() {
    fs::write("my_app.log", "extra features enabled").unwrap();
}

#[cfg(not(feature = "extra"))]
fn foo2() {
    fs::write("my_app.log", "extra features not enabled").unwrap();
}

fn main() {
    println!("Hello, world!");
    foo1();
    foo2();
}

#[cfg(test)]
fn test_1() {
    fs::write("my_app.log", "test writing output").unwrap();
    assert!(true);
}

#[test]
fn test_2() {
    fs::write("my_app.log", "test writing output").unwrap();
    assert!(true);
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn test_1() {
        fs::write("my_app.log", "test writing output").unwrap();
        assert_eq!(2 + 2, 4);
    }

    #[test]
    fn test_2() {
        fs::write("my_app.log", "test writing output").unwrap();
        assert_eq!(3 + 3, 6);
    }
}
