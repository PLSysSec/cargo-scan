# [mio](https://github.com/tokio-rs/mio)

Audited by: Caleb Stanford
(IN PROGRESS)

Date: 2022-10-07

Mio is a low-level high-performance I/O library -- developed by the tokio
team and predominantly used (as far as I know) by the tokio crate.

TODO: Audit libc first.

Both tokio and mio also rely on `libc`. So the rough chain of wrappers around
a LOT of important system-related code (network, process, and filesystem
accesses) is

tokio -> mio -> libc -> std

mio is the crate with the largest number of unsafe stdlib imports among
the top 1000 crates.
Despite these, it has minimal dependencies: besides libc, basically
just `log`, which uses `cfg-if`.
And it is a much smaller crate than tokio.

## List of imports <!-- number -->

<!-- Copy from experiments/results/top100_all.csv -->

## Analysis

<!-- Detailed audit -->

## Security summary

1. Security risks

<!-- Short answer -->

2. Permissions

<!-- Short answer -->

3. Transitive risk

<!-- Short answer -->

4. Automation feasibility

<!-- Feasible/infeasible -->

- Spec:
- Static analysis:
- Dynamic enforcement overhead:
