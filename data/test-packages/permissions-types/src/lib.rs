/*
Some examples to help think about different types of function-level
permissions.

2022-10-17
*/

use std::fs;
use std::process::Command;

pub fn remove(path: &str) {
    Command::new("rm").arg("-f").arg(path).output().unwrap();
}

pub fn save_data(data: &str, path: &str) {
    remove(path);
    fs::write(path, data).unwrap()
}

pub fn prepare_data(data: Vec<String>) -> String {
    if data.len() > 100 {
        fs::write("my_app.log", "warning: preparing more than 100 rows").unwrap();
    }
    data.join("\n")
}
