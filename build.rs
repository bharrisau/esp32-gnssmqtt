fn main() {
    embuild::espidf::sysenv::output();

    // ESP-IDF cmake resolves CONFIG_PARTITION_TABLE_CUSTOM_FILENAME relative to its
    // own cmake project directory (target/.../build/esp-idf-sys-<hash>/out/), not the
    // Cargo project root. On Windows, embuild cannot create symlinks without Developer
    // Mode, so we copy the file manually on every build.
    //
    // NOTE: On a clean build the first `cargo build` may still fail because esp-idf-sys
    // cmake runs before this script. In that case, run `cargo build` a second time —
    // this script will have copied the file on the first run, allowing the second to
    // succeed. After the initial setup subsequent builds work without manual steps.
    println!("cargo:rerun-if-changed=partitions.csv");

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR must be set");
    // OUT_DIR = target/<triple>/debug/build/esp32-gnssmqtt-<hash>/out
    // We want  target/<triple>/debug/build/
    let build_dir = std::path::Path::new(&out_dir)
        .parent()
        .and_then(|p| p.parent())
        .expect("could not find build dir");

    if let Ok(entries) = std::fs::read_dir(build_dir) {
        for entry in entries.flatten() {
            if entry
                .file_name()
                .to_string_lossy()
                .starts_with("esp-idf-sys-")
            {
                let dest = entry.path().join("out").join("partitions.csv");
                if entry.path().join("out").is_dir() {
                    let _ = std::fs::copy("partitions.csv", &dest);
                }
            }
        }
    }
}
