use std::path::Path;
use windows::{
    Win32::{
        Foundation::*,
        Media::MediaFoundation::*,
        System::Com::StructuredStorage::PROPVARIANT,
        UI::{HiDpi::GetDpiForWindow, WindowsAndMessaging::*},
    },
    core::{HSTRING, w},
};

pub struct Player {
    player: IMFPMediaPlayer,
    hwnd: HWND,
    parent: HWND,
    pub paused: bool,
}
impl Player {
    pub fn open(path: &Path, parent: HWND) -> Result<Self, String> {
        (|| -> windows::core::Result<Self> {
            unsafe {
                let hwnd = CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    w!("STATIC"),
                    w!(""),
                    WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS,
                    12,
                    100,
                    800,
                    500,
                    Some(parent),
                    None,
                    None,
                    None,
                )?;
                let mut player = None;
                if let Err(e) = MFPCreateMediaPlayer(
                    &HSTRING::from(path.as_os_str()),
                    true,
                    MFP_CREATION_OPTIONS(0),
                    None,
                    Some(hwnd),
                    Some(&mut player),
                ) {
                    let _ = DestroyWindow(hwnd);
                    return Err(e);
                }
                let Some(player) = player else {
                    let _ = DestroyWindow(hwnd);
                    return Err(windows::core::Error::from_thread());
                };
                let result = Self {
                    player,
                    hwnd,
                    parent,
                    paused: false,
                };
                result.resize(false);
                Ok(result)
            }
        })()
        .map_err(|e| {
            format!("音视频无法打开，文件可能损坏或缺少系统解码器：{e}；按 Enter 使用默认应用打开")
        })
    }
    pub fn resize(&self, inspector: bool) {
        unsafe {
            let mut r = RECT::default();
            if GetClientRect(self.parent, &mut r).is_err() {
                return;
            }
            let dpi = GetDpiForWindow(self.parent) as f64 / 96.0;
            let pad = 0;
            let top = (32.0 * dpi) as i32;
            let bottom = (88.0 * dpi) as i32;
            let _ = SetWindowPos(
                self.hwnd,
                Some(HWND_TOP),
                pad,
                top,
                (r.right - pad * 2 - if inspector { (265.0 * dpi) as i32 } else { 0 }).max(1),
                (r.bottom - top - bottom).max(1),
                SWP_NOACTIVATE,
            );
            let _ = self.player.UpdateVideo();
        }
    }
    pub fn toggle(&mut self) -> Result<(), String> {
        unsafe {
            let r = if self.paused {
                self.player.Play()
            } else {
                self.player.Pause()
            };
            r.map_err(|e| e.to_string())?;
            self.paused = !self.paused;
            Ok(())
        }
    }
    pub fn progress(&self) -> (f64, f64) {
        unsafe {
            let position = self
                .player
                .GetPosition(&MFP_POSITIONTYPE_100NS)
                .ok()
                .and_then(|v| u64::try_from(&v).ok())
                .unwrap_or(0);
            let duration = self
                .player
                .GetDuration(&MFP_POSITIONTYPE_100NS)
                .ok()
                .and_then(|v| u64::try_from(&v).ok())
                .unwrap_or(0);
            (
                position as f64 / 10_000_000.0,
                duration as f64 / 10_000_000.0,
            )
        }
    }
    pub fn seek(&self, seconds: f64) -> Result<(), String> {
        unsafe {
            self.player
                .SetPosition(
                    &MFP_POSITIONTYPE_100NS,
                    // MFPlay requires VT_I8, not VT_UI8, for seek positions.
                    &PROPVARIANT::from((seconds.max(0.0) * 10_000_000.0) as i64),
                )
                .map_err(|e| e.to_string())
        }
    }
    pub fn volume(&self, value: f64) {
        unsafe {
            let _ = self.player.SetVolume(value.clamp(0.0, 1.0) as f32);
        }
    }
    pub fn rate(&self, value: f32) -> Result<(), String> {
        unsafe { self.player.SetRate(value).map_err(|e| e.to_string()) }
    }
}
impl Drop for Player {
    fn drop(&mut self) {
        unsafe {
            let _ = self.player.Shutdown();
            let _ = DestroyWindow(self.hwnd);
        }
    }
}
