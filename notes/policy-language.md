# Policy language

## Components

Each policy has the following components.
The auditor identifies the following:
- Effective permissions: permissions needed by the crate in effect
  For example, a crate that only reads config files might in effect have no
  effective permissions.
- Internal permissions: permissions needed by the crate internally,
  given by the auditor
  Example: the auditor might declare it safe for the crate to have access
  to some config files
- Exceptions: function calls that require additional permissions.
Each has the form:
```
function_name, lambda |(arg1, arg2, ...)| {permissions needed}
```
where `{permissions needed}` are capabilities to the arguments that
are passed in by the caller.
- Assumptions: these are requirements made on subcalls or dependencies
at each call site.

## Syntax

**Work in progress**

For simplicity, assume that a *capability* `c` is an element of a fixed lattice `L`.
A *namespace* `n` is either the top-level application `()`, a crate `c`, a module `c::m` within a crate, or a function `c::m::f` within a module.
A *principal* is defined by the grammar
```
p ::= user() | audit(n) | build(n) | import(n) | f(arg_pattern)
```

A *permission* consists of one principal giving permissions to another: `(p, c) -> (p', c')`, which can be quantified in the case of arguments: like `forall x. call(f,` ...
and a *policy* is a list of permissions.
```
P ::= nil | () ->_c p', P
```

Permissions intuitively "flow" from the user to various other principals;
a principal has some permissions if there is a path from `user()` to the principal.

<!-- TODO: maybe some well-formedness conditions on the list
of permissions. There really should be a top-down structure to how the
permissions flow. -->

## Examples

### test-packages/permissions-ex

<!-- TBD -->

### num_cpus

<!--
num_cpus. External:
```
libc_spec -> read(sysinfo)
```
Internal:
```
num_cpus: read(sysinfo)
    + libc::sysconf(_SC_NPROCESSORS_ONLN)
    + libc::sysconf(CONF_NAME)
    + libc::sysctl(x)
    + libc_spec
libc::sysctl + libc::sysctlbyname(
```
-->
