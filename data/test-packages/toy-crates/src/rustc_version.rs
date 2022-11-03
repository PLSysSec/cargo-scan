/*
    Toy version of the rustc_version crate.
*/

use std::env;
use std::ffi::OsString;
use std::process::Command;

// VersionMeta::for_command
pub fn version_meta_for_command(mut cmd: Command) -> String {
    let out = cmd.arg("-vV").output().unwrap();
    String::from_utf8(out.stdout).unwrap()
}

pub fn version_meta() -> String {
    let cmd = env::var_os("RUSTC").unwrap_or_else(|| OsString::from("rustc"));
    version_meta_for_command(Command::new(cmd))
}
