fn main() {
    // Same compile-time source stamp the CLI uses (clients/cli/package.nix).
    println!("cargo:rerun-if-env-changed=KORRI_BUILD_ID");
    slint_build::compile("ui/inspector.slint").expect("compile ui/inspector.slint");
}
