/*
    Type representing an audit policy.

    Serializes to and deserializes from a .policy file.
    See example .policy files in policies/
*/

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashSet;
use std::fmt::{self, Display};
use std::str::FromStr;

#[derive(Debug, PartialEq, Eq)]
pub struct Expr(String);

#[derive(Debug, PartialEq, Eq)]
pub struct Ident(String);

#[derive(Debug, PartialEq, Eq)]
pub struct Args(String);

/// Simplified effect model
/// Serialized syntax: [fn name]([args]) or [fn name](*)
#[derive(Debug, PartialEq, Eq)]
pub enum Effect {
    // effectful stdlib function call on any args
    FnAll(Ident),
    // effectful stdlib function call on specific args
    FnCall(Ident, Args),
}
impl Display for Effect {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::FnAll(ident) => write!(f, "{}(*)", ident.0),
            Self::FnCall(ident, args) => write!(f, "{}({})", ident.0, args.0),
        }
    }
}
impl Serialize for Effect {
    fn serialize<S>(&self, ser: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        ser.collect_str(self)
    }
}
impl FromStr for Effect {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (s1, s23) = s.split_once('(').ok_or("expected ( in Effect")?;
        let (s2, s3) = s23.split_once(')').ok_or("expected ) in Effect")?;
        if !s3.is_empty() {
            Err("expected empty string after )")
        } else if s1.is_empty() {
            Err("expected nonempty fn name")
        } else if s2 == "*" {
            Ok(Self::all(s1))
        } else {
            Ok(Self::call(s1, s2))
        }
    }
}
impl<'de> Deserialize<'de> for Effect {
    fn deserialize<D>(des: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(des)?.parse().map_err(de::Error::custom)
    }
}
impl Effect {
    pub fn all(s1: &str) -> Self {
        let name = Ident(s1.to_string());
        Self::FnAll(name)
    }
    pub fn call(s1: &str, s2: &str) -> Self {
        let name = Ident(s1.to_string());
        let args = Args(s2.to_string());
        Self::FnCall(name, args)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Region {
    // crate
    Crate(String),
    // crate::mod
    Module(String, String),
    // crate::mod::fun
    Function(String, String, String),
    // crate::mod::fun::args
    FunctionCall(String, String, String, Args),
}
impl Display for Region {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // TODO: debug assert that strings are alphanumeric, no ::
        match self {
            Self::Crate(cr) => write!(f, "{}", cr),
            Self::Module(cr, md) => write!(f, "{}::{}", cr, md),
            Self::Function(cr, md, fun) => write!(f, "{}::{}::{}", cr, md, fun),
            Self::FunctionCall(cr, md, fun, args) => {
                write!(f, "{}::{}::{}::{}", cr, md, fun, args.0)
            }
        }
    }
}
impl Serialize for Region {
    fn serialize<S>(&self, ser: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        ser.collect_str(self)
    }
}
impl FromStr for Region {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split("::").collect();
        let reg = match parts.as_slice() {
            [cr] => Self::whole_crate(cr),
            [cr, md] => Self::module(cr, md),
            [cr, md, fun] => Self::function(cr, md, fun),
            [cr, md, fun, args] => Self::function_call(cr, md, fun, args),
            _ => return Err("expected at most 3 :: separators in Region"),
        };
        Ok(reg)
    }
}
impl<'de> Deserialize<'de> for Region {
    fn deserialize<D>(des: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(des)?.parse().map_err(de::Error::custom)
    }
}
impl Region {
    pub fn whole_crate(cr: &str) -> Self {
        let cr = cr.to_string();
        Self::Crate(cr)
    }
    pub fn module(cr: &str, md: &str) -> Self {
        let cr = cr.to_string();
        let md = md.to_string();
        Self::Module(cr, md)
    }
    pub fn function(cr: &str, md: &str, fun: &str) -> Self {
        let cr = cr.to_string();
        let md = md.to_string();
        let fun = fun.to_string();
        Self::Function(cr, md, fun)
    }
    pub fn function_call(cr: &str, md: &str, fun: &str, args: &str) -> Self {
        let cr = cr.to_string();
        let md = md.to_string();
        let fun = fun.to_string();
        let args = Args(args.to_string());
        Self::FunctionCall(cr, md, fun, args)
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
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
impl Statement {
    pub fn allow_crate(cr: &str, effect: Effect) -> Self {
        let region = Region::whole_crate(cr);
        Self::Allow { region, effect }
    }
    pub fn allow_mod(cr: &str, md: &str, effect: Effect) -> Self {
        let region = Region::module(cr, md);
        Self::Allow { region, effect }
    }
    pub fn allow_fn(cr: &str, md: &str, fun: &str, args: &str, effect: Effect) -> Self {
        let region = Region::function_call(cr, md, fun, args);
        Self::Allow { region, effect }
    }
    // Q: do we need these?
    // pub fn require_crate(name: &str, effect: Effect) -> Self {
    //     Self::Require(Region::Crate(name.to_string()), effect)
    // }
    // pub fn require_mod(name: &str, effect: Effect) -> Self {
    //     Self::Require(Region::Module(name.to_string()), effect)
    // }
    pub fn require_fn(cr: &str, md: &str, fun: &str, args: &str, effect: Effect) -> Self {
        let region = Region::function_call(cr, md, fun, args);
        Self::Require { region, effect }
    }
    pub fn trust_crate(cr: &str) -> Self {
        let region = Region::whole_crate(cr);
        Self::Trust { region }
    }
    pub fn trust_mod(cr: &str, md: &str) -> Self {
        let region = Region::module(cr, md);
        Self::Trust { region }
    }
    pub fn trust_fn(cr: &str, md: &str, fun: &str) -> Self {
        let region = Region::function(cr, md, fun);
        Self::Trust { region }
    }
}

// TODO: make crate_version and policy_version semver objects
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policy {
    crate_name: String,
    crate_version: String,
    policy_version: String,
    statements: Vec<Statement>,
}
impl Policy {
    pub fn new(crate_name: &str, crate_version: &str, policy_version: &str) -> Self {
        let crate_name = crate_name.to_string();
        let crate_version = crate_version.to_string();
        let policy_version = policy_version.to_string();
        let statements = Vec::new();
        Policy { crate_name, crate_version, policy_version, statements }
    }
    pub fn add_statement(&mut self, s: Statement) {
        self.statements.push(s);
    }
    pub fn allow_crate(&mut self, cr: &str, eff: Effect) {
        self.add_statement(Statement::allow_crate(cr, eff))
    }
    pub fn allow_mod(&mut self, cr: &str, md: &str, eff: Effect) {
        self.add_statement(Statement::allow_mod(cr, md, eff))
    }
    pub fn allow_fn(&mut self, cr: &str, md: &str, fun: &str, args: &str, eff: Effect) {
        self.add_statement(Statement::allow_fn(cr, md, fun, args, eff))
    }
    pub fn require_fn(&mut self, cr: &str, md: &str, fun: &str, args: &str, eff: Effect) {
        self.add_statement(Statement::require_fn(cr, md, fun, args, eff))
    }
    pub fn trust_crate(&mut self, cr: &str) {
        self.add_statement(Statement::trust_crate(cr))
    }
    pub fn trust_mod(&mut self, cr: &str, md: &str) {
        self.add_statement(Statement::trust_mod(cr, md))
    }
    pub fn trust_fn(&mut self, cr: &str, md: &str, fun: &str) {
        self.add_statement(Statement::trust_fn(cr, md, fun))
    }
}

/// Quick-lookup summary of the policy.
/// Note: may make more sense to merge these fields into Policy eventually; current separate
/// because would require custom serialization/deserialization logic.
#[allow(dead_code, unused_variables)]
#[derive(Debug)]
pub struct PolicyLookup {
    allow_set: HashSet<Ident>,
    require_set: HashSet<Ident>,
}
#[allow(dead_code, unused_variables)]
impl PolicyLookup {
    pub fn empty() -> Self {
        Self { allow_set: HashSet::new(), require_set: HashSet::new() }
    }
    pub fn from_policy(p: &Policy) -> Self {
        let mut result = Self::empty();
        for stmt in &p.statements {
            result.add_statement(stmt);
        }
        result
    }
    pub fn add_statement(&mut self, stmt: &Statement) {
        match stmt {
            Statement::Allow { region: r, effect: e } => {
                unimplemented!()
            }
            Statement::Require { region: r, effect: e } => {
                unimplemented!()
            }
            Statement::Trust { region: _ } => {
                unimplemented!()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_policy_serialize_deserialize() {
        // Note: this example uses dummy strings that don't correspond
        // to real effects
        let cr = "permissions-ex";
        let md = "lib";
        let mut policy = Policy::new(cr, "0.1", "0.1");
        let eff1 = Effect::call("fs::delete", "path");
        policy.require_fn(cr, md, "remove", "path", eff1);
        let eff2 = Effect::call("fs::create", "path");
        policy.require_fn(cr, md, "save_data", "path", eff2);
        let eff3 = Effect::call("fs::write", "path");
        policy.require_fn(cr, md, "save_data", "path", eff3);
        let eff4 = Effect::call("process::exec", "rm -f path");
        policy.allow_fn(cr, md, "remove", "path", eff4);
        let eff5 = Effect::call("fs::delete", "path");
        policy.allow_fn(cr, md, "save_data", "path", eff5);
        let eff6 = Effect::call("fs::append", "my_app.log");
        policy.allow_fn(cr, md, "prepare_data", "", eff6);
        // example of trust statements
        policy.trust_fn(cr, md, "prepare_data");

        println!("Policy example: {:?}", policy);

        let policy_toml = toml::to_string(&policy).unwrap();
        println!("Policy serialized: {}", policy_toml);

        let policy2: Policy = toml::from_str(&policy_toml).unwrap();
        println!("Policy deserialized again: {:?}", policy2);

        assert_eq!(policy, policy2);
    }
}
