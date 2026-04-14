use std::process::Command;

fn main() {
    // Re-run when HEAD changes so the embedded git sha stays fresh. A missing
    // .git (e.g. cargo install from a crates.io tarball) is not fatal — we
    // simply emit an empty sha.
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/refs");

    let sha = Command::new("git")
        .args(["rev-parse", "--short=7", "HEAD"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout).ok()
            } else {
                None
            }
        })
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    println!("cargo:rustc-env=RUSTLET_GIT_SHA={sha}");
}
