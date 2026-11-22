use std::cell::RefCell;
use windows::Win32::{
    Foundation::*,
    UI::{
        HiDpi::{GetDpiForSystem, GetDpiForWindow},
        Shell::*,
        WindowsAndMessaging::*,
    },
};
pub const LIGHT_BACKGROUND: u32 = 0xf2f2f8;
pub const HEIGHT: f32 = 32.;
pub const BUTTON_WIDTH: f32 = 46.;
pub const ICON_SIZE: f32 = 20.;
pub const ICON_LEFT: f32 = 16.;
pub const TEXT_LEFT: f32 = 46.;
pub const TEXT_SIZE: f32 = 12.;

const ID: usize = 0x5150;
thread_local! { static CHANGED: RefCell<Option<Box<dyn Fn()>>> = const { RefCell::new(None) }; }

// WinUI TitleBar reserves 48 DIPs for dragging and currently treats the physical
// RightInset (three 46-DIP caption buttons) as DIPs. Compensate for both so our
// right-header button sits directly before the system caption buttons.
// See microsoft-ui-xaml/controls/dev/TitleBar/TitleBar.cpp UpdatePadding.
pub fn right_compensation() -> f64 {
    let dpi = unsafe {
        super::current_window()
            .map(|h| GetDpiForWindow(h))
            .unwrap_or_else(|| GetDpiForSystem())
    };
    48.0 + f64::from(3. * BUTTON_WIDTH) * (dpi as f64 / 96.0 - 1.0)
}
pub fn install(changed: impl Fn() + 'static) -> Result<(), String> {
    let hwnd = super::current_window().ok_or("预览窗口未就绪")?;
    CHANGED.with(|c| *c.borrow_mut() = Some(Box::new(changed)));
    if unsafe { SetWindowSubclass(hwnd, Some(procedure), ID, 0) }.as_bool() {
        Ok(())
    } else {
        Err("无法监听窗口 DPI 变化".into())
    }
}
unsafe extern "system" fn procedure(
    hwnd: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
    _: usize,
    _: usize,
) -> LRESULT {
    unsafe {
        let result = DefSubclassProc(hwnd, msg, w, l);
        if msg == WM_DPICHANGED || msg == WM_SIZE {
            CHANGED.with(|c| {
                if let Some(c) = c.borrow().as_ref() {
                    c();
                }
            });
        }
        if msg == WM_NCDESTROY {
            let _ = RemoveWindowSubclass(hwnd, Some(procedure), ID);
            CHANGED.with(|c| {
                c.borrow_mut().take();
            });
        }
        result
    }
}
