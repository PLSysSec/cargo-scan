/*
Representation of an Effect location in source code
*/

use std::path::Path;

fn sanitize_comma(s: &str) -> String {
    if s.contains(',') {
        eprintln!("Warning: ignoring unexpected comma when generating CSV: {s}");
    }
    s.replace(',', "")
}

#[derive(Debug, Clone)]
pub struct EffectPathLoc {
    // Name of crate, e.g. num_cpus
    crt: String,
    // Full path to module, e.g. num_cpus::linux
    module: String,
    // Caller function, e.g. logical_cpus
    caller: String,
}
impl EffectPathLoc {
    pub fn new(filepath: &Path, mod_scope: &[String], caller: String) -> Self {
        // note: tries to infer the crate name and modules from the filepath
        // TBD: use Cargo.toml to get crate name & other info
        let pre_src: Vec<String> = filepath
            .iter()
            .map(|x| {
                x.to_str().unwrap_or_else(|| {
                    panic!("found path that wasn't a valid UTF-8 string: {:?}", x)
                })
            })
            .take_while(|&x| x != "src")
            .map(|x| x.to_string())
            .collect();
        let post_src: Vec<String> = filepath
            .iter()
            .map(|x| {
                x.to_str().unwrap_or_else(|| {
                    panic!("found path that wasn't a valid UTF-8 string: {:?}", x)
                })
            })
            .skip_while(|&x| x != "src")
            .skip(1)
            .filter(|&x| x != "main.rs" && x != "lib.rs")
            .map(|x| x.replace(".rs", ""))
            .collect();

        let crt = pre_src.last().cloned().unwrap_or_else(|| {
            eprintln!("warning: unable to infer crate from path: {:?}", filepath);
            "".to_string()
        });
        let mut module = crt.clone();
        for x in &post_src {
            module.push_str("::");
            module.push_str(x);
        }
        for x in mod_scope {
            module.push_str("::");
            module.push_str(x);
        }

        Self { crt, module, caller }
    }
    pub fn csv_header() -> &'static str {
        "crate, module, caller"
    }
    pub fn to_csv(&self) -> String {
        let crt = sanitize_comma(&self.crt);
        let module = sanitize_comma(&self.module);
        let caller = sanitize_comma(&self.caller);
        format!("{}, {}, {}", crt, module, caller)
    }
}

#[derive(Debug, Clone)]
pub struct EffectSrcLoc {
    // Directory in which the call occurs
    dir: String,
    // File in which the call occurs -- in the above directory
    file: String,
    // Location in which the call occurs -- in the above file
    line: usize,
    col: usize,
}
impl EffectSrcLoc {
    pub fn new(filepath: &Path, line: usize, col: usize) -> Self {
        // TBD: lots can go wrong -- consider returning Result<Self, Err>
        let dir = filepath.parent().unwrap().to_string_lossy().into_owned();
        let file = filepath.file_name().unwrap().to_string_lossy().into_owned();
        Self { dir, file, line, col }
    }
    pub fn csv_header() -> &'static str {
        "dir, file, loc"
    }
    pub fn to_csv(&self) -> String {
        let dir = sanitize_comma(&self.dir);
        let file = sanitize_comma(&self.file);
        let loc = sanitize_comma(&format!("{}:{}", self.line, self.col));
        format!("{}, {}, {}", dir, file, loc)
    }
}

#[derive(Debug, Clone)]
pub struct Effect {
    // Location of caller (Rust path::to::fun)
    caller_loc: EffectPathLoc,
    // Callee (effect) function, e.g. libc::sched_getaffinity
    callee: String,
    // Effect pattern -- prefix of callee (effect), e.g. libc
    pattern: Option<String>,
    // Location of call (Directory, file, line)
    call_loc: EffectSrcLoc,
}

impl Effect {
    pub fn new(
        caller: String,
        callee: String,
        filepath: &Path,
        mod_scope: &[String],
        line: usize,
        col: usize,
    ) -> Self {
        let caller_loc = EffectPathLoc::new(filepath, mod_scope, caller);
        let pattern = None;
        let call_loc = EffectSrcLoc::new(filepath, line, col);
        Self { caller_loc, callee, pattern, call_loc }
    }

    pub fn csv_header() -> &'static str {
        "crate, module, caller, callee, pattern, dir, file, loc"
    }

    #[test]
    fn test_csv_header() {
        assert!(Self::csv_header().starts_with(EffectPathLoc::csv_header()));
        assert!(Self::csv_header().ends_with(EffectSrcLoc::csv_header()));
    }

    pub fn to_csv(&self) -> String {
        let caller_loc_csv = self.caller_loc.to_csv();
        let callee = sanitize_comma(&self.callee);
        let pattern = sanitize_comma(self.pattern.as_deref().unwrap_or("[none]"));
        let call_loc_csv = self.call_loc.to_csv();

        format!("{}, {}, {}, {}", caller_loc_csv, callee, pattern, call_loc_csv)
    }
}
