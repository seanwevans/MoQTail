fn main() {
    println!("cargo:rerun-if-changed=src/dummy.c");
    cc::Build::new()
        .file("src/dummy.c")
        .compile("moqtail_mosquitto_dummy");

    println!("cargo:rerun-if-changed=wrapper.h");

    let bindings = bindgen::Builder::default()
        .header("wrapper.h")
        .allowlist_type("mosquitto_opt")
        .allowlist_type("mosquitto_evt_message")
        .allowlist_function("mosquitto_callback_register")
        .allowlist_function("mosquitto_callback_unregister")
        .generate()
        .expect("failed to generate bindings");

    let out_path = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("could not write bindings");
}
