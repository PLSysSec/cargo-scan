use std::fs;
use std::io::Read;

pub fn read_fn() -> Option<()> {
     if let Ok(mut f) = fs::File::open("foo.txt") {
         let mut buffer = Vec::new();

         match f.read_to_end(&mut buffer) {
             Ok(_) => Some(()),
             Err(_) => None,
        }
    } else {
        None
    }
}

pub fn unsafe_deref() -> Option<u32> {
    let x: i32 = 5;
    let y: *mut i32 = x as *mut i32;
    unsafe {
        *y = 6;
    }
    Some(1)
}
