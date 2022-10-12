# [chrono](https://docs.rs/chrono/latest/chrono/)

Audited by: Caleb Stanford

Date: 2022-10-12

65th most downloaded crate

## List of imports (6)

```
src/offset/local, unix.rs, std::env
src/offset/local, unix.rs, std::fs
src/offset/local/tz_info, timezone.rs, std::fs::File
src/offset/local/tz_info, timezone.rs, std::fs::self
src/offset/local/tz_info, timezone.rs, std::path::Path
src/offset/local/tz_info, timezone.rs, std::path::PathBuf
```

## Analysis

Among other things, on Unix, calls
```
fs::symlink_metadata("/etc/localtime")
```
to get time information (if it fails, falls back on a different method).
Throughout the crate, uses the usual stdlib `SystemTime::now()`.

## Security summary

1. Security risks

Access to machine's location data

2. Permissions

Read-only file system access to fixed file; clock/time access

3. Transitive risk

None

4. Automation feasibility

- Spec: project-independent
- Static analysis: feasible
- Dynamic enforcement overhead: likely not acceptable
