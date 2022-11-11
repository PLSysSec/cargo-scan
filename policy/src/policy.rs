/*
    Type representing an audit policy.

    Serializes to and deserializes from a .policy file.
    See example .policy files in policies/
*/

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt::{self, Display};

#[derive(Debug)]
pub struct Expr(String);

#[derive(Debug)]
pub struct Args(String);

#[derive(Debug)]
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
impl Display for Effect {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::EnvRead(e) => write!(f, "env_read({})", e.0),
            Self::EnvWrite(e) => write!(f, "env_write({})", e.0),
            Self::FsRead(e) => write!(f, "fs_read({})", e.0),
            Self::FsWrite(e) => write!(f, "fs_write({})", e.0),
            Self::Exec(v) => {
                // Ineffecient implementation bc .intersperse() / .join()
                // on iterators is not stable
                let v: Vec<&str> = v.iter().map(|e| e.0.as_ref()).collect();
                write!(f, "exec({})", v.join(", "))
            }
        }
    }
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

#[derive(Debug)]
pub enum Region {
    Crate(String),
    Module(String),
    Function(String, Args),
    FunctionAll(String),
}
impl Display for Region {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Crate(s) => write!(f, "{}", s),
            Self::Module(s) => write!(f, "::{}", s),
            Self::Function(s, a) => write!(f, "::{}({})", s, a.0),
            Self::FunctionAll(s) => write!(f, "::{}()", s),
        }
    }
}

#[derive(Debug)]
pub enum Statement {
    Allow { region: Region, effect: Effect },
    Require { region: Region, effect: Effect },
    Trust { region: Region },
}
impl Display for Statement {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Allow { region, effect } => {
                write!(f, "allow {} {}", region, effect)
            }
            Self::Require { region, effect } => {
                write!(f, "require {} {}", region, effect)
            }
            Self::Trust { region } => {
                write!(f, "trust {}", region)
            }
        }
    }
}
/// Serialization for the Statement type
///
/// Serde is unfortunately rather clunky at serializing enums
/// with unnamed fields in each enum variant.
/// E.g., it generates things like
/// ```
/// [[statements]]
/// type = "Allow"
///
/// [statements.expr]
/// region = ["prepare_data", ""]
///
/// [statements.expr.effect]
/// type = "FsWrite"
/// expr = "my_app.log"
/// ```
///
/// Here we leverage Display to get a more compact representation:
/// ```
/// allow prepare_data() FsWrite("my_app.log")
/// ```
impl Serialize for Statement {
    fn serialize<S>(&self, ser: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        ser.collect_str(self)
    }
}
impl<'de> Deserialize<'de> for Statement {
    fn deserialize<D>(_deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        todo!()
    }
}
impl Statement {
    pub fn allow_crate(name: &str, effect: Effect) -> Self {
        let region = Region::Crate(name.to_string());
        Self::Allow { region, effect }
    }
    pub fn allow_mod(name: &str, effect: Effect) -> Self {
        let region = Region::Module(name.to_string());
        Self::Allow { region, effect }
    }
    pub fn allow_fn(name: &str, args: &str, effect: Effect) -> Self {
        let name = name.to_string();
        let args = args.to_string();
        let region = Region::Function(name, Args(args));
        Self::Allow { region, effect }
    }
    // Q: do we need these?
    // pub fn require_crate(name: &str, effect: Effect) -> Self {
    //     Self::Require(Region::Crate(name.to_string()), effect)
    // }
    // pub fn require_mod(name: &str, effect: Effect) -> Self {
    //     Self::Require(Region::Module(name.to_string()), effect)
    // }
    pub fn require_fn(name: &str, args: &str, effect: Effect) -> Self {
        let name = name.to_string();
        let args = args.to_string();
        let region = Region::Function(name, Args(args));
        Self::Require { region, effect }
    }
    pub fn trust_crate(name: &str) -> Self {
        let region = Region::Crate(name.to_string());
        Self::Trust { region }
    }
    pub fn trust_mod(name: &str) -> Self {
        let region = Region::Module(name.to_string());
        Self::Trust { region }
    }
    pub fn trust_fn(name: &str) -> Self {
        let region = Region::FunctionAll(name.to_string());
        Self::Trust { region }
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
