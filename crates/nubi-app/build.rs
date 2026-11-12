fn main() {
    let built = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("build clock")
        .as_secs();
    println!("cargo:rustc-env=NUBI_BUILD_TIME={built}");
    slint_build::compile("ui/notch.slint").expect("compile native UI");
}
