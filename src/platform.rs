use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicIsize, Ordering},
};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::{Com::*, LibraryLoader::GetModuleHandleW, Threading::GetCurrentProcessId},
        UI::{
            Accessibility::*, HiDpi::*, Input::KeyboardAndMouse::*, Shell::*,
            WindowsAndMessaging::*,
        },
    },
    core::{HSTRING, Interface, w},
};

pub mod caption;
pub mod media;
pub mod pdf;
mod updates;
pub mod wheel;

static PDF_ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static READER_ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub fn reader_active(active: bool) {
    READER_ACTIVE.store(active, Ordering::Relaxed);
}
pub fn content_height() -> f64 {
    unsafe {
        if let Some(hwnd) = current_window() {
            let mut rect = RECT::default();
            if GetClientRect(hwnd, &mut rect).is_ok() {
                return (rect.bottom as f64 / (GetDpiForWindow(hwnd) as f64 / 96.0) - 32.0)
                    .max(1.0);
            }
        }
    }
    660.0
}
static PAGED_TEXT_ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub fn pdf_active(active: bool) {
    PDF_ACTIVE.store(active, Ordering::Relaxed);
}

pub fn paged_text_active(active: bool) {
    PAGED_TEXT_ACTIVE.store(active, Ordering::Relaxed);
}

pub fn content_width(inspector: bool) -> f64 {
    unsafe {
        if let Some(hwnd) = current_window() {
            let mut rect = RECT::default();
            if GetClientRect(hwnd, &mut rect).is_ok() {
                let dpi = GetDpiForWindow(hwnd) as f64 / 96.0;
                return (rect.right as f64 / dpi - if inspector { 265.0 } else { 0.0 }).max(1.0);
            }
        }
    }
    960.0
}

pub fn trace(message: &str) {
    if let Some(path) = std::env::var_os("QUICKPEEK_TRACE") {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis();
            let _ = writeln!(f, "{stamp} {} {message}", std::process::id());
        }
    }
}

static EDITABLE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
static FOCUS_WINDOW: AtomicIsize = AtomicIsize::new(0);
pub struct FocusMonitor(std::sync::Arc<std::sync::atomic::AtomicBool>);
impl FocusMonitor {
    pub fn start(explorer: bool) -> Self {
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let cancelled = stop.clone();
        std::thread::spawn(move || unsafe {
            if CoInitializeEx(None, COINIT_MULTITHREADED).ok().is_err() {
                return;
            }
            let automation: Option<IUIAutomation> =
                CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok();
            while !cancelled.load(Ordering::Relaxed) {
                let hwnd = GetForegroundWindow();
                let mut owner = 0;
                GetWindowThreadProcessId(hwnd, Some(&mut owner));
                let mut class = [0u16; 128];
                let len = GetClassNameW(hwnd, &mut class);
                if if explorer {
                    String::from_utf16_lossy(&class[..len as usize]) != "CabinetWClass"
                } else {
                    owner != GetCurrentProcessId()
                } {
                    FOCUS_WINDOW.store(0, Ordering::Release);
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    continue;
                }
                let editable = focused_editable(automation.as_ref());
                if GetForegroundWindow() == hwnd {
                    EDITABLE.store(editable, Ordering::Release);
                    FOCUS_WINDOW.store(hwnd.0 as isize, Ordering::Release);
                }
                std::thread::sleep(std::time::Duration::from_millis(40));
            }
            drop(automation);
            CoUninitialize();
        });
        Self(stop)
    }
}
fn focused_editable(automation: Option<&IUIAutomation>) -> bool {
    unsafe {
        automation
            .and_then(|a| a.GetFocusedElement().ok())
            .and_then(|e| {
                let t = e.CurrentControlType().ok()?;
                let class = e.CurrentClassName().ok()?.to_string().to_ascii_lowercase();
                Some(
                    t == UIA_EditControlTypeId
                        || t == UIA_ComboBoxControlTypeId
                        || class.contains("rename")
                        || class.contains("edit"),
                )
            })
            .unwrap_or(true)
    }
}
impl Drop for FocusMonitor {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}
fn is_editing(hwnd: HWND) -> bool {
    unsafe {
        let thread = GetWindowThreadProcessId(hwnd, None);
        let mut info = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        if GetGUIThreadInfo(thread, &mut info).is_ok() {
            let mut name = [0u16; 128];
            let len = GetClassNameW(info.hwndFocus, &mut name);
            let class = String::from_utf16_lossy(&name[..len as usize]).to_ascii_lowercase();
            if class.contains("edit") {
                return true;
            }
        }
        FOCUS_WINDOW.load(Ordering::Acquire) != hwnd.0 as isize || EDITABLE.load(Ordering::Acquire)
    }
}

