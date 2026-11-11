//! OS glue: clipboard, URLs, file manager, system theme/metrics, dialogs.

pub use mdview_render::Subpixel;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn has_safe_scheme(url: &str) -> bool {
    let b = url.as_bytes();
    let has = |p: &str| b.len() >= p.len() && b[..p.len()].eq_ignore_ascii_case(p.as_bytes());
    has("http://") || has("https://") || has("mailto:")
}

/// Native "open files" dialog filtered to Markdown plus "All files".
pub fn pick_files(start_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut d = rfd::FileDialog::new()
        .add_filter("Markdown", &["md", "markdown", "mdown", "mkd"])
        .add_filter("All files", &["*"]);
    if let Some(dir) = start_dir {
        d = d.set_directory(dir);
    }
    d.pick_files().unwrap_or_default()
}

/// Native folder picker.
pub fn pick_folder(start_dir: Option<&Path>) -> Option<PathBuf> {
    let mut d = rfd::FileDialog::new();
    if let Some(dir) = start_dir {
        d = d.set_directory(dir);
    }
    d.pick_folder()
}

#[cfg(windows)]
mod imp {
    use super::*;
    use windows_sys::Win32::Foundation::{GlobalFree, HANDLE, HWND};
    use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
    };
    use windows_sys::Win32::System::Memory::{
        GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
    };
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetDoubleClickTime;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetCaretBlinkTime, SW_SHOWNORMAL};

    const CF_UNICODETEXT: u32 = 13;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Opens the clipboard, retrying briefly; returns whether it is now open.
    fn open_clipboard() -> bool {
        for _ in 0..10 {
            // SAFETY: a null owner window is allowed; associates with the current task.
            if unsafe { OpenClipboard(std::ptr::null_mut()) } != 0 {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    pub fn clipboard_text() -> Option<String> {
        if !open_clipboard() {
            return None;
        }
        let result = read_clipboard_open();
        // SAFETY: the clipboard was opened by this thread above.
        unsafe { CloseClipboard() };
        result
    }

    fn read_clipboard_open() -> Option<String> {
        // SAFETY: the clipboard is open; the returned handle is owned by the system.
        let h: HANDLE = unsafe { GetClipboardData(CF_UNICODETEXT) };
        if h.is_null() {
            return None;
        }
        // SAFETY: h is a valid global memory handle from the clipboard.
        let ptr = unsafe { GlobalLock(h) } as *const u16;
        if ptr.is_null() {
            return None;
        }
        // SAFETY: h is locked, so querying its size is valid.
        let max = unsafe { GlobalSize(h) } / 2;
        // SAFETY: ptr points to `max` readable u16 units while the lock is held.
        let units = unsafe { std::slice::from_raw_parts(ptr, max) };
        let len = units.iter().position(|&u| u == 0).unwrap_or(units.len());
        let text = String::from_utf16_lossy(&units[..len]);
        // SAFETY: matches the GlobalLock above.
        unsafe { GlobalUnlock(h) };
        Some(text.replace("\r\n", "\n"))
    }

    pub fn set_clipboard_text(text: &str) -> bool {
        let converted;
        let text = if text.contains("\r\n") || !text.contains('\n') {
            text
        } else {
            converted = text.replace('\n', "\r\n");
            &converted
        };
        let data = wide(text);
        let bytes = data.len() * 2;
        if !open_clipboard() {
            return false;
        }
        let ok = write_clipboard_open(&data, bytes);
        // SAFETY: the clipboard was opened by this thread above.
        unsafe { CloseClipboard() };
        ok
    }

    fn write_clipboard_open(data: &[u16], bytes: usize) -> bool {
        // SAFETY: the clipboard is open and owned by this thread.
        if unsafe { EmptyClipboard() } == 0 {
            return false;
        }
        // SAFETY: plain allocation request.
        let h = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes) };
        if h.is_null() {
            return false;
        }
        // SAFETY: h is a valid moveable global handle.
        let dst = unsafe { GlobalLock(h) } as *mut u16;
        if dst.is_null() {
            // SAFETY: h is still owned by us.
            unsafe { GlobalFree(h) };
            return false;
        }
        // SAFETY: dst has room for `data.len()` u16 units; regions do not overlap.
        unsafe { std::ptr::copy_nonoverlapping(data.as_ptr(), dst, data.len()) };
        // SAFETY: matches the GlobalLock above.
        unsafe { GlobalUnlock(h) };
        // SAFETY: h holds NUL-terminated UTF-16; on success the system owns it.
        if unsafe { SetClipboardData(CF_UNICODETEXT, h) }.is_null() {
            // SAFETY: ownership was not transferred.
            unsafe { GlobalFree(h) };
            return false;
        }
        true
    }

    pub fn open_url(url: &str) -> bool {
        if !has_safe_scheme(url) {
            return false;
        }
        let verb = wide("open");
        let target = wide(url);
        // SAFETY: both strings are NUL-terminated and outlive the call.
        let r = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                verb.as_ptr(),
                target.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        };
        (r as isize) > 32
    }

    pub fn reveal_in_file_manager(path: &Path) -> bool {
        use std::os::windows::process::CommandExt;
        let mut arg = std::ffi::OsString::from("/select,\"");
        arg.push(path.as_os_str());
        arg.push("\"");
        // Explorer wants backslashes; raw_arg avoids Rust's own quoting.
        let arg = arg.to_string_lossy().replace('/', "\\");
        std::process::Command::new("explorer.exe")
            .raw_arg(arg)
            .spawn()
            .is_ok()
    }

    fn reg_dword(subkey: &str, name: &str) -> Option<u32> {
        let sub = wide(subkey);
        let nm = wide(name);
        let mut val: u32 = 0;
        let mut size: u32 = 4;
        // SAFETY: strings are NUL-terminated; val/size are valid for 4 bytes.
        let rc = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                sub.as_ptr(),
                nm.as_ptr(),
                RRF_RT_REG_DWORD,
                std::ptr::null_mut(),
                &mut val as *mut u32 as *mut _,
                &mut size,
            )
        };
        (rc == 0).then_some(val)
    }

    pub fn system_is_dark() -> bool {
        reg_dword(
            "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize",
            "AppsUseLightTheme",
        ) == Some(0)
    }

    pub fn double_click_time() -> Duration {
        // SAFETY: no preconditions.
        let ms = unsafe { GetDoubleClickTime() };
        Duration::from_millis(if ms == 0 { 500 } else { ms as u64 })
    }

    pub fn caret_blink_time() -> Option<Duration> {
        // SAFETY: no preconditions.
        let ms = unsafe { GetCaretBlinkTime() };
        match ms {
            0 => Some(Duration::from_millis(530)),
            u32::MAX => None,
            ms => Some(Duration::from_millis(ms as u64)),
        }
    }

    /// The system DPI as a scale factor, for rendering without a window (`--shot`).
    pub fn system_scale() -> f32 {
        use windows_sys::Win32::UI::HiDpi::{
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForSystem,
            SetProcessDpiAwarenessContext,
        };
        // SAFETY: no pointers; failure (already set) is harmless.
        unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        // SAFETY: no preconditions.
        let dpi = unsafe { GetDpiForSystem() };
        if dpi == 0 { 1.0 } else { dpi as f32 / 96.0 }
    }

    /// The system's text antialiasing: greyscale unless font smoothing is on and set to
    /// ClearType, then the ClearType stripe order. A failed query keeps the RGB default.
    pub fn subpixel_order() -> Subpixel {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            FE_FONTSMOOTHINGCLEARTYPE, FE_FONTSMOOTHINGORIENTATIONBGR, SPI_GETFONTSMOOTHING,
            SPI_GETFONTSMOOTHINGORIENTATION, SPI_GETFONTSMOOTHINGTYPE, SystemParametersInfoW,
        };
        let get = |action| {
            let mut value: u32 = 0;
            // SAFETY: each of these actions writes one BOOL or UINT (4 bytes) to `pvparam`.
            let ok =
                unsafe { SystemParametersInfoW(action, 0, (&mut value as *mut u32).cast(), 0) };
            (ok != 0).then_some(value)
        };
        match get(SPI_GETFONTSMOOTHING) {
            Some(0) => return Subpixel::None,
            None => return Subpixel::Rgb,
            Some(_) => {}
        }
        match get(SPI_GETFONTSMOOTHINGTYPE) {
            Some(FE_FONTSMOOTHINGCLEARTYPE) => {}
            Some(_) => return Subpixel::None,
            None => return Subpixel::Rgb,
        }
        match get(SPI_GETFONTSMOOTHINGORIENTATION) {
            Some(FE_FONTSMOOTHINGORIENTATIONBGR) => Subpixel::Bgr,
            _ => Subpixel::Rgb,
        }
    }

    /// The window rectangle, the client rectangle (screen coordinates) and the monitor's work area,
    /// for checking that a maximized frameless window covers exactly the work area.
    pub fn frame_report(hwnd: isize) -> String {
        use windows_sys::Win32::Foundation::{POINT, RECT};
        use windows_sys::Win32::Graphics::Gdi::{
            ClientToScreen, GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO,
            MonitorFromWindow,
        };
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetClientRect, GetWindowRect};
        let h = hwnd as HWND;
        let mut win = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        let mut client = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        let mut origin = POINT { x: 0, y: 0 };
        let mut info: MONITORINFO = unsafe { std::mem::zeroed() };
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        // SAFETY: all out-pointers reference live locals of the right type.
        unsafe {
            GetWindowRect(h, &mut win);
            GetClientRect(h, &mut client);
            ClientToScreen(h, &mut origin);
            let monitor = MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST);
            GetMonitorInfoW(monitor, &mut info);
        }
        let r = |r: RECT| format!("({},{})-({},{})", r.left, r.top, r.right, r.bottom);
        let client = RECT {
            left: origin.x,
            top: origin.y,
            right: origin.x + client.right,
            bottom: origin.y + client.bottom,
        };
        format!(
            "window {} client {} work {} monitor {}",
            r(win),
            r(client),
            r(info.rcWork),
            r(info.rcMonitor)
        )
    }

    pub fn style_window(hwnd: isize, dark: bool) {
        let h = hwnd as HWND;
        let corner: u32 = 2; // DWMWCP_ROUND
        let dark: u32 = dark as u32;
        // SAFETY: pointers reference 4-byte locals valid for the call; failures are ignored.
        unsafe {
            DwmSetWindowAttribute(h, 33, &corner as *const u32 as *const _, 4);
        }
        // SAFETY: as above.
        unsafe {
            DwmSetWindowAttribute(h, 20, &dark as *const u32 as *const _, 4);
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;

    pub fn clipboard_text() -> Option<String> {
        None
    }
    pub fn set_clipboard_text(_text: &str) -> bool {
        false
    }
    pub fn open_url(url: &str) -> bool {
        has_safe_scheme(url)
            && std::process::Command::new("xdg-open")
                .arg(url)
                .spawn()
                .is_ok()
    }
    pub fn reveal_in_file_manager(path: &Path) -> bool {
        let dir = path.parent().unwrap_or(path);
        std::process::Command::new("xdg-open")
            .arg(dir)
            .spawn()
            .is_ok()
    }
    pub fn system_is_dark() -> bool {
        false
    }
    pub fn double_click_time() -> Duration {
        Duration::from_millis(500)
    }
    pub fn caret_blink_time() -> Option<Duration> {
        Some(Duration::from_millis(530))
    }
    pub fn style_window(_hwnd: isize, _dark: bool) {}
    pub fn system_scale() -> f32 {
        1.0
    }
    pub fn subpixel_order() -> Subpixel {
        Subpixel::Rgb
    }
    pub fn frame_report(_hwnd: isize) -> String {
        String::new()
    }
}

/// UTF-8 text from the clipboard, if any.
pub fn clipboard_text() -> Option<String> {
    imp::clipboard_text()
}
/// Puts text on the clipboard. Returns false on failure.
pub fn set_clipboard_text(text: &str) -> bool {
    imp::set_clipboard_text(text)
}
/// Opens an http(s)/mailto URL with the default handler.
pub fn open_url(url: &str) -> bool {
    imp::open_url(url)
}
/// Opens the file manager with the file selected.
pub fn reveal_in_file_manager(path: &Path) -> bool {
    imp::reveal_in_file_manager(path)
}
/// Whether apps should use the dark theme.
pub fn system_is_dark() -> bool {
    imp::system_is_dark()
}
/// The system double-click interval, default 500 ms.
pub fn double_click_time() -> Duration {
    imp::double_click_time()
}
/// The caret blink half-period; None when blinking is disabled.
pub fn caret_blink_time() -> Option<Duration> {
    imp::caret_blink_time()
}
/// The desktop's display scale, without a window.
pub fn system_scale() -> f32 {
    imp::system_scale()
}

/// The subpixel order to antialias UI text with: the system's ClearType setting on Windows, RGB
/// elsewhere.
pub fn subpixel_order() -> Subpixel {
    imp::subpixel_order()
}
/// Window, client and work-area rectangles, for the frameless-window checks under `MDVIEW_TRACE`.
pub fn frame_report(hwnd: isize) -> String {
    imp::frame_report(hwnd)
}
/// Windows 11 polish for a borderless window (rounded corners, dark mode).
pub fn style_window(hwnd: isize, dark: bool) {
    imp::style_window(hwnd, dark)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn clipboard_round_trip() {
        let s = "İstanbul ı ğ 😀 ok\nsecond line";
        assert!(set_clipboard_text(s));
        assert_eq!(clipboard_text().as_deref(), Some(s));
    }

    #[test]
    fn open_url_refuses_unsafe() {
        assert!(!open_url("file:///c:/x"));
        assert!(!open_url("javascript:alert(1)"));
        assert!(!open_url(""));
    }

    /// The query itself cannot be checked against a known answer (it is the user's setting), but
    /// it must agree with itself and not fall over.
    #[test]
    fn subpixel_order_reads_the_system_setting() {
        let order = subpixel_order();
        eprintln!("system subpixel order: {order:?}");
        assert_eq!(order, subpixel_order());
    }
}
