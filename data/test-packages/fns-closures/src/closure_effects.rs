use std::fs::read_to_string;
use std::path::Path;

pub fn return_file_reader_1(f: &Path) -> impl Fn() -> String + '_ {
    move || { read_to_string(f).unwrap() }
}

pub fn return_file_reader_2() -> impl Fn(&Path) -> String {
    |f: &Path| { read_to_string(f).unwrap() }
}

#[test]
fn main() {
    let p = Path::new("test.txt");
    let text1 = return_file_reader_1(p)();
    let text2 = return_file_reader_2()(p);
    assert_eq!(text1, "test\n");
    assert_eq!(text2, "test\n");
}