// MAKEINTRESOURCEW(1), a resource identifier rather than a dereferenced pointer.
#[allow(clippy::manual_dangling_ptr)]
fn app_icon() -> Option<HICON> {
    unsafe {
        let instance = GetModuleHandleW(None).ok()?;
        LoadIconW(
            Some(instance.into()),
            windows::core::PCWSTR(1usize as *const u16),
        )
        .ok()
    }
}
static PREVIEW_WORK_AREA: std::sync::Mutex<Option<RECT>> = std::sync::Mutex::new(None);

pub fn prepare_preview() {
    unsafe {
        // WinUI enables DPI awareness later; capture physical coordinates now.
        let previous = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let monitor = MonitorFromWindow(GetForegroundWindow(), MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(monitor, &mut info).as_bool() {
            trace(&format!("work area {:?}", info.rcWork));
            *PREVIEW_WORK_AREA.lock().unwrap() = Some(info.rcWork);
        }
        SetThreadDpiAwarenessContext(previous);
    }
}
fn fit_preview(area: RECT, width: i32, height: i32) -> (i32, i32, i32, i32) {
    let w = width.min(area.right - area.left);
    let h = height.min(area.bottom - area.top);
    (
        area.left + (area.right - area.left - w) / 2,
        area.top + (area.bottom - area.top - h) / 2,
        w,
        h,
    )
}

#[cfg(feature = "gpui-preview")]
pub fn drag_caption() {
    unsafe {
        if let Some(hwnd) = current_window() {
            let mut at = POINT::default();
            if GetCursorPos(&mut at).is_ok() {
                let position = (at.x as u16 as u32) | ((at.y as u16 as u32) << 16);
                let _ = windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture();
                let _ = PostMessageW(
                    Some(hwnd),
                    WM_NCLBUTTONDOWN,
                    WPARAM(HTCAPTION as usize),
                    LPARAM(position as isize),
                );
            }
        }
    }
}
#[cfg(feature = "gpui-preview")]
pub fn classic_menu() -> bool {
    unsafe {
        let Some(hwnd) = current_window() else {
            return false;
        };
        let Ok(menu) = CreatePopupMenu() else {
            return false;
        };
        let mut point = POINT::default();
        let _ = GetCursorPos(&mut point);
        let _ = AppendMenuW(menu, MF_STRING, 1, w!("完整预览\tEnter"));
        let choice = TrackPopupMenu(menu, TPM_RETURNCMD, point.x, point.y, Some(0), hwnd, None);
        let _ = DestroyMenu(menu);
        choice.0 == 1
    }
}
#[cfg(feature = "gpui-preview")]
pub fn grant_foreground(pid: u32) {
    unsafe {
        let _ = AllowSetForegroundWindow(pid);
    }
}
pub fn activate_preview() {
    activate_window(false);
}
#[cfg(feature = "gpui-preview")]
pub fn present_reader() {
    activate_window(true);
}
fn activate_window(skip_animation: bool) {
    unsafe {
        let previous = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        if let Some(hwnd) = current_window() {
            use windows::Win32::Graphics::Dwm::{
                DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE, DWMWA_TRANSITIONS_FORCEDISABLED,
                DwmSetWindowAttribute,
            };
            if skip_animation {
                let disable = windows::core::BOOL(1);
                let _ = DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_TRANSITIONS_FORCEDISABLED,
                    (&raw const disable).cast(),
                    size_of_val(&disable) as u32,
                );
            }
            let border = DWMWA_COLOR_NONE;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_BORDER_COLOR,
                (&raw const border).cast(),
                size_of_val(&border) as u32,
            );
            let mut rect = RECT::default();
            let area = *PREVIEW_WORK_AREA.lock().unwrap();
            if let Some(area) = area
                && GetWindowRect(hwnd, &mut rect).is_ok()
            {
                trace(&format!("initial bounds {rect:?}"));
                let (x, y, width, height) =
                    fit_preview(area, rect.right - rect.left, rect.bottom - rect.top);
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_NOTOPMOST),
                    x,
                    y,
                    width,
                    height,
                    SWP_FRAMECHANGED | SWP_SHOWWINDOW,
                );
            } else {
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_NOTOPMOST),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_FRAMECHANGED | SWP_SHOWWINDOW,
                );
            }
            let foreground_thread = GetWindowThreadProcessId(GetForegroundWindow(), None);
            let thread = windows::Win32::System::Threading::GetCurrentThreadId();
            let attached = foreground_thread != 0
                && foreground_thread != thread
                && windows::Win32::System::Threading::AttachThreadInput(
                    thread,
                    foreground_thread,
                    true,
                )
                .as_bool();
            let _ = ShowWindow(hwnd, SW_SHOWNORMAL);
            let _ = SetForegroundWindow(hwnd);
            // Moving between monitors can synchronously resize WinUI via
            // WM_DPICHANGED. Center using the resulting physical size.
            if let Some(area) = area
                && GetWindowRect(hwnd, &mut rect).is_ok()
            {
                let (x, y, w, h) =
                    fit_preview(area, rect.right - rect.left, rect.bottom - rect.top);
                let _ = SetWindowPos(hwnd, Some(HWND_TOP), x, y, w, h, SWP_SHOWWINDOW);
            }
            let mut final_rect = RECT::default();
            let _ = GetWindowRect(hwnd, &mut final_rect);
            trace(&format!("activated bounds {final_rect:?}"));
            if attached {
                let _ = windows::Win32::System::Threading::AttachThreadInput(
                    thread,
                    foreground_thread,
                    false,
                );
            }
        }
        SetThreadDpiAwarenessContext(previous);
    }
}
pub fn pin_preview(pinned: bool) {
    unsafe {
        if let Some(hwnd) = current_window() {
            let _ = SetWindowPos(
                hwnd,
                Some(if pinned { HWND_TOPMOST } else { HWND_NOTOPMOST }),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }
}
pub fn set_window_icon() {
    unsafe {
        if let (Some(hwnd), Some(icon)) = (current_window(), app_icon()) {
            for size in [0, 1] {
                SendMessageW(
                    hwnd,
                    WM_SETICON,
                    Some(WPARAM(size)),
                    Some(LPARAM(icon.0 as isize)),
                );
            }
        }
    }
}
pub fn toggle_maximize() {
    unsafe {
        if let Some(hwnd) = current_window() {
            let _ = ShowWindow(
                hwnd,
                if IsZoomed(hwnd).as_bool() {
                    SW_RESTORE
                } else {
                    SW_MAXIMIZE
                },
            );
        }
    }
}
type KeyCallback = Option<Box<dyn Fn(u32)>>;
thread_local! { static KEY_CALLBACK: std::cell::RefCell<KeyCallback> = const { std::cell::RefCell::new(None) }; }
thread_local! { static KEY_DOWN: std::cell::RefCell<[bool;256]> = const { std::cell::RefCell::new([false;256]) }; }
pub struct KeyboardGuard(HHOOK);
impl Drop for KeyboardGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = UnhookWindowsHookEx(self.0);
        }
        KEY_CALLBACK.with(|c| {
            c.borrow_mut().take();
        });
    }
}
pub fn install_keys(callback: impl Fn(u32) + 'static) -> Result<KeyboardGuard, String> {
    KEY_DOWN.with(|down| unsafe {
        down.borrow_mut()[32] = GetAsyncKeyState(VK_SPACE.0 as i32) < 0;
    });
    unsafe extern "system" fn hook(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
        unsafe {
            if code == HC_ACTION as i32 && w.0 == WM_KEYUP as usize {
                let key = (*(l.0 as *const KBDLLHOOKSTRUCT)).vkCode as usize;
                if key < 256 {
                    KEY_DOWN.with(|down| down.borrow_mut()[key] = false);
                }
            }
            if code == HC_ACTION as i32 && w.0 == WM_KEYDOWN as usize {
                let event = &*(l.0 as *const KBDLLHOOKSTRUCT);
                if READER_ACTIVE.load(Ordering::Relaxed)
                    && matches!(event.vkCode, 187 | 189 | 107 | 109)
                {
                    return CallNextHookEx(None, code, w, l);
                }
                if matches!(event.vkCode, 33 | 34) && !PAGED_TEXT_ACTIVE.load(Ordering::Relaxed) {
                    return CallNextHookEx(None, code, w, l);
                }
                if PDF_ACTIVE.load(Ordering::Relaxed)
                    && (matches!(
                        event.vkCode,
                        13 | 32 | 33 | 34 | 37 | 39 | 187 | 189 | 107 | 109
                    ) || (matches!(event.vkCode, 70 | 82)
                        && GetAsyncKeyState(VK_CONTROL.0 as i32) < 0))
                {
                    return CallNextHookEx(None, code, w, l);
                }
                let mut pid = 0;
                GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut pid));
                let shortcut = GetAsyncKeyState(VK_CONTROL.0 as i32) < 0
                    && [70, 73, 82, 79, 76].contains(&event.vkCode);
                if pid == GetCurrentProcessId()
                    && (shortcut
                        || [27, 32, 33, 34, 37, 39, 13, 187, 189, 107, 109].contains(&event.vkCode))
                {
                    let editing = is_editing(GetForegroundWindow());
                    trace(&format!("key {} editing={editing}", event.vkCode));
                    if shortcut || event.vkCode == 27 || !editing {
                        let repeat = KEY_DOWN.with(|down| {
                            let mut down = down.borrow_mut();
                            let old = down[event.vkCode as usize];
                            down[event.vkCode as usize] = true;
                            old
                        });
                        if !repeat {
                            KEY_CALLBACK.with(|c| {
                                if let Some(f) = c.borrow().as_ref() {
                                    f(event.vkCode | if shortcut { 0x10000 } else { 0 });
                                }
                            });
                        }
                        return LRESULT(1);
                    }
                }
            }
            CallNextHookEx(None, code, w, l)
        }
    }
    KEY_CALLBACK.with(|c| *c.borrow_mut() = Some(Box::new(callback)));
    unsafe {
        SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(hook),
            Some(GetModuleHandleW(None).map_err(|e| e.to_string())?.into()),
            0,
        )
        .map(KeyboardGuard)
        .map_err(|e| e.to_string())
    }
}

