pub mod policy;
use policy::Policy;
use toml;

fn main() {
    let policy = Policy::new("permissions-ex", "0.1", "0.1");

    println!("Policy example: {:?}", policy);

    println!("{}", toml::to_string(&policy).unwrap());
}
