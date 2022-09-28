
use serde::{Serialize, Deserialize};
use std::fs::*;
use std::os::*;

// nested braces
use std::fs::{mod1, mod2::{test1, test2}};
use std::{a::b::{c::d, e::f}, process::{process::Command, Command}::*};

// multi-line braces
use std::fs::test::{
    mod3,
    mod4
};
use std::{
    a, fs::mod5
};
use std::{
    env,
    b,
}
;

// comments
use std::fs::mod6; // comment, with, commas, in, it
use std::collections::*; // comment with import: std::fs
use std::fs::mod7;//comment-without-spaces

// example missed in rustc_version
use std::{env, error, fmt, io, num, str};
use std::{error, env}; // a variant