static BROKER: AtomicIsize = AtomicIsize::new(0);
static SPACE_DOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
const OPEN_SELECTION: u32 = WM_APP + 1;
const TRAY: u32 = WM_APP + 2;
thread_local! { static REQUESTS: std::cell::RefCell<Option<std::sync::mpsc::Sender<isize>>> = const { std::cell::RefCell::new(None) }; }

pub fn show_error(error: &str) {
    unsafe {
        MessageBoxW(
            None,
            &HSTRING::from(error),
            w!("QuickPeek"),
            MB_OK | MB_ICONERROR,
        );
    }
}

pub fn open_external(path: &Path) {
    unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            &HSTRING::from(path.as_os_str()),
            None,
            None,
            SW_SHOWNORMAL,
        );
    }
}
pub fn reveal(path: &Path) {
    let _ = Command::new("explorer.exe")
        .arg(format!("/select,{}", path.display()))
        .spawn();
}

pub fn pick_file() -> Option<PathBuf> {
    unsafe {
        let dialog: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        dialog
            .SetOptions(FOS_FILEMUSTEXIST | FOS_FORCEFILESYSTEM)
            .ok()?;
        dialog.Show(current_window()).ok()?;
        let value = dialog
            .GetResult()
            .ok()?
            .GetDisplayName(SIGDN_FILESYSPATH)
            .ok()?;
        let result = value.to_string().ok().map(PathBuf::from);
        CoTaskMemFree(Some(value.0.cast()));
        result
    }
}

