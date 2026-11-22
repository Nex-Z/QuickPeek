use super::*;
static IMAGE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub fn image_zoom(active: bool) {
    IMAGE.store(active, Ordering::Relaxed);
}
type Callback = Option<Box<dyn Fn(f64)>>;
thread_local! { static CALLBACK: std::cell::RefCell<Callback> = const { std::cell::RefCell::new(None) }; }
pub struct WheelGuard(HHOOK);

pub fn install(callback: impl Fn(f64) + 'static) -> Result<WheelGuard, String> {
    unsafe extern "system" fn hook(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
        unsafe {
            if code == HC_ACTION as i32
                && w.0 == WM_MOUSEWHEEL as usize
                && (IMAGE.load(Ordering::Relaxed) || GetAsyncKeyState(VK_CONTROL.0 as i32) < 0)
                && !PDF_ACTIVE.load(Ordering::Relaxed)
                && !READER_ACTIVE.load(Ordering::Relaxed)
            {
                let event = &*(l.0 as *const MSLLHOOKSTRUCT);
                let hwnd = GetAncestor(WindowFromPoint(event.pt), GA_ROOT);
                let mut pid = 0;
                GetWindowThreadProcessId(hwnd, Some(&mut pid));
                if pid == GetCurrentProcessId() && hwnd == GetForegroundWindow() {
                    CALLBACK.with(|slot| {
                        if let Some(callback) = slot.borrow().as_ref() {
                            callback((event.mouseData >> 16) as i16 as f64 / 1200.0);
                        }
                    });
                    return LRESULT(1);
                }
            }
            CallNextHookEx(None, code, w, l)
        }
    }
    let hook = unsafe {
        SetWindowsHookExW(
            WH_MOUSE_LL,
            Some(hook),
            Some(GetModuleHandleW(None).map_err(|e| e.to_string())?.into()),
            0,
        )
    }
    .map_err(|e| e.to_string())?;
    CALLBACK.with(|slot| *slot.borrow_mut() = Some(Box::new(callback)));
    Ok(WheelGuard(hook))
}
impl Drop for WheelGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = UnhookWindowsHookEx(self.0);
        }
        CALLBACK.with(|slot| {
            slot.borrow_mut().take();
        });
    }
}
