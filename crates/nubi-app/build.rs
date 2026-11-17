fn main() {
    println!("cargo:rerun-if-changed=nubi.manifest");
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        let manifest = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("nubi.manifest");
        println!("cargo:rustc-link-arg-bin=nubi=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg-bin=nubi=/MANIFESTINPUT:{}", manifest.display());
    }
    let built = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("build clock")
        .as_secs();
    println!("cargo:rustc-env=NUBI_BUILD_TIME={built}");
    slint_build::compile("ui/notch.slint").expect("compile native UI");
}