pub fn current_window() -> Option<HWND> {
    window_for_process(unsafe { GetCurrentProcessId() })
}
fn window_for_process(owner: u32) -> Option<HWND> {
    unsafe extern "system" fn collect(hwnd: HWND, param: LPARAM) -> windows::core::BOOL {
        unsafe {
            let mut pid = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            let (owner, result) = &mut *(param.0 as *mut (u32, HWND));
            if pid == *owner
                && IsWindowVisible(hwnd).as_bool()
                && GetWindow(hwnd, GW_OWNER).unwrap_or_default().0.is_null()
            {
                *result = hwnd;
                return false.into();
            }
            true.into()
        }
    }
    let mut data = (owner, HWND::default());
    unsafe {
        let _ = EnumWindows(
            Some(collect),
            LPARAM((&mut data as *mut (u32, HWND)) as isize),
        );
    }
    (!data.1.0.is_null()).then_some(data.1)
}

fn explorer_selection(foreground: HWND) -> Option<PathBuf> {
    unsafe {
        let shell: IShellWindows = CoCreateInstance(&ShellWindows, None, CLSCTX_ALL).ok()?;
        for i in 0..shell.Count().ok()? {
            let Ok(dispatch) = shell.Item(&i.into()) else {
                continue;
            };
            let Ok(browser) = dispatch.cast::<IWebBrowserApp>() else {
                continue;
            };
            if browser.HWND().ok()?.0 != foreground.0 as isize {
                continue;
            }
            // Windows 11 tabs share the top-level HWND. The shell browser owns
            // a tab-specific child; hidden tabs must never provide selection.
            if let Ok(service) = dispatch.cast::<IServiceProvider>()
                && let Ok(tab) = service.QueryService::<IShellBrowser>(&SID_STopLevelBrowser)
                && let Ok(tab_hwnd) = tab.GetWindow()
            {
                if !IsWindowVisible(tab_hwnd).as_bool() {
                    continue;
                }
                let mut active =
                    FindWindowExW(Some(foreground), None, w!("ShellTabWindowClass"), None).ok();
                while let Some(candidate) = active {
                    if IsWindowVisible(candidate).as_bool() {
                        if candidate != tab_hwnd {
                            break;
                        }
                        let view = tab.QueryActiveShellView().ok()?;
                        let folder = view.cast::<IFolderView2>().ok()?;
                        let selection = folder.GetSelection(false).ok()?;
                        if selection.GetCount().ok()? == 0 {
                            return None;
                        }
                        let value = selection
                            .GetItemAt(0)
                            .ok()?
                            .GetDisplayName(SIGDN_FILESYSPATH)
                            .ok()?;
                        let path = value.to_string().ok().map(PathBuf::from);
                        CoTaskMemFree(Some(value.0.cast()));
                        return path;
                    }
                    active = FindWindowExW(
                        Some(foreground),
                        Some(candidate),
                        w!("ShellTabWindowClass"),
                        None,
                    )
                    .ok();
                }
                if active.is_some() {
                    continue;
                }
            }
            let document = browser.Document().ok()?;
            let folder = document.cast::<IShellFolderViewDual>().ok()?;
            let selected = folder.SelectedItems().ok()?;
            if selected.Count().ok()? == 0 {
                return None;
            }
            let item = selected.Item(&0i32.into()).ok()?;
            return item.Path().ok().map(|p| PathBuf::from(p.to_string()));
        }
        None
    }
}

