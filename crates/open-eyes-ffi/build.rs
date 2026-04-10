fn main() {
    // Generate C header from the Rust FFI types
    let crate_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let config = cbindgen::Config::from_file(format!("{}/cbindgen.toml", crate_dir))
        .expect("Failed to read cbindgen.toml");

    cbindgen::Builder::new()
        .with_crate(&crate_dir)
        .with_config(config)
        .generate()
        .expect("Failed to generate C bindings")
        .write_to_file(format!("{}/../../bindings/openeyes.h", crate_dir));
}
