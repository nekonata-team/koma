fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    for (key, default) in [
        ("MAKEPAD_BUNDLE_NAME", "Koma"),
        ("MAKEPAD_BUNDLE_IDENTIFIER", "dev.nekonata.koma"),
    ] {
        println!("cargo:rerun-if-env-changed={key}");
        if std::env::var_os(key).is_none() {
            println!("cargo:rustc-env={key}={default}");
        }
    }
}
