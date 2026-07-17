fn main() {
    // Optional header generation: cargo build -p recall-ffi --features generate-header
    #[cfg(feature = "generate-header")]
    {
        let crate_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let config = cbindgen::Config::from_file(format!("{crate_dir}/cbindgen.toml"))
            .expect("cbindgen.toml");
        cbindgen::Builder::new()
            .with_crate(&crate_dir)
            .with_config(config)
            .generate()
            .expect("cbindgen")
            .write_to_file(format!("{crate_dir}/include/recall.h"));
    }
}
