pub mod policy;
use policy::{Effect, Policy};

fn main() {
    let cr = "permissions-ex";
    let md = "lib";
    let mut policy = Policy::new(cr, "0.1", "0.1");
    policy.require_fn(cr, md, "remove", "path", Effect::fs_delete("path"));
    policy.require_fn(cr, md, "save_data", "path", Effect::fs_create("path"));
    policy.require_fn(cr, md, "save_data", "path", Effect::fs_write("path"));
    // TODO: path is a variable, -f is a string
    policy.allow_fn(
        cr,
        md,
        "remove",
        "path",
        Effect::exec("rm", &["-f", "path"]),
    );
    policy.allow_fn(cr, md, "save_data", "path", Effect::fs_delete("path"));
    policy.allow_fn(
        cr,
        md,
        "prepare_data",
        "",
        Effect::fs_append("my_app.log"),
    );
    // example of trust statements
    policy.trust_fn(cr, md, "prepare_data");

    println!("Policy example: {:?}", policy);

    let policy_toml = toml::to_string(&policy).unwrap();
    println!("Policy serialized: {}", policy_toml);

    let policy2: Policy = toml::from_str(&policy_toml).unwrap();
    println!("Policy deserialized again: {:?}", policy2);

    assert_eq!(policy, policy2);
}
