pub mod policy;
use policy::Policy;

fn main() {
    let policy = Policy::new("permissions-ex", "0.1", "0.1");

    println!("Policy example: {:?}", policy);
}