unsafe extern "system" fn keyboard(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        if code == HC_ACTION as i32
            && wparam.0 == WM_KEYUP as usize
            && (*(lparam.0 as *const KBDLLHOOKSTRUCT)).vkCode == VK_SPACE.0 as u32
        {
            SPACE_DOWN.store(false, Ordering::Relaxed);
        }
        if code == HC_ACTION as i32 && wparam.0 == WM_KEYDOWN as usize {
            let event = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
            if event.vkCode == VK_SPACE.0 as u32
                && GetAsyncKeyState(VK_CONTROL.0 as i32) >= 0
                && GetAsyncKeyState(VK_MENU.0 as i32) >= 0
                && GetAsyncKeyState(VK_SHIFT.0 as i32) >= 0
            {
                let hwnd = GetForegroundWindow();
                let mut name = [0u16; 128];
                let n = GetClassNameW(hwnd, &mut name);
                if String::from_utf16_lossy(&name[..n as usize]) == "CabinetWClass" {
                    // Leave Space untouched in search/address/rename fields, including Win11 XAML fields.
                    let editing_or_unknown = is_editing(hwnd);
                    let was_down = SPACE_DOWN.swap(true, Ordering::Relaxed);
                    if !was_down || GetAsyncKeyState(VK_SPACE.0 as i32) >= 0 {
                        let _ = PostMessageW(
                            Some(HWND(BROKER.load(Ordering::Relaxed) as *mut _)),
                            OPEN_SELECTION,
                            WPARAM(hwnd.0 as usize),
                            LPARAM(0),
                        );
                    }
                    // Leave input intact while the UIA sample is pending. The worker
                    // resolves the actual focused control before opening anything.
                    if !editing_or_unknown {
                        return LRESULT(1);
                    }
                }
            }
        }
        CallNextHookEx(None, code, wparam, lparam)
    }
}

