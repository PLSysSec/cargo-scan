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

### Examples and Syntax

TBD

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
