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
    pub fn env_read(s: String) -> Self {
        Self::EnvRead(Expr(s))
    }
    pub fn env_write(s: String) -> Self {
        Self::EnvWrite(Expr(s))
    }
    pub fn fs_read(s: String) -> Self {
        Self::FsRead(Expr(s))
    }
    pub fn fs_write(s: String) -> Self {
        Self::FsWrite(Expr(s))
    }
    pub fn exec(cmd: String, args: Vec<String>) -> Self {
        let mut result = vec![Expr(cmd)];
        for arg in args.into_iter() {
            result.push(Expr(arg))
        }
        Self::Exec(result)
    }
}


#[derive(Debug, Serialize, Deserialize)]
pub enum Region {
    Crate(String),
    Module(String),
    Function(String, Args),
    FunctionAll(String),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Statement {
    Allow(Region, Effect),
    Require(Region, Effect),
    Trust(Region),
}
impl Statement {
    pub fn allow_crate(name: String, eff: Effect) -> Self {
        Self::Allow(Region::Crate(name), eff)
    }
    pub fn allow_mod(name: String, eff: Effect) -> Self {
        Self::Allow(Region::Module(name), eff)
    }
    pub fn allow_fn(name: String, args: String, eff: Effect) -> Self {
        Self::Allow(Region::Function(name, Args(args)), eff)
    }
    // Q: do we need these?
    // pub fn require_crate(name: String, eff: Effect) -> Self {
    //     Self::Require(Region::Crate(name), eff)
    // }
    // pub fn require_mod(name: String, eff: Effect) -> Self {
    //     Self::Require(Region::Module(name), eff)
    // }
    pub fn require_fn(name: String, args: String, eff: Effect) -> Self {
        Self::Require(Region::Function(name, Args(args)), eff)
    }
    pub fn trust_crate(name: String) -> Self {
        Self::Trust(Region::Crate(name))
    }
    pub fn trust_mod(name: String) -> Self {
        Self::Trust(Region::Module(name))
    }
    pub fn trust_fn(name: String) -> Self {
        Self::Trust(Region::FunctionAll(name))
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
    pub fn allow_crate(&mut self, name: String, eff: Effect) {
        self.add_statement(Statement::allow_crate(name, eff))
    }
    pub fn allow_mod(&mut self, name: String, eff: Effect) {
        self.add_statement(Statement::allow_mod(name, eff))
    }
    pub fn allow_fn(&mut self, name: String, args: String, eff: Effect) {
        self.add_statement(Statement::allow_fn(name, args, eff))
    }
    pub fn require_fn(&mut self, name: String, args: String, eff: Effect) {
        self.add_statement(Statement::require_fn(name, args, eff))
    }
    pub fn trust_crate(&mut self, name: String) {
        self.add_statement(Statement::trust_crate(name))
    }
    pub fn trust_mod(&mut self, name: String) {
        self.add_statement(Statement::trust_mod(name))
    }
    pub fn trust_fn(&mut self, name: String) {
        self.add_statement(Statement::trust_fn(name))
    }
}