unsafe extern "system" fn broker_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            OPEN_SELECTION => {
                REQUESTS.with(|q| {
                    if let Some(tx) = q.borrow().as_ref() {
                        let _ = tx.send(w.0 as isize);
                    }
                });
                LRESULT(0)
            }
            TRAY if l.0 as u32 == WM_LBUTTONDBLCLK => {
                if let Ok(exe) = std::env::current_exe() {
                    let _ = Command::new(exe).arg("--preview").spawn();
                }
                LRESULT(0)
            }
            TRAY if l.0 as u32 == WM_RBUTTONUP => {
                if let Ok(menu) = CreatePopupMenu() {
                    let _ = AppendMenuW(menu, MF_STRING, 1, w!("打开预览…"));
                    let checked = |on| {
                        if on {
                            MF_STRING | MF_CHECKED
                        } else {
                            MF_STRING
                        }
                    };
                    let _ = AppendMenuW(menu, MF_STRING, 3, w!("检查更新…"));
                    let _ = AppendMenuW(menu, checked(updates::automatic()), 4, w!("自动更新"));
                    let _ = AppendMenuW(menu, checked(updates::startup()), 5, w!("开机自启"));
                    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
                    let _ = AppendMenuW(menu, MF_STRING, 2, w!("退出 QuickPeek"));
                    let mut point = POINT::default();
                    let _ = GetCursorPos(&mut point);
                    let _ = SetForegroundWindow(hwnd);
                    let choice =
                        TrackPopupMenu(menu, TPM_RETURNCMD, point.x, point.y, Some(0), hwnd, None)
                            .0;
                    let _ = DestroyMenu(menu);
                    if choice == 3 {
                        updates::check(true);
                    } else if choice == 4 {
                        if let Err(e) = updates::toggle_auto() {
                            show_error(&e);
                        } else if updates::automatic() {
                            updates::check(false);
                        }
                    } else if choice == 5 {
                        if let Err(e) = updates::toggle_startup() {
                            show_error(&e);
                        }
                    } else if choice == 2 {
                        let _ = DestroyWindow(hwnd);
                    } else if choice == 1
                        && let Ok(exe) = std::env::current_exe()
                    {
                        let _ = Command::new(exe).arg("--preview").spawn();
                    }
                }
                LRESULT(0)
            }
            WM_TIMER if w.0 == 17 => {
                SetTimer(Some(hwnd), 17, 24 * 60 * 60 * 1000, None);
                if updates::automatic() {
                    updates::check(false);
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                trace("broker WM_DESTROY");
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, w, l),
        }
    }
}

#[cfg(feature = "gpui-preview")]
fn standby_worker() -> Option<std::process::Child> {
    Command::new(std::env::current_exe().ok()?)
        .arg("--standby")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .ok()
}

fn selection_worker(requests: std::sync::mpsc::Receiver<isize>) {
    unsafe {
        if CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_err() {
            return;
        }
        {
            let automation: Option<IUIAutomation> =
                CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok();
            #[cfg(feature = "gpui-preview")]
            let mut standby = standby_worker();
            #[cfg(feature = "gpui-preview")]
            let mut used = false;
            loop {
                #[cfg(feature = "gpui-preview")]
                let target = if used {
                    match requests.recv_timeout(std::time::Duration::from_secs(30)) {
                        Ok(hwnd) => hwnd,
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                            if standby
                                .as_ref()
                                .is_some_and(|p| window_for_process(p.id()).is_none())
                            {
                                drop(standby.take());
                                standby = standby_worker();
                                used = false;
                            }
                            continue;
                        }
                    }
                } else if let Ok(hwnd) = requests.recv() {
                    hwnd
                } else {
                    break;
                };
                #[cfg(not(feature = "gpui-preview"))]
                let Ok(target) = requests.recv() else { break };
                trace("preview request");
                let hwnd = HWND(target as *mut _);
                if GetForegroundWindow() != hwnd || focused_editable(automation.as_ref()) {
                    continue;
                }
                let mut path = explorer_selection(hwnd);
                if path.is_none() {
                    std::thread::sleep(std::time::Duration::from_millis(25));
                    if GetForegroundWindow() == hwnd && !focused_editable(automation.as_ref()) {
                        path = explorer_selection(hwnd);
                    }
                }
                let Some(path) = path else {
                    continue;
                };
                if GetForegroundWindow() != hwnd {
                    continue;
                }
                #[cfg(feature = "gpui-preview")]
                if crate::gpui_preview::supports(&path) {
                    trace("selection ready");
                    use std::{io::Write, os::windows::ffi::OsStrExt};
                    if standby
                        .as_mut()
                        .is_none_or(|child| !matches!(child.try_wait(), Ok(None)))
                    {
                        standby = standby_worker();
                    }
                    let sent = standby.as_mut().is_some_and(|child| {
                        let _ = AllowSetForegroundWindow(child.id());
                        let Some(pipe) = child.stdin.as_mut() else {
                            return false;
                        };
                        let mut frame = vec![0; 4];
                        frame.extend(path.as_os_str().encode_wide().flat_map(u16::to_le_bytes));
                        let len = (frame.len() - 4) as u32;
                        frame[..4].copy_from_slice(&len.to_le_bytes());
                        pipe.write_all(&frame).is_ok()
                    });
                    if !sent {
                        standby = None;
                    }
                    if sent {
                        used = true;
                        continue;
                    }
                }
                if let Ok(exe) = std::env::current_exe()
                    && let Ok(child) = Command::new(exe).arg("--preview").arg(path).spawn()
                {
                    let _ = AllowSetForegroundWindow(child.id());
                }
            }
        }
        CoUninitialize();
    }
}

