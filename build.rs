fn build_date_utc() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Howard Hinnant's civil-from-days algorithm.
    let z = (secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

fn git_short_hash() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_default()
}

fn main() {
    println!("cargo:rustc-env=QUICKPEEK_BUILD_DATE={}", build_date_utc());
    println!("cargo:rustc-env=QUICKPEEK_GIT_HASH={}", git_short_hash());
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
