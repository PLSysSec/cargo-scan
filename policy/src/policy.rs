/*
    Type representing an audit policy.

    Serializes to and deserializes from a .policy file.
    See example .policy files in policies/
*/

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Expr(String);

#[derive(Debug, Serialize, Deserialize)]
pub struct Args(String);

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "c")]
pub enum Effect {
    EnvRead(Expr),
    EnvWrite(Expr),
    FsRead(Expr),
    FsWrite(Expr),
    // TBD
    // NetRecv(String),
    // NetSend(String),
    Exec(Vec<Expr>),
}
impl Effect {
    pub fn env_read(s: &str) -> Self {
        Self::EnvRead(Expr(s.to_string()))
    }
    pub fn env_write(s: &str) -> Self {
        Self::EnvWrite(Expr(s.to_string()))
    }
    pub fn fs_read(s: &str) -> Self {
        Self::FsRead(Expr(s.to_string()))
    }
    pub fn fs_write(s: &str) -> Self {
        Self::FsWrite(Expr(s.to_string()))
    }
    pub fn fs_create(s: &str) -> Self {
        // TBD: distinguish more
        Self::FsWrite(Expr(s.to_string()))
    }
    pub fn fs_delete(s: &str) -> Self {
        // TBD: distinguish more
        Self::FsWrite(Expr(s.to_string()))
    }
    pub fn fs_append(s: &str) -> Self {
        // TBD: distinguish more
        Self::FsWrite(Expr(s.to_string()))
    }
    pub fn exec(cmd: &str, args: &[&str]) -> Self {
        let mut result = vec![Expr(cmd.to_string())];
        for arg in args {
            result.push(Expr(arg.to_string()))
        }
        Self::Exec(result)
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "c")]
pub enum Region {
    Crate(String),
    Module(String),
    Function(String, Args),
    FunctionAll(String),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "c")]
pub enum Statement {
    Allow(Region, Effect),
    Require(Region, Effect),
    Trust(Region),
}
impl Statement {
    pub fn allow_crate(name: &str, eff: Effect) -> Self {
        Self::Allow(Region::Crate(name.to_string()), eff)
    }
    pub fn allow_mod(name: &str, eff: Effect) -> Self {
        Self::Allow(Region::Module(name.to_string()), eff)
    }
    pub fn allow_fn(name: &str, args: &str, eff: Effect) -> Self {
        let name = name.to_string();
        let args = args.to_string();
        Self::Allow(Region::Function(name, Args(args)), eff)
    }
    // Q: do we need these?
    // pub fn require_crate(name: &str, eff: Effect) -> Self {
    //     Self::Require(Region::Crate(name.to_string()), eff)
    // }
    // pub fn require_mod(name: &str, eff: Effect) -> Self {
    //     Self::Require(Region::Module(name.to_string()), eff)
    // }
    pub fn require_fn(name: &str, args: &str, eff: Effect) -> Self {
        let name = name.to_string();
        let args = args.to_string();
        Self::Require(Region::Function(name, Args(args)), eff)
    }
    pub fn trust_crate(name: &str) -> Self {
        Self::Trust(Region::Crate(name.to_string()))
    }
    pub fn trust_mod(name: &str) -> Self {
        Self::Trust(Region::Module(name.to_string()))
    }
    pub fn trust_fn(name: &str) -> Self {
        Self::Trust(Region::FunctionAll(name.to_string()))
    }
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
    pub fn add_statement(&mut self, s: Statement) {
        self.statements.push(s);
    }
    pub fn allow_crate(&mut self, name: &str, eff: Effect) {
        self.add_statement(Statement::allow_crate(name, eff))
    }
    pub fn allow_mod(&mut self, name: &str, eff: Effect) {
        self.add_statement(Statement::allow_mod(name, eff))
    }
    pub fn allow_fn(&mut self, name: &str, args: &str, eff: Effect) {
        self.add_statement(Statement::allow_fn(name, args, eff))
    }
    pub fn require_fn(&mut self, name: &str, args: &str, eff: Effect) {
        self.add_statement(Statement::require_fn(name, args, eff))
    }
    pub fn trust_crate(&mut self, name: &str) {
        self.add_statement(Statement::trust_crate(name))
    }
    pub fn trust_mod(&mut self, name: &str) {
        self.add_statement(Statement::trust_mod(name))
    }
    pub fn trust_fn(&mut self, name: &str) {
        self.add_statement(Statement::trust_fn(name))
    }
}
