//use crate_scan::audit_chain;
use anyhow::Result;
use assert_cmd::prelude::*;
use std::fs;
use std::path::Path;
use std::process::Command;

// hanging -- ignored
#[ignore]
#[test]
fn cross_crate_effects() -> Result<()> {
    // Clean up previous test
    let audit_test_path = Path::new("./.audit_test");
    if audit_test_path.exists() && audit_test_path.is_dir() {
        fs::remove_dir_all(audit_test_path)?;
    }

    // Create the new audit chain for the child package
    let output1 = Command::cargo_bin("chain")?
        .args([
            "create",
            "./data/test-packages/dependency-ex",
            "./.audit_test/dependency-ex.manifest",
        ])
        .args(["-p", "./.audit_test"])
        .output()?;
    println!("{:?}", output1);

    // Create the chain for the parent package
    let output2 = Command::cargo_bin("chain")?
        .args([
            "create",
            "./data/test-packages/dependency-parent",
            "./.audit_test/dependency-parent.manifest",
        ])
        .args(["-p", "./.audit_test"])
        .output()?;
    println!("{:?}", output2);

    // The chain for the parent should re-use the existing audit for the child,
    // so the above command should succeed without having to force-overwrite the
    // audit files

    Ok(())
}
