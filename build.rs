fn main() {
    windows_reactor_setup::as_self_contained();
    if std::env::var_os("CARGO_FEATURE_GPUI_PREVIEW").is_some() {
        // GPUI imports TaskDialogIndirect from version 6 of the common controls.
        println!(
            "cargo:rustc-link-arg-bins=/MANIFESTINPUT:{}/assets/gpui.manifest",
            env!("CARGO_MANIFEST_DIR")
        );
        println!("cargo:rerun-if-changed=assets/gpui.manifest");
    }
    winresource::WindowsResource::new()
        .set_icon("assets/QuickPeek.ico")
        .set("ProductName", "QuickPeek")
        .set("FileDescription", "QuickPeek native file preview")
        .compile()
        .expect("compile application icon");
    println!("cargo:rerun-if-changed=assets/QuickPeek.ico");
    println!("cargo:rerun-if-changed=build.rs");
}
