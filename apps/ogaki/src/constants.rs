pub const OS: &str = {
    // In Rust [`std::env::consts::OS`] for MacOs is `macos` but we need
    // `darwin` as it's set so in the release CI.
    if cfg!(target_os = "macos") {
        "darwin"
    } else {
        std::env::consts::OS
    }
};
