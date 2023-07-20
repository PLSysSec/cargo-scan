# Auditing with cargo-scan

Audits are tied to particular crates and can be carried out either on just the
crate itself, or on a crate and all its dependencies.

## Individual crates

Auditing just the crate without its dependencies will only flag effects that
happen in that crate as effects, and *will not* look at functions with effects
in other crates. If a function from another crate is marked as `unsafe`, it will
still be labeled as an effect.

Auditing individual crates is done with the `audit` binary. The auditor will
provide the path to the crate to be audited, as well as where the audit should
be saved. Full command line arguments are documented in the binary.

We call a particular audit on a crate a "policy" to designate its role in
specifying the behavior in a package that the auditor deems potentially unsafe.
Note that which effects are dangerous may depend on the context the package is
running in. For instance, in some situations file system access may be
permissible.

## Crates with dependencies

The method of auditing crates while taking into account their dependencies is
through the `chain` binary. This binary introduces the notion of a policy
"chain", which is the collection of policy files corresponding to each
dependency for the top-level crate, as well as the top-level crate itself. Note
that these policy files relate to each other via the crate's dependency graph,
hence "chain". Updates to dependency policies may update parent policies, so all
auditing on a policy chain should be handled through the `chain` binary to
ensure it remains well-formed.

The `chain` binary has several subcommands for interacting with a chain in
different ways. The first subcommand you should run is `chain create`. This
subcommand will take the specified top-level crate and iterate through it and
all its dependencies making the caller-checked default policy for each of them
until the top-level crate. This step will likely take a while because of
inefficiencies right now with rust analyzer, but also because it tracks a lot of
information across packages. By default, it will look for the crate in the
directory specified, but you can also download it with flags.

Once the chain has been created, you can now go through the process of auditing
it. It's easiest to first list the packages that have been added with the review
command (`chain review path_to.manifest -i crates`), then run the audit command
with the full crate name (`chain audit path_to.manifest full_crate_name-0.0.1`).

When auditing packages in the chain, you can start auditing the top-level
package and follow effects from there through the dependency chain into the
packages that cause them, or you can audit the lower-level packages directly.

***WARNING*** Do not edit the policy files for an audit-chain using the `audit`
binary. There is information shared across policies in a chain which will cause
the chain to become malformed if the policies are update through tools other
than `chain`.

You can also review effects for individual packages to varying levels of detail
with the `chain review` subcommand. This is useful for things like inspecting
which public functions are marked caller-checked in dependency packages.
