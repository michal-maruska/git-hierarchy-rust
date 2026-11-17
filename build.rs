use std::os::unix::fs::symlink;

fn main() {
    println!("cargo:rerun-if-changed=src/bin/git-walk-down.rs");

    // The walk-down binary is multi-call: under the name
    // git-delete-down, the walk consumes (deletes) the tree.  Instead
    // of compiling the source twice, build a second NAME for the same
    // binary -- the link dangles until the compiler writes the real
    // artifact, and resolves as soon as the build ends.  With the
    // target dir on the PATH, this link IS the install; only a
    // `cargo install' user would need a manual
    //     ln -sf git-walk-down ~/.cargo/bin/git-delete-down
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
    let profile = std::env::var("PROFILE").expect("PROFILE not set");

    // OUT_DIR's depth is cargo-version dependent (modern:
    // <target>/<profile>/build/<pkg>-<hash>/out, legacy:
    // <target>/build/<pkg>-<hash>/out), so find the profile dir by
    // its name while walking up, and fall back to the arithmetic for
    // the legacy layout:
    let out_dir = std::path::Path::new(&out_dir);
    let bin_dir = out_dir
        .ancestors()
        .find(|p| p.file_name().is_some_and(|n| n == std::ffi::OsStr::new(&profile)))
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|| {
            out_dir
                .ancestors()
                .nth(3)
                .expect("OUT_DIR below the target dir")
                .join(&profile)
        });
    let link = bin_dir.join("git-delete-down");

    let _ = std::fs::remove_file(&link);
    if let Err(e) = symlink("git-walk-down", &link) {
        // e.g. a filesystem without symlink support -- the walk still
        // works, only under its original name.
        println!("cargo:warning=could not create the git-delete-down link: {e}");
    }
}
