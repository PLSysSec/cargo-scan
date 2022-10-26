# Policy language

(Work in progress, October 2022)

## Threat model

We want to attach a security policy to each build script, crate, module, and function.
Some of these are audited manually, others inferred.
I would like to be able to prove a security property under the following threat model:

- adversary can update any build, crate, or module arbitrarily, other than audited functions, as long as the inferred policies for those are still consistent

- no matter what the adversary does, program should be secure in the sense that any build or run of the program uses only the capabilities explicitly passed by the user to perform necessary functionality.

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
