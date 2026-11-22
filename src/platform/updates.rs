use base64::Engine;
use std::{
    os::windows::process::CommandExt,
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
};
use windows_registry::CURRENT_USER;

const SETTINGS: &str = r"Software\Nex-Z\QuickPeek";
const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
static CHECKING: AtomicBool = AtomicBool::new(false);

pub fn automatic() -> bool {
    CURRENT_USER
        .open(SETTINGS)
        .and_then(|k| k.get_u32("AutoUpdate"))
        .unwrap_or(0)
        == 1
}
pub fn toggle_auto() -> Result<(), String> {
    CURRENT_USER
        .create(SETTINGS)
        .and_then(|k| k.set_u32("AutoUpdate", u32::from(!automatic())))
        .map_err(|e| e.to_string())
}
fn startup_command() -> Result<String, String> {
    Ok(format!(
        "\"{}\"",
        std::env::current_exe()
            .map_err(|e| e.to_string())?
            .display()
    ))
}
pub fn startup() -> bool {
    CURRENT_USER
        .open(RUN)
        .and_then(|k| k.get_string("QuickPeek"))
        .ok()
        .zip(startup_command().ok())
        .is_some_and(|(value, cmd)| value.eq_ignore_ascii_case(&cmd))
}
pub fn toggle_startup() -> Result<(), String> {
    let key = CURRENT_USER.create(RUN).map_err(|e| e.to_string())?;
    if startup() {
        key.remove_value("QuickPeek")
    } else {
        key.set_string("QuickPeek", startup_command()?)
    }
    .map_err(|e| e.to_string())
}
pub fn check(manual: bool) {
    if CHECKING.swap(true, Ordering::AcqRel) {
        if manual {
            super::show_error("正在检查或下载更新。");
        }
        return;
    }
    std::thread::spawn(move || {
        let run = || -> std::io::Result<()> {
            let exe = std::env::current_exe()?;
            let script: Vec<u8> = include_str!("../../scripts/update.ps1")
                .trim_start_matches('\u{feff}')
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect();
            let powershell = std::path::PathBuf::from(
                std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into()),
            )
            .join("System32/WindowsPowerShell/v1.0/powershell.exe");
            Command::new(powershell)
                .creation_flags(0x08000000)
                .args(["-NoProfile", "-NonInteractive", "-EncodedCommand"])
                .arg(base64::engine::general_purpose::STANDARD.encode(script))
                .env("QUICKPEEK_UPDATE_EXE", exe)
                .env("QUICKPEEK_UPDATE_PID", std::process::id().to_string())
                .env("QUICKPEEK_UPDATE_VERSION", env!("CARGO_PKG_VERSION"))
                .env("QUICKPEEK_UPDATE_MANUAL", if manual { "1" } else { "0" })
                .status()?;
            Ok(())
        };
        if let Err(e) = run()
            && manual
        {
            super::show_error(&e.to_string());
        }
        CHECKING.store(false, Ordering::Release);
    });
}
