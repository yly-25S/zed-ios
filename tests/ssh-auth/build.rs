use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=ZED_IOS_SOURCE_DIR");
    let source = PathBuf::from(
        env::var_os("ZED_IOS_SOURCE_DIR")
            .expect("Set ZED_IOS_SOURCE_DIR to the prepared upstream source"),
    )
    .join("crates/remote/src/transport/russh_auth.rs")
    .canonicalize()
    .expect("Prepare the upstream authentication patch first");
    println!("cargo:rerun-if-changed={}", source.display());
    let module = format!("#[path = {:?}]\npub mod authentication;\n", source);
    fs::write(
        PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("authentication.rs"),
        module,
    )
    .unwrap();
}
