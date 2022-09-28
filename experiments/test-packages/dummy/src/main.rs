
use serde::{Serialize, Deserialize};
use std::fs::*;
use std::os::*;

// nested and multi-line braces
use std::fs::{mod1, mod2::{test1, test2}};
use std::fs::test::{
    mod1,
    mod2
}

// comments
use std::fs::mod3; // comment, with, commas, in, it
use std::collections::*; // comment with import: std::fs
use std::fs::mod4;//comment-without-spaces

// example missed in rustc_version
use std::{env, error, fmt, io, num, str};
use std::{error, env}; // a variant
