//use crate_scan::audit_chain;
use anyhow::Result;
use assert_cmd::prelude::*;
use std::fs;
use std::path::Path;
use std::process::Command;

#[test]
fn cross_crate_effects() -> Result<()> {
    // Clean up previous test
    let policy_test_path = Path::new("./.policy_test");
    if policy_test_path.exists() && policy_test_path.is_dir() {
        fs::remove_dir_all(policy_test_path)?;
    }

    // Create the new audit chain for the child package
    let _cmd = Command::cargo_bin("chain")?
        .args([
            "create",
            "./data/test-packages/dependency-ex",
            "./.policy_test/dependency-ex.manifest",
            "./.policy_test",
        ])
        .output();

    // Create the chain for the parent package
    let _cmd = Command::cargo_bin("chain")?
        .args([
            "create",
            "./data/test-packages/dependency-parent",
            "./.policy_test/dependency-parent.manifest",
            "./.policy_test",
        ])
        .output();

    Ok(())
}
