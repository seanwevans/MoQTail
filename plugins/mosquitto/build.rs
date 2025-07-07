fn main() {
    println!("cargo:rerun-if-changed=src/dummy.c");
    cc::Build::new()
        .file("src/dummy.c")
        .compile("moqtail_mosquitto_dummy");
}
