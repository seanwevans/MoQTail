use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=src/dummy.c");
    cc::Build::new()
        .file("src/dummy.c")
        .compile("moqtail_mosquitto_dummy");

    println!("cargo:rerun-if-changed=src/bindings.rs");
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    fs::copy("src/bindings.rs", out_path.join("bindings.rs")).expect("could not copy bindings");
}
