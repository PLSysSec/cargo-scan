pub mod policy;
use policy::{Effect, Policy};
use toml;

fn main() {
    let mut policy = Policy::new("permissions-ex", "0.1", "0.1");
    policy.require_fn("remove", "path", Effect::fs_delete("path"));
    policy.require_fn("save_data", "path", Effect::fs_create("path"));
    policy.require_fn("save_data", "path", Effect::fs_write("path"));
    // TODO: path is a variable, -f is a string
    policy.allow_fn("remove", "path", Effect::exec("rm", &["-f", "path"]));
    policy.allow_fn("save_data", "path", Effect::fs_delete("path"));
    policy.allow_fn("prepare_data", "", Effect::fs_append("my_app.log"));
    // example of trust statements
    policy.trust_fn("prepare_data");

    println!("Policy example: {:?}", policy);

    println!("{}", toml::to_string(&policy).unwrap());
}
