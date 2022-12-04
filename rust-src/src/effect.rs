/*
Representation of an Effect location in source code
*/

pub struct Effect {
    // Name of crate, e.g. num_cpus
    crt: String,
    // Full path to module, e.g. num_cpus::linux
    module: String,
    // Caller function, e.g. logical_cpus
    caller: String,
    // Callee (effect) function, e.g. libc::sched_getaffinity
    callee: String,
    // Effect pattern -- prefix of callee (effect), e.g. libc
    pattern: String,
    // Directory in which the call occurs
    dir: String,
    // File in which the call occurs -- in the above directory
    file: String,
    // Loc in which the call occurs -- in the above file
    loc: String,
}

fn sanitize_comma(s: &str) -> String {
    if s.contains(',') {
        eprintln!("Warning: ignoring comma when generating CSV: {s}");
    }
    s.replace(',', "")
}

impl Effect {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        crt: String,
        module: String,
        caller: String,
        callee: String,
        pattern: String,
        dir: String,
        file: String,
        loc: String,
    ) -> Self {
        Self {
            crt,
            module,
            caller,
            callee,
            pattern,
            dir,
            file,
            loc,
        }
    }

    pub fn csv_header() -> &'static str {
        "crate, module, caller, callee, pattern, dir, file, loc"
    }

    pub fn to_csv(&self) -> String {
        let crt = sanitize_comma(&self.crt);
        let module = sanitize_comma(&self.module);
        let caller = sanitize_comma(&self.caller);
        let callee = sanitize_comma(&self.callee);
        let pattern = sanitize_comma(&self.pattern);
        let dir = sanitize_comma(&self.dir);
        let file = sanitize_comma(&self.file);
        let loc = sanitize_comma(&self.loc);

        let v = vec![crt, module, caller, callee, pattern, dir, file, loc];
        v.join(", ")
    }
}
