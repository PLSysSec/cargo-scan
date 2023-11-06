# [memchr](https://docs.rs/memchr/latest/memchr/)

Audited by: Caleb Stanford

Date: 2022-10-12

18th most downloaded crate.

In the top 100 crates, `memchr` and `getrandom` are the only
crates that had no stdlib dangerous imports, and were only
flagged after including third party imports (in both cases, `libc`).

## List of imports (3)

```
memchr, libc, data/packages/memchr/src/memchr, c.rs, libc::c_int
memchr, libc, data/packages/memchr/src/memchr, c.rs, libc::c_void
memchr, libc, data/packages/memchr/src/memchr, c.rs, libc::size_t
```

## Analysis

Types only.

## Security summary

1. Security risks

None

2. Permissions

None

3. Transitive risk

None

4. Automation feasibility

- Spec: project-independent
- Static analysis: feasible
- Dynamic enforcement overhead: unsure
