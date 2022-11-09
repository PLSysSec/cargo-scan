/*
    Type representing an audit policy.

    Serializes to and deserializes from a .policy file.
    See example .policy files in policies/
*/

use serde::{Serialize, Deserialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Expr(String);

#[derive(Debug, Serialize, Deserialize)]
pub struct Args(String);

#[derive(Debug, Serialize, Deserialize)]
pub enum Effect {
    EnvRead(String),
    EnvWrite(String),
    FsRead(String),
    FsWrite(String),
    // TBD
    // NetRecv(String),
    // NetSend(String),
    Exec(Vec<String>),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Region {
    Crate(String),
    Module(String),
    Function(String),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Statement {
    Allow(Region, Effect),
    Require(Region, Effect),
    Trust(Region),
}

// TODO: make crate_version and policy_version semver objects
#[derive(Debug, Serialize, Deserialize)]
pub struct Policy {
    crate_name: String,
    crate_version: String,
    policy_version: String,
    statements: Vec<Statement>,
}
impl Policy {
    pub fn new(
        crate_name: &str,
        crate_version: &str,
        policy_version: &str,
    ) -> Self {
        let crate_name = crate_name.to_owned();
        let crate_version = crate_version.to_owned();
        let policy_version = policy_version.to_owned();
        let statements = Vec::new();
        Policy { crate_name, crate_version, policy_version, statements }
    }
}