pub fn run_broker() -> Result<(), String> {
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED)
            .ok()
            .map_err(|e| e.to_string())?;
        if FindWindowW(w!("QuickPeek.Broker.v1"), None).is_ok() {
            return Ok(());
        }
        let instance = GetModuleHandleW(None).map_err(|e| e.to_string())?;
        let class = WNDCLASSW {
            lpfnWndProc: Some(broker_proc),
            hInstance: instance.into(),
            lpszClassName: w!("QuickPeek.Broker.v1"),
            ..Default::default()
        };
        RegisterClassW(&class);
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class.lpszClassName,
            w!("QuickPeek"),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .map_err(|e| e.to_string())?;
        BROKER.store(hwnd.0 as isize, Ordering::Relaxed);
        SetTimer(Some(hwnd), 17, 60_000, None);
        let mut data = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: 1,
            uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
            uCallbackMessage: TRAY,
            hIcon: app_icon()
                .unwrap_or(LoadIconW(None, IDI_APPLICATION).map_err(|e| e.to_string())?),
            ..Default::default()
        };
        let tip: Vec<u16> = "QuickPeek · 选中文件，按空格预览".encode_utf16().collect();
        data.szTip[..tip.len()].copy_from_slice(&tip);
        if !Shell_NotifyIconW(NIM_ADD, &data).as_bool() {
            return Err("无法创建托盘图标".into());
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || selection_worker(rx));
        REQUESTS.with(|q| *q.borrow_mut() = Some(tx));
        let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard), Some(instance.into()), 0)
            .map_err(|e| e.to_string())?;
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let _ = UnhookWindowsHookEx(hook);
        REQUESTS.with(|q| q.borrow_mut().take());
        let _ = worker.join();
        trace("broker loop ended and hook removed");
        let _ = Shell_NotifyIconW(NIM_DELETE, &data);
        trace("broker tray removed");
        CoUninitialize();
        Ok(())
    }
}

#[cfg(test)]
mod window_bounds_tests {
    use super::{RECT, fit_preview};

    #[test]
    fn fits_work_area_at_common_scales_and_desktop_origins() {
        for (w, h) in [
            (1280, 720),
            (1366, 768),
            (1920, 1080),
            (2560, 1440),
            (3840, 2160),
        ] {
            for percent in [100, 125, 150, 175, 200, 250, 300] {
                for (left, top) in [(0, 0), (-w, 0), (0, -h)] {
                    let area = RECT {
                        left,
                        top: top + 40 * percent / 100,
                        right: left + w,
                        bottom: top + h,
                    };
                    let (x, y, width, height) =
                        fit_preview(area, 920 * percent / 100 + 16, 640 * percent / 100 + 48);
                    assert!(
                        x >= area.left
                            && y >= area.top
                            && x + width <= area.right
                            && y + height <= area.bottom
                    );
                    assert!(((x - area.left) - (area.right - x - width)).abs() <= 1);
                    assert!(((y - area.top) - (area.bottom - y - height)).abs() <= 1);
                }
            }
        }
    }
}
