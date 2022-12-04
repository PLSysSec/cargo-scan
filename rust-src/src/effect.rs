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
    pub fn from_caller_name(caller: String) -> Self {
        // TBD: make crt, module Option or get them from somewhere
        Self {
            crt: "".to_string(),
            module: "".to_string(),
            caller,
        }
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
    // Location in which the call occurs -- in the above file (line/col)
    loc: String,
}
impl EffectSrcLoc {
    pub fn from_filepath_loc(filepath: &Path, loc: String) -> Self {
        // TBD: lots can go wrong -- consider returning Result<Self, Err>
        let dir = filepath.parent().unwrap().to_string_lossy().into_owned();
        let file = filepath.file_name().unwrap().to_string_lossy().into_owned();
        Self { dir, file, loc }
    }
    pub fn csv_header() -> &'static str {
        "dir, file, loc"
    }
    pub fn to_csv(&self) -> String {
        let dir = sanitize_comma(&self.dir);
        let file = sanitize_comma(&self.file);
        let loc = sanitize_comma(&self.loc);
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
    pattern: String,
    // Location of call (Directory, file, line)
    call_loc: EffectSrcLoc,
}

impl Effect {
    pub fn new(caller: String, callee: String, filepath: &Path, loc: String) -> Self {
        let caller_loc = EffectPathLoc::from_caller_name(caller);
        let pattern = "".to_string(); // TBD
        let call_loc = EffectSrcLoc::from_filepath_loc(filepath, loc);
        Self {
            caller_loc,
            callee,
            pattern,
            call_loc,
        }
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
        let pattern = sanitize_comma(&self.pattern);
        let call_loc_csv = self.call_loc.to_csv();

        format!(
            "{}, {}, {}, {}",
            caller_loc_csv, callee, pattern, call_loc_csv
        )
    }
}
