#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use quickpeek::core;
#[cfg(feature = "gpui-preview")]
mod gpui_preview;
mod platform;
mod ui;

fn main() {
    // Set before starting threads; XAML WebView2 otherwise writes its profile
    // beside the executable, which may be in a read-only portable directory.
    if std::env::var_os("WEBVIEW2_USER_DATA_FOLDER").is_none() {
        let folder = std::env::var_os("LOCALAPPDATA")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("QuickPeek")
            .join("WebView2");
        unsafe {
            std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", folder);
        }
    }
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    #[cfg(feature = "gpui-preview")]
    if args.first().is_some_and(|s| s == "--standby") {
        gpui_preview::standby();
        return;
    }
    if args.first().is_some_and(|s| s == "--archive-member") {
        if let (Some(archive), Some(name)) = (args.get(1), args.get(2)) {
            let _monitor = platform::FocusMonitor::start(false);
            let result =
                quickpeek::archive::member(std::path::Path::new(archive), &name.to_string_lossy())
                    .and_then(|(_directory, path)| ui::run(Some(path)));
            if let Err(error) = result {
                platform::show_error(&error);
            }
        }
        return;
    }
    let classic = args.first().is_some_and(|s| s == "--classic");
    let preview = args.first().is_some_and(|s| s == "--preview");
    let path = if classic || preview {
        args.get(1)
    } else {
        args.first()
    }
    .map(std::path::PathBuf::from);
    #[cfg(feature = "gpui-preview")]
    if !classic && let Some(path) = path.as_ref().filter(|p| gpui_preview::supports(p)) {
        if let Err(error) = gpui_preview::run(path.clone()) {
            platform::show_error(&error);
        }
        return;
    }
    let _monitor = platform::FocusMonitor::start(args.is_empty());
    let result = if args.is_empty() {
        platform::run_broker()
    } else {
        ui::run(path)
    };
    if let Err(error) = result {
        platform::show_error(&error);
    }
    platform::trace("main completed");
}
