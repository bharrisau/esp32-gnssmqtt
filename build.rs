fn main() {
    embuild::espidf::sysenv::output();

    // Compile the C vprintf hook shim for log interception.
    // ESP-IDF headers (esp_log.h etc.) are found via include paths from embuild cincl_args.
    if let Some(cincl) = embuild::espidf::sysenv::cincl_args() {
        let mut build = cc::Build::new();
        build.file("src/log_shim.c");

        // cincl.args is a shell-style string of compiler flags.
        // Each flag is either bare (no spaces) or wrapped in "..." with internal \" escapes.
        // Parse each token: strip outer quotes and unescape internal \" sequences.
        for raw in cincl.args.split_whitespace() {
            let flag = if raw.starts_with('"') && raw.ends_with('"') && raw.len() >= 2 {
                // Strip outer double-quotes and unescape internal \"
                raw[1..raw.len() - 1].replace("\\\"", "\"")
            } else {
                raw.to_owned()
            };
            if let Some(path) = flag.strip_prefix("-isystem") {
                build.include(path);
            } else if let Some(path) = flag.strip_prefix("-I") {
                build.include(path);
            } else if flag.starts_with("-D") {
                // Pass defines directly as flags
                build.flag(&flag);
            }
        }

        build.compile("log_shim");
    }
}
