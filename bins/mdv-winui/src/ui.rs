//! The window: a menu bar over one `SwapChainPanel` that shows the laid-out page, with the wheel
//! scrolling it. Text selection, links and the tree come later.
use crate::bindings::*;
use crate::diag;
use crate::render::{Gpu, PageState, Renderer};
use crate::tabs;
use mdview_core::commands::strings;
use mdview_core::files::{canonical_or_absolute, tab_labels};
use mdview_core::links::{LinkAction, classify_link};
use mdview_core::parse_bytes;
use mdview_core::session::{Recents, Session};
use mdview_render::{DARK, LIGHT};
use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::thread::JoinHandle;
use windows_core::*;

fn add_row(grid: &Grid, unit: GridUnitType, value: f64) -> Result<()> {
    let row = RowDefinition::new()?;
    row.SetHeight(GridLength {
        value,
        grid_unit_type: unit,
    })?;
    grid.RowDefinitions()?.Append(&row)
}

fn place(panel: &Grid, child: &impl Interface, row: i32, column: i32) -> Result<()> {
    let element: FrameworkElement = child.cast()?;
    Grid::SetRow(&element, row)?;
    Grid::SetColumn(&element, column)?;
    panel
        .cast::<IPanel>()?
        .Children()?
        .Append(&child.cast::<UIElement>()?)
}

windows_core::link!("kernel32.dll" "system" fn GetCurrentThreadId() -> u32);
windows_core::link!("user32.dll" "system" fn EnumThreadWindows(thread: u32, callback: unsafe extern "system" fn(*mut c_void, isize) -> i32, param: isize) -> i32);
windows_core::link!("user32.dll" "system" fn EnumChildWindows(parent: *mut c_void, callback: unsafe extern "system" fn(*mut c_void, isize) -> i32, param: isize) -> i32);
windows_core::link!("user32.dll" "system" fn GetClassNameW(window: *mut c_void, name: *mut u16, capacity: i32) -> i32);
windows_core::link!("user32.dll" "system" fn GetDpiForWindow(window: *mut c_void) -> u32);
windows_core::link!("user32.dll" "system" fn SetWindowPos(window: *mut c_void, after: *mut c_void, x: i32, y: i32, cx: i32, cy: i32, flags: u32) -> i32);
windows_core::link!("comdlg32.dll" "system" fn GetOpenFileNameW(ofn: *mut OpenFileNameW) -> i32);
windows_core::link!("advapi32.dll" "system" fn RegGetValueW(hkey: *mut c_void, subkey: PCWSTR, value: PCWSTR, flags: u32, ty: *mut u32, data: *mut c_void, size: *mut u32) -> i32);
windows_core::link!("user32.dll" "system" fn GetKeyState(key: i32) -> i16);
windows_core::link!("user32.dll" "system" fn SetForegroundWindow(hwnd: *mut c_void) -> i32);
windows_core::link!("user32.dll" "system" fn ShowWindow(hwnd: *mut c_void, command: i32) -> i32);
windows_core::link!("user32.dll" "system" fn SetTimer(hwnd: *mut c_void, id: usize, ms: u32, callback: unsafe extern "system" fn(*mut c_void, u32, usize, u32)) -> usize);
windows_core::link!("user32.dll" "system" fn OpenClipboard(owner: *mut c_void) -> i32);
windows_core::link!("user32.dll" "system" fn CloseClipboard() -> i32);
windows_core::link!("user32.dll" "system" fn EmptyClipboard() -> i32);
windows_core::link!("user32.dll" "system" fn SetClipboardData(format: u32, data: *mut c_void) -> *mut c_void);
windows_core::link!("user32.dll" "system" fn GetClipboardData(format: u32) -> *mut c_void);
windows_core::link!("kernel32.dll" "system" fn GlobalAlloc(flags: u32, bytes: usize) -> *mut c_void);
windows_core::link!("kernel32.dll" "system" fn GlobalLock(mem: *mut c_void) -> *mut c_void);
windows_core::link!("kernel32.dll" "system" fn GlobalUnlock(mem: *mut c_void) -> i32);
windows_core::link!("kernel32.dll" "system" fn GlobalFree(mem: *mut c_void) -> *mut c_void);
windows_core::link!("shell32.dll" "system" fn ShellExecuteW(window: *mut c_void, operation: PCWSTR, file: PCWSTR, parameters: PCWSTR, directory: PCWSTR, show: i32) -> isize);

/// Starts receiving files from later launches (see `mdview_core::single`).
pub fn set_incoming(receiver: std::sync::mpsc::Receiver<PathBuf>) {
    INCOMING.with(|cell| *cell.borrow_mut() = Some(receiver));
}

thread_local! {
    /// Paths later launches sent to this window; drained by the poll timer.
    static INCOMING: RefCell<Option<std::sync::mpsc::Receiver<PathBuf>>> = const { RefCell::new(None) };
    static SHELL: RefCell<Option<Rc<Shell>>> = const { RefCell::new(None) };
}

/// Follows the Windows app theme, the way the shell's own chrome does.
fn system_theme() -> mdview_render::Theme {
    const HKEY_CURRENT_USER: isize = 0xFFFF_FFFF_8000_0001u64 as isize;
    const RRF_RT_REG_DWORD: u32 = 0x10;
    let mut light: u32 = 1;
    let mut size = size_of::<u32>() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER as *mut c_void,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&raw mut light).cast(),
            &mut size,
        )
    };
    if status == 0 && light == 0 {
        DARK
    } else {
        LIGHT
    }
}

const WINDOW_CLASS: &str = "WinUIDesktopWin32WindowClass";

thread_local! {
    /// Logical size of the window to open, so `--size` can make it big for a measurement.
    static WINDOW_SIZE: Cell<(i32, i32)> = const { Cell::new((1100, 900)) };
}

unsafe extern "system" fn size_if_winui(window: *mut c_void, _: isize) -> i32 {
    const SWP_NOMOVE_NOZORDER_NOACTIVATE: u32 = 0x0002 | 0x0004 | 0x0010;
    let mut name = [0u16; 64];
    let length = unsafe { GetClassNameW(window, name.as_mut_ptr(), name.len() as i32) };
    if String::from_utf16_lossy(&name[..length.max(0) as usize]) != WINDOW_CLASS {
        return 1;
    }
    let dpi = unsafe { GetDpiForWindow(window) } as i32;
    let (logical_width, logical_height) = WINDOW_SIZE.with(|size| size.get());
    let (width, height) = (logical_width * dpi / 96, logical_height * dpi / 96);
    let moved = unsafe {
        SetWindowPos(
            window,
            std::ptr::null_mut(),
            0,
            0,
            width,
            height,
            SWP_NOMOVE_NOZORDER_NOACTIVATE,
        )
    };
    diag::trace(&format!(
        "window sized {width}x{height} at dpi {dpi}, ok={moved}"
    ));
    0
}

unsafe extern "system" fn remember_window(window: *mut c_void, out: isize) -> i32 {
    let mut name = [0u16; 64];
    let length = unsafe { GetClassNameW(window, name.as_mut_ptr(), name.len() as i32) };
    if String::from_utf16_lossy(&name[..length.max(0) as usize]) == WINDOW_CLASS {
        unsafe { *(out as *mut isize) = window as isize };
        return 0;
    }
    1
}

/// The WinUI window's `HWND`, for sizing and to own the file dialog.
fn winui_window() -> isize {
    let mut found: isize = 0;
    unsafe {
        EnumThreadWindows(
            GetCurrentThreadId(),
            remember_window,
            &mut found as *mut isize as isize,
        )
    };
    found
}

fn set_window_size(width: i32, height: i32) {
    WINDOW_SIZE.with(|size| size.set((width, height)));
    unsafe { EnumThreadWindows(GetCurrentThreadId(), size_if_winui, 0) };
}

#[repr(C)]
struct OpenFileNameW {
    struct_size: u32,
    owner: *mut c_void,
    instance: *mut c_void,
    filter: *const u16,
    custom_filter: *mut u16,
    max_custom_filter: u32,
    filter_index: u32,
    file: *mut u16,
    max_file: u32,
    file_title: *mut u16,
    max_file_title: u32,
    initial_dir: *const u16,
    title: *const u16,
    flags: u32,
    file_offset: u16,
    file_extension: u16,
    default_ext: *const u16,
    custom_data: isize,
    hook: *mut c_void,
    template_name: *const u16,
    reserved: *mut c_void,
    reserved2: u32,
    flags_ex: u32,
}

/// `GetOpenFileNameW`, because the WinRT file pickers need a window handle and a COM object this
/// app does not otherwise use.
/// Opens a web or mail link in the user's browser. Best effort: a failure leaves no message on a
/// windowless shell, but the trace log has the destination.
fn open_url(url: &str) {
    let wide: Vec<u16> = url.encode_utf16().chain(std::iter::once(0)).collect();
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            PCWSTR(std::ptr::null()),
            PCWSTR(wide.as_ptr()),
            PCWSTR(std::ptr::null()),
            PCWSTR(std::ptr::null()),
            1,
        )
    };
    diag::trace(&format!("opened {url}: {result}"));
}

fn file_dialog() -> Option<PathBuf> {
    const FILE_MUST_EXIST: u32 = 0x0000_1000;
    const PATH_MUST_EXIST: u32 = 0x0000_0800;
    const NO_CHANGE_DIR: u32 = 0x0000_0008;
    const EXPLORER: u32 = 0x0008_0000;
    let filter: Vec<u16> = "Markdown\0*.md;*.markdown;*.mdown;*.txt\0All files\0*.*\0\0"
        .encode_utf16()
        .collect();
    let mut buffer = vec![0u16; 32_768];
    let mut ofn = OpenFileNameW {
        struct_size: size_of::<OpenFileNameW>() as u32,
        owner: winui_window() as *mut c_void,
        instance: std::ptr::null_mut(),
        filter: filter.as_ptr(),
        custom_filter: std::ptr::null_mut(),
        max_custom_filter: 0,
        filter_index: 1,
        file: buffer.as_mut_ptr(),
        max_file: buffer.len() as u32,
        file_title: std::ptr::null_mut(),
        max_file_title: 0,
        initial_dir: std::ptr::null(),
        title: std::ptr::null(),
        flags: FILE_MUST_EXIST | PATH_MUST_EXIST | NO_CHANGE_DIR | EXPLORER,
        file_offset: 0,
        file_extension: 0,
        default_ext: std::ptr::null(),
        custom_data: 0,
        hook: std::ptr::null_mut(),
        template_name: std::ptr::null(),
        reserved: std::ptr::null_mut(),
        reserved2: 0,
        flags_ex: 0,
    };
    if unsafe { GetOpenFileNameW(&mut ofn) } == 0 {
        return None;
    }
    let length = buffer.iter().position(|&c| c == 0).unwrap_or(0);
    (length > 0).then(|| PathBuf::from(String::from_utf16_lossy(&buffer[..length])))
}

/// A tab. The showing tab's document state lives in the `Renderer`; the others keep theirs here,
/// and `activate` swaps them, as the Qt shell does.
struct Page {
    path: PathBuf,
    item: TabViewItem,
    label: String,
    state: Option<PageState>,
}

struct Shell {
    window: Window,
    root: Grid,
    renderer: RefCell<Renderer>,
    /// The showing tab's file; `None` on the empty page.
    path: RefCell<Option<PathBuf>>,
    /// Tab headers only; the one panel below draws whichever page is current.
    tab_view: TabView,
    /// The page panel, to give the keyboard back after a tab or menu click.
    page: UIElement,
    /// Open documents in tab order, and which one is showing. `None` shows the empty page.
    pages: RefCell<Vec<Page>>,
    current: Cell<Option<usize>>,
    /// Set while the shell changes the TabView itself: XAML raises SelectionChanged synchronously
    /// inside Append, RemoveAt and SelectedIndex, and those must not re-enter.
    tabs_busy: Cell<bool>,
    recent: RefCell<Recents>,
    /// The saved session and where it lives. `None` with the session off (`--no-session`, and
    /// scripted checks unless `MDVIEW_SETTINGS` is set).
    session: RefCell<Option<(PathBuf, Session)>>,
    /// The Recent menu, rebuilt whenever Recent changes.
    recent_menu: RefCell<Option<MenuBarItem>>,
    /// Run the tabs check after the first frame, for `--tabs-test`.
    tabs_test: Cell<bool>,
    /// What the showing tab's file looked like when last read; the 800 ms poll compares against it.
    stamp: Cell<mdview_core::files::FileStamp>,
    /// Run the zoom check after the first frame, for `--zoom-test`.
    zoom_test: Cell<bool>,
    presented: Cell<bool>,
    /// How far into the scrollbar thumb the drag started, in panel device px.
    dragging: Cell<Option<f32>>,
    /// Remaining `--scroll-steps` steps, each taken after a frame has been drawn.
    steps: Cell<u32>,
    /// A text selection drag is in progress.
    selecting: Cell<bool>,
    /// Where the selection press landed.
    selection_anchor: Cell<Option<u32>>,
    /// The link or footnote under the press, followed on release when the pointer did not move.
    pressed_link: Cell<Option<u32>>,
    pressed_footnote: Cell<Option<u32>>,
    pressed_details: Cell<Option<u32>>,
    selection_moved: Cell<bool>,
    /// Select a word after the first frame, for `--select-test`.
    select_test: Cell<bool>,
    /// Toggle the first details after the first frame, for `--details-test`.
    details_test: Cell<bool>,
    /// Find mode: the next typed characters build the query.
    find_active: Cell<bool>,
    /// Run the find check after the first frame, for `--find-test`.
    find_test: Cell<bool>,
    /// Run the outline check after the first frame, for `--outline-test`.
    outline_test: Cell<bool>,
    /// Run the clipboard check after the first frame, for `--copy-test`.
    copy_test: Cell<bool>,
    /// The Outline menu, rebuilt when a document loads.
    outline_menu: RefCell<Option<MenuBarItem>>,
    outline_entries: Cell<usize>,
}

impl Shell {
    /// A key no control handled, from the root's `KeyDown`. Returns true when the shell used it.
    fn key(&self, wparam: usize) -> bool {
        let ctrl = unsafe { GetKeyState(0x11) } < 0;
        let shift = unsafe { GetKeyState(0x10) } < 0;
        self.key_with(wparam, ctrl, shift)
    }

    /// `key` with the modifiers given, so the scripted checks can press Ctrl+Shift+Tab.
    fn key_with(&self, wparam: usize, ctrl: bool, shift: bool) -> bool {
        {
            {
                const VK_ESCAPE: usize = 0x1b;
                const VK_TAB: usize = 0x09;
                const VK_F4: usize = 0x73;
                if ctrl && wparam == 0x4f {
                    self.open_dialog();
                    return true;
                }
                if ctrl && (wparam == 0x57 || (wparam == VK_F4 && !shift)) {
                    if shift {
                        self.close_all();
                    } else if let Some(current) = self.current.get() {
                        self.close_tab(current);
                    }
                    return true;
                }
                if ctrl && wparam == VK_TAB {
                    self.cycle(if shift { -1 } else { 1 });
                    return true;
                }
                if ctrl && wparam == 0x46 {
                    self.find_open();
                    return true;
                }
                if ctrl && matches!(wparam, 0x43 | 0x63) {
                    self.copy_selection();
                    return true;
                }
                // Zoom, 60-250% in steps of 10: Ctrl+plus (also numpad), Ctrl+minus, Ctrl+0.
                if ctrl && matches!(wparam, 0xbb | 0x6b | 0xbd | 0x6d | 0x30 | 0x60) {
                    self.zoom_step(match wparam {
                        0xbb | 0x6b => 1,
                        0xbd | 0x6d => -1,
                        _ => 0,
                    });
                    return true;
                }
                if wparam == 0x74 {
                    if let Some(shell) = SHELL.with(|cell| cell.borrow().clone()) {
                        reload(&shell);
                    }
                    return true;
                }
                if wparam == 0x72 && self.find_active.get() {
                    self.find_step(if shift { -1 } else { 1 });
                    return true;
                }
                if wparam == VK_ESCAPE {
                    if self.find_active.get() {
                        self.find_close();
                        return true;
                    }
                    self.renderer.borrow_mut().set_selection(None);
                    self.show();
                    return true;
                }
                let scrolled = self.renderer.borrow_mut().key(wparam as u32);
                if scrolled {
                    diag::trace(&format!(
                        "key {:#04x}, offset {:.0}",
                        wparam,
                        self.renderer.borrow().offset()
                    ));
                    self.show();
                    true
                } else {
                    false
                }
            }
        }
    }

    fn poll_file(&self) {
        use mdview_core::files::{FileStamp, read_stable, stamp};
        let Some(path) = self.path.borrow().clone() else {
            return;
        };
        let now = stamp(&path);
        if now == self.stamp.get() {
            return;
        }
        let name = file_name(&path);
        if now == FileStamp::Missing {
            self.stamp.set(now);
            _ = self
                .window
                .SetTitle(&format!("mdview - {name} (deleted; showing the last content)"));
            return;
        }
        let Ok(loaded) = read_stable(&path) else {
            return; // still being written; the next tick tries again
        };
        self.stamp.set(loaded.stamp);
        let document = parse_bytes(&loaded.bytes);
        self.renderer
            .borrow_mut()
            .reload_document(document, path.parent().map(Path::to_path_buf));
        _ = self.window.SetTitle(&format!("mdview - {name}"));
        refresh_outline(self);
        diag::trace("auto-refresh: file changed on disk, reloaded");
        self.show();
    }

    /// Zooms one step (+1 / -1) or back to 100% (0), within 60-250%, and redraws.
    fn zoom_step(&self, dir: i32) -> f32 {
        let now = self.renderer.borrow().zoom();
        let next = match dir {
            0 => 1.0,
            d => now + 0.1 * d as f32,
        };
        let next = ((next * 10.0).round() / 10.0).clamp(0.6, 2.5);
        if (next - now).abs() > 1e-4 {
            self.renderer.borrow_mut().set_zoom(next);
            if let Some((_, session)) = self.session.borrow_mut().as_mut() {
                session.zoom = (next * 100.0).round() as u16;
            }
            self.save_session();
            self.show();
        }
        next
    }

    /// Typed text from `CharacterReceived`, which only find mode uses. Escape is left to `key`.
    fn character(&self, wparam: usize) -> bool {
        {
            {
                if !self.find_active.get() {
                    return false;
                }
                match wparam {
                    0x08 => {
                        let mut query = self.renderer.borrow().find_query().to_string();
                        query.pop();
                        self.find_set_query(query);
                    }
                    0x0d | 0x0a => self.find_step(1),
                    0x1b => {}
                    code => {
                        if let Some(c) = char::from_u32(code as u32)
                            && !c.is_control()
                        {
                            let mut query = self.renderer.borrow().find_query().to_string();
                            query.push(c);
                            self.find_set_query(query);
                        }
                    }
                }
                true
            }
        }
    }

    /// Ctrl+F: start a find. The next typed characters build the query; the title shows its state.
    fn find_open(&self) {
        self.find_active.set(true);
        self.renderer.borrow_mut().find_update("");
        self.refresh_find_title();
        self.show();
    }

    fn find_close(&self) {
        self.find_active.set(false);
        self.renderer.borrow_mut().find_update("");
        let name = self
            .path
            .borrow()
            .as_ref()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "mdview".to_string());
        _ = self.window.SetTitle(&format!("mdview - {name}"));
        self.show();
    }

    fn find_set_query(&self, query: String) {
        self.renderer.borrow_mut().find_update(&query);
        self.refresh_find_title();
        self.show();
    }

    fn find_step(&self, delta: i32) {
        if !self.find_active.get() {
            return;
        }
        self.renderer.borrow_mut().find_step(delta);
        self.refresh_find_title();
        self.show();
    }

    fn copy_selection(&self) {
        let Some(text) = self.renderer.borrow().selection_text() else {
            diag::trace("copy: no selection");
            return;
        };
        if set_clipboard_text(&text) {
            diag::trace(&format!("copy: {} characters", text.chars().count()));
        } else {
            diag::trace("copy: clipboard unavailable");
        }
    }

    fn refresh_find_title(&self) {
        let name = self
            .path
            .borrow()
            .as_ref()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "mdview".to_string());
        let (hits, current, capped) = self.renderer.borrow().find_summary();
        let query = self.renderer.borrow().find_query().to_string();
        let status = if query.is_empty() {
            "type to search".to_string()
        } else if hits == 0 {
            "no matches".to_string()
        } else {
            format!(
                "{} of {}{}",
                current + 1,
                hits,
                if capped { "+" } else { "" }
            )
        };
        _ = self
            .window
            .SetTitle(&format!("mdview - {name} — Find: {query} [{status}]"));
    }

    /// Follows the link or footnote under a click. A `#fragment` scrolls here, another Markdown
    /// file is loaded (and scrolled to its fragment), and anything else goes to the browser.
    fn follow(&self, link: Option<u32>, footnote: Option<u32>) {
        if let Some(index) = footnote {
            if self.renderer.borrow_mut().follow_footnote(index) {
                self.show();
            }
            return;
        }
        let Some(index) = link else {
            return;
        };
        let Some(destination) = self.renderer.borrow().link_destination(index) else {
            return;
        };
        let doc_dir = self
            .path
            .borrow()
            .as_ref()
            .and_then(|path| path.parent().map(Path::to_path_buf))
            .unwrap_or_default();
        // Core decides what a link may do: it ignores unknown schemes and refuses UNC paths, so a
        // document can never make the machine contact another host.
        let (resolved, fragment) = match classify_link(&destination, &doc_dir) {
            LinkAction::Fragment(fragment) => {
                self.renderer.borrow_mut().scroll_to_fragment(&fragment);
                self.show();
                return;
            }
            LinkAction::External(url) => {
                open_url(&url);
                return;
            }
            LinkAction::OpenMarkdown { path, fragment } => (path, fragment),
            LinkAction::OtherLocalFile(path) => {
                diag::trace(&format!("not a markdown link: {}", path.display()));
                return;
            }
            LinkAction::Ignored => {
                diag::trace(&format!("ignored link: {destination}"));
                return;
            }
        };
        if !resolved.is_file() {
            diag::trace(&format!(
                "link target is not a file: {}",
                resolved.display()
            ));
            return;
        }
        // Lay the new document out first, then jump: `scroll_to_fragment` needs a page.
        if self.open(&resolved) {
            self.show();
            if let Some(fragment) = fragment {
                self.renderer.borrow_mut().scroll_to_fragment(&fragment);
            }
            self.show();
        }
    }

    /// Opens `path` in a new tab, or switches to the tab that already shows it. Adds it to Recent
    /// either way. False when the file could not be read.
    fn open(&self, path: &Path) -> bool {
        let path = canonical_or_absolute(path);
        let existing = self
            .pages
            .borrow()
            .iter()
            .position(|page| same_file(&page.path, &path));
        if let Some(index) = existing {
            self.recent.borrow_mut().add(path);
            self.switch_to(index);
            self.save_session();
            return true;
        }
        match read_document(&path) {
            Ok(document) => {
                self.recent.borrow_mut().add(path.clone());
                let index = self.add_page(path, document);
                self.switch_to(index);
                self.save_session();
                true
            }
            Err(e) => {
                diag::error_box(&e);
                false
            }
        }
    }

    /// File > Open and Ctrl+O.
    fn open_dialog(&self) {
        if let Some(path) = file_dialog() {
            self.open(&path);
        }
        self.focus_page();
    }

    /// Appends a tab without showing it, and returns its index.
    fn add_page(&self, path: PathBuf, document: mdview_core::Document) -> usize {
        let item = match TabViewItem::new() {
            Ok(item) => item,
            Err(e) => diag::fatal("TabViewItem", &e),
        };
        let base_dir = path.parent().map(Path::to_path_buf);
        let index = {
            let mut pages = self.pages.borrow_mut();
            pages.push(Page {
                path,
                item: item.clone(),
                label: String::new(),
                state: Some(PageState::new(document, base_dir)),
            });
            pages.len() - 1
        };
        self.tabs_busy.set(true);
        if let Ok(items) = self.tab_view.TabItems()
            && let Ok(item) = item.cast::<IInspectable>()
        {
            _ = items.Append(&item);
        }
        self.tabs_busy.set(false);
        self.relabel();
        self.update_strip();
        index
    }

    /// Restores a saved tab. A file that is gone still gets its tab, saying so.
    fn restore_page(&self, path: &Path) {
        let path = canonical_or_absolute(path);
        if self
            .pages
            .borrow()
            .iter()
            .any(|page| same_file(&page.path, &path))
        {
            return;
        }
        let document = read_document(&path).unwrap_or_else(|_| {
            let name = file_name(&path);
            parse_bytes(format!("# {name}\n\n{}\n", strings::FILE_NOT_FOUND).as_bytes())
        });
        self.add_page(path, document);
    }

    /// Selects tab `index` in the strip and shows it.
    fn switch_to(&self, index: usize) {
        self.tabs_busy.set(true);
        _ = tabs::set_selected_index(&self.tab_view, index as i32);
        self.tabs_busy.set(false);
        self.activate(index);
    }

    /// Shows tab `index`, keeping the state of the tab it replaces.
    fn activate(&self, index: usize) {
        if self.current.get() == Some(index) || index >= self.pages.borrow().len() {
            return;
        }
        self.stash();
        self.current.set(Some(index));
        let (path, state) = {
            let mut pages = self.pages.borrow_mut();
            let page = &mut pages[index];
            (page.path.clone(), page.state.take())
        };
        if let Some(state) = state {
            self.renderer.borrow_mut().put_page(state);
        }
        self.stamp.set(mdview_core::files::stamp(&path));
        *self.path.borrow_mut() = Some(path.clone());
        self.show_page(&file_name(&path));
    }

    /// Moves the showing tab's state back into its `Page`.
    fn stash(&self) {
        let Some(index) = self.current.get() else {
            return;
        };
        let state = self.renderer.borrow_mut().take_page();
        if let Some(page) = self.pages.borrow_mut().get_mut(index) {
            page.state = Some(state);
        }
    }

    /// Resets what belongs to the page on screen: find, the title and the outline.
    fn show_page(&self, name: &str) {
        self.find_active.set(false);
        self.renderer.borrow_mut().find_update("");
        _ = self.window.SetTitle(&format!("mdview - {name}"));
        refresh_outline(self);
        self.show();
    }

    /// Closes tab `index`. The last one closing shows the empty page; the app stays open.
    fn close_tab(&self, index: usize) {
        self.sync_order();
        if index >= self.pages.borrow().len() {
            return;
        }
        match self.current.get() {
            // The showing tab's state is in the renderer and is simply replaced.
            Some(current) if current == index => self.current.set(None),
            Some(current) if current > index => self.current.set(Some(current - 1)),
            _ => {}
        }
        self.pages.borrow_mut().remove(index);
        self.tabs_busy.set(true);
        if let Ok(items) = self.tab_view.TabItems() {
            _ = items.RemoveAt(index as u32);
        }
        if let Some(current) = self.current.get() {
            _ = tabs::set_selected_index(&self.tab_view, current as i32);
        }
        self.tabs_busy.set(false);
        self.relabel();
        self.update_strip();
        let count = self.pages.borrow().len();
        if count == 0 {
            self.show_empty(strings::STATUS_ALL_CLOSED);
        } else if self.current.get().is_none() {
            self.switch_to(index.min(count - 1));
        }
        self.save_session();
        self.focus_page();
    }

    fn close_all(&self) {
        while !self.pages.borrow().is_empty() {
            let last = self.pages.borrow().len() - 1;
            self.close_tab(last);
        }
    }

    /// Ctrl+Tab and Ctrl+Shift+Tab: the next or previous tab, wrapping. Nothing with one tab.
    fn cycle(&self, delta: isize) {
        self.sync_order();
        let count = self.pages.borrow().len();
        let Some(current) = self.current.get() else {
            return;
        };
        if count > 1 {
            let next = (current as isize + delta).rem_euclid(count as isize) as usize;
            self.switch_to(next);
        }
    }

    /// Puts `pages` in the TabView's order, which a drag may have changed, keeping `current` on
    /// the same page. True when anything moved.
    fn sync_order(&self) -> bool {
        let Ok(items) = self.tab_view.TabItems() else {
            return false;
        };
        let Ok(size) = items.Size() else {
            return false;
        };
        let order: Vec<IInspectable> = (0..size).filter_map(|i| items.GetAt(i).ok()).collect();
        let mut pages = self.pages.borrow_mut();
        if order.len() != pages.len() {
            return false;
        }
        let in_order = order
            .iter()
            .zip(pages.iter())
            .all(|(item, page)| tabs::same_object(item, &page.item));
        if in_order {
            return false;
        }
        let current_item = self.current.get().map(|i| pages[i].item.clone());
        let mut old: Vec<Option<Page>> = pages.drain(..).map(Some).collect();
        for item in &order {
            if let Some(slot) = old
                .iter_mut()
                .find(|slot| slot.as_ref().is_some_and(|p| tabs::same_object(item, &p.item)))
            {
                pages.push(slot.take().expect("found"));
            }
        }
        // Anything not matched (it should not happen) goes at the end rather than being lost.
        pages.extend(old.into_iter().flatten());
        if let Some(current_item) = current_item {
            self.current.set(
                pages
                    .iter()
                    .position(|p| tabs::same_object(&p.item, &current_item)),
            );
        }
        true
    }

    /// A tab was dragged to a new place in the strip.
    fn tabs_reordered(&self) {
        if self.sync_order() {
            self.relabel();
            self.save_session();
        }
    }

    /// The user picked a tab in the strip.
    fn tab_selected(&self) {
        if self.tabs_busy.get() {
            return;
        }
        self.sync_order();
        let Ok(index) = tabs::selected_index(&self.tab_view) else {
            return;
        };
        if index >= 0 {
            self.activate(index as usize);
            self.save_session();
        }
        self.focus_page();
    }

    /// The close button on a tab.
    fn tab_close_requested(&self, item: &TabViewItem) {
        self.sync_order();
        let index = self
            .pages
            .borrow()
            .iter()
            .position(|page| tabs::same_object(&page.item, item));
        if let Some(index) = index {
            self.close_tab(index);
        }
    }

    /// Tab titles from core: repeated filenames gain parent folders until they differ.
    fn relabel(&self) {
        let paths: Vec<PathBuf> = self.pages.borrow().iter().map(|p| p.path.clone()).collect();
        let labels = tab_labels(&paths);
        let items: Vec<TabViewItem> = {
            let mut pages = self.pages.borrow_mut();
            for (page, label) in pages.iter_mut().zip(&labels) {
                page.label = label.clone();
            }
            pages.iter().map(|p| p.item.clone()).collect()
        };
        for ((item, label), path) in items.iter().zip(&labels).zip(&paths) {
            _ = item.SetHeader(&tabs::boxed(label));
            _ = tabs::set_tooltip(item, &path.display().to_string());
        }
    }

    /// The strip shows only while a tab is open.
    fn update_strip(&self) {
        let any = !self.pages.borrow().is_empty();
        if let Ok(element) = self.tab_view.cast::<UIElement>() {
            _ = tabs::set_visible(&element, any);
        }
    }

    /// The page shown with no tab open.
    fn show_empty(&self, status: &str) {
        self.current.set(None);
        *self.path.borrow_mut() = None;
        self.renderer
            .borrow_mut()
            .put_page(PageState::new(parse_bytes(WELCOME.as_bytes()), None));
        self.find_active.set(false);
        _ = self.window.SetTitle("mdview");
        refresh_outline(self);
        self.show();
        self.message(status);
    }

    /// No status bar here: messages go to the trace and, briefly, the title.
    fn message(&self, text: &str) {
        diag::trace(&format!("status: {text}"));
        let name = self
            .path
            .borrow()
            .as_deref()
            .map(file_name)
            .map_or_else(|| "mdview".to_string(), |name| format!("mdview - {name}"));
        _ = self.window.SetTitle(&format!("{name} — {text}"));
    }

    /// Writes tabs, the current tab and Recent at once, so a crash loses nothing. Also rebuilds
    /// the Recent menu, since every change to Recent ends here.
    fn save_session(&self) {
        self.sync_order();
        refresh_recent(self);
        let mut session = self.session.borrow_mut();
        let Some((path, session)) = session.as_mut() else {
            return;
        };
        session.tabs = self.pages.borrow().iter().map(|p| p.path.clone()).collect();
        session.active = self.current.get().unwrap_or(0);
        session.recent = self.recent.borrow().clone();
        match session.save(path) {
            Ok(()) => diag::trace(&format!("session saved: {} tabs", session.tabs.len())),
            Err(e) => diag::trace(&format!("could not save {}: {e}", path.display())),
        }
    }

    fn focus_page(&self) {
        _ = crate::input::focus(&self.page, crate::input::FOCUS_PROGRAMMATIC);
    }

    /// `--tabs-test`: with showcase.md open, opens `linked.md` beside it, switches back by opening
    /// the first again, cycles, moves, closes both, and checks tabs, scroll, labels and Recent.
    fn run_tabs_check(&self) -> ! {
        let mut failures: Vec<String> = Vec::new();
        let mut expect = |ok: bool, what: String| {
            if !ok {
                failures.push(what);
            }
        };
        let Some(first) = self.path.borrow().clone() else {
            diag::trace("tabs test failed: start with a document");
            std::process::exit(1);
        };
        let selected = || tabs::selected_index(&self.tab_view).unwrap_or(-2);
        let strip = || self.tab_view.TabItems().and_then(|i| i.Size()).unwrap_or(99);
        let other = first.with_file_name("linked.md");
        self.renderer.borrow_mut().scroll_to(500.0);
        self.show();
        let scrolled = self.renderer.borrow().offset();
        expect(scrolled > 0.0, format!("first page did not scroll ({scrolled})"));
        self.open(&other);
        expect(self.pages.borrow().len() == 2, "second tab not added".into());
        expect(self.current.get() == Some(1), "second tab not current".into());
        expect(
            strip() == 2 && selected() == 1,
            format!("strip out of step after open: {} {}", strip(), selected()),
        );
        expect(
            self.renderer.borrow().offset() == 0.0,
            "new tab did not start at the top".into(),
        );
        self.open(&first);
        expect(self.pages.borrow().len() == 2, "reopening added a duplicate".into());
        expect(self.current.get() == Some(0), "reopening did not switch".into());
        expect(selected() == 0, "strip did not follow the switch".into());
        let kept = self.renderer.borrow().offset();
        expect(
            kept == scrolled,
            format!("scroll not kept: {kept} vs {scrolled}"),
        );
        let labels: Vec<String> = self.pages.borrow().iter().map(|p| p.label.clone()).collect();
        expect(
            labels == ["showcase.md", "linked.md"],
            format!("labels {labels:?}"),
        );
        self.key_with(0x09, true, false);
        expect(
            self.current.get() == Some(1) && selected() == 1,
            "Ctrl+Tab".into(),
        );
        self.key_with(0x09, true, false);
        expect(self.current.get() == Some(0), "Ctrl+Tab did not wrap".into());
        self.key_with(0x09, true, true);
        expect(self.current.get() == Some(1), "Ctrl+Shift+Tab".into());
        // A drag reorders the TabView's own items; move one the same way and let the shell follow.
        self.move_item(1, 0);
        self.tabs_reordered();
        expect(
            self.current.get() == Some(0)
                && same_file(&self.pages.borrow()[0].path, &canonical_or_absolute(&other)),
            "move did not carry the current tab".into(),
        );
        self.move_item(0, 1);
        self.tabs_reordered();
        expect(self.current.get() == Some(1), "move back".into());
        self.key_with(0x57, true, false);
        expect(
            self.pages.borrow().len() == 1
                && self
                    .path
                    .borrow()
                    .as_deref()
                    .is_some_and(|p| same_file(p, &first)),
            "Ctrl+W closed the wrong tab".into(),
        );
        expect(
            self.current.get() == Some(0) && strip() == 1 && selected() == 0,
            format!("after close: strip {} selected {}", strip(), selected()),
        );
        let reopened = self.renderer.borrow().offset();
        expect(
            reopened == scrolled,
            format!("scroll after close {reopened} vs {scrolled}"),
        );
        self.key_with(0x57, true, true);
        expect(
            self.pages.borrow().is_empty() && strip() == 0,
            "Ctrl+Shift+W".into(),
        );
        expect(self.path.borrow().is_none(), "empty page still has a path".into());
        expect(
            self.renderer.borrow().block_count() > 0,
            "empty page shows nothing".into(),
        );
        let recent: Vec<PathBuf> = self.recent.borrow().paths().to_vec();
        expect(
            recent.len() == 2 && same_file(&recent[0], &first),
            format!("recent {recent:?}"),
        );
        let recent_items = self
            .recent_menu
            .borrow()
            .as_ref()
            .and_then(|m| m.Items().ok())
            .and_then(|i| i.Size().ok())
            .unwrap_or(0);
        expect(recent_items == 3, format!("Recent menu has {recent_items} items"));
        if failures.is_empty() {
            diag::trace(&format!(
                "tabs test ok: 2 tabs, scroll {scrolled:.0} kept, recent {}",
                recent.len()
            ));
            std::process::exit(0);
        }
        diag::trace(&format!("tabs test failed: {}", failures.join("; ")));
        std::process::exit(1);
    }

    /// Moves a tab in the TabView the way a drag does, keeping the current tab selected.
    fn move_item(&self, from: u32, to: u32) {
        let Ok(items) = self.tab_view.TabItems() else {
            return;
        };
        let Ok(item) = items.GetAt(from) else {
            return;
        };
        self.tabs_busy.set(true);
        _ = items.RemoveAt(from);
        _ = items.InsertAt(to, &item);
        let selected = self
            .current
            .get()
            .and_then(|i| self.pages.borrow().get(i).map(|p| p.item.clone()))
            .and_then(|current| {
                (0..items.Size().unwrap_or(0))
                    .find(|&i| items.GetAt(i).is_ok_and(|it| tabs::same_object(&it, &current)))
            });
        if let Some(selected) = selected {
            _ = tabs::set_selected_index(&self.tab_view, selected as i32);
        }
        self.tabs_busy.set(false);
    }

    /// Where a pointer event landed, in panel device px: XAML reports it in the panel's own logical
    /// coordinates, so only the DPI scale has to be applied.
    fn pointer_at(&self, args: &PointerRoutedEventArgs, source: &UIElement) -> Option<(f32, f32)> {
        let point = args.GetCurrentPoint(source).ok()?;
        let position = point.position().ok()?;
        let scale = self.renderer.borrow().device_scale();
        Some((position.x * scale, position.y * scale))
    }

    fn show(&self) {
        // The borrow ends before the match body, which scrolls and calls `show` again.
        let drawn = self.renderer.borrow_mut().draw();
        match drawn {
            Ok(true) => {
                if !self.presented.replace(true) {
                    diag::trace("first frame presented");
                    if self.select_test.get() {
                        self.select_test.set(false);
                        let range = self.renderer.borrow_mut().select_word("Markdown");
                        let rect = self.renderer.borrow_mut().selection_bounds();
                        diag::trace(&format!(
                            "select test: {range:?}, first rect {rect:?}, offset {:.0}",
                            self.renderer.borrow().offset()
                        ));
                        self.show();
                    }
                    if self.details_test.get() {
                        self.details_test.set(false);
                        let result = self.renderer.borrow_mut().details_test();
                        match result {
                            Some((block, before, after, open)) => diag::trace(&format!(
                                "details test ok: block {block} open={open}, {before:.0} -> {after:.0} px"
                            )),
                            None => diag::trace("details test failed: no summary or no change"),
                        }
                        self.show();
                    }
                    if self.find_test.get() {
                        self.find_test.set(false);
                        self.find_active.set(true);
                        self.renderer.borrow_mut().find_update("");
                        // Feed the same handlers the XAML key events call, so this covers the input
                        // path rather than calling the find API directly.
                        for byte in b"Markdown" {
                            self.character(*byte as usize);
                        }
                        let (hits, _, capped) = self.renderer.borrow().find_summary();
                        let marks = self.renderer.borrow_mut().find_rect_count(false);
                        self.key(0x72);
                        let (_, current, _) = self.renderer.borrow().find_summary();
                        let current_rects = self.renderer.borrow_mut().find_rect_count(true);
                        let ok = hits >= 2 && marks > 0 && current_rects > 0 && current == 1;
                        diag::trace(&format!(
                            "find test {}: {hits} matches, {marks} mark rects, \
                             {current_rects} current rects, index {current}, capped {capped}",
                            if ok { "ok" } else { "failed" }
                        ));
                        self.show();
                    }
                    if self.outline_test.get() {
                        self.outline_test.set(false);
                        let headings = self.renderer.borrow().headings();
                        let entries = self.outline_entries.get();
                        let target = headings
                            .iter()
                            .find(|(_, title, _)| title == "Diagrams")
                            .map(|(_, _, block)| *block);
                        let expected = target
                            .and_then(|block| self.renderer.borrow_mut().scroll_to_block(block));
                        let after = self.renderer.borrow().offset();
                        let ok = entries >= 5
                            && expected.is_some_and(|expected| (after - expected).abs() < 1.0);
                        diag::trace(&format!(
                            "outline test {}: {entries} headings, scrolled to {after:.0}, \
                             expected {expected:?}",
                            if ok { "ok" } else { "failed" }
                        ));
                        self.show();
                    }
                    if self.copy_test.get() {
                        self.copy_test.set(false);
                        let range = self.renderer.borrow_mut().select_word("Markdown");
                        self.copy_selection();
                        let copied = clipboard_text();
                        let ok = copied.as_deref() == Some("Markdown");
                        diag::trace(&format!(
                            "copy test {}: {range:?}, clipboard {copied:?}",
                            if ok { "ok" } else { "failed" }
                        ));
                        self.show();
                    }
                    if self.zoom_test.get() {
                        self.zoom_test.set(false);
                        let up = self.zoom_step(1);
                        let reset = self.zoom_step(0);
                        let mut low = reset;
                        for _ in 0..30 {
                            low = self.zoom_step(-1);
                        }
                        let mut high = low;
                        for _ in 0..40 {
                            high = self.zoom_step(1);
                        }
                        let ok = (up - 1.1).abs() < 1e-4
                            && (reset - 1.0).abs() < 1e-4
                            && (low - 0.6).abs() < 1e-4
                            && (high - 2.5).abs() < 1e-4;
                        diag::trace(&format!(
                            "zoom test {}: up {up:.1}, reset {reset:.1}, floor {low:.1}, ceiling {high:.1}",
                            if ok { "ok" } else { "failed" }
                        ));
                    }
                    if self.tabs_test.get() {
                        self.tabs_test.set(false);
                        self.run_tabs_check();
                    }
                    if diag::measuring() {
                        // One line and out, so a script can time the first frame from process start.
                        diag::measure_line(&format!(
                            "result=ok first_frame_ms={:.1}",
                            diag::since_process_start_ms()
                        ));
                        std::process::exit(0);
                    }
                }
                // `--scroll-steps`: walk the page down one wheel-sized step per frame, so a
                // screenshot shows a view reached by scrolling rather than by a fresh layout.
                if self.steps.get() > 0 {
                    self.steps.set(self.steps.get() - 1);
                    self.renderer.borrow_mut().scroll_by(40.0);
                    self.show();
                }
            }
            Ok(false) => {}
            // Nothing has been shown yet, so without this the window stays empty with no message.
            Err(e) if !self.presented.get() => diag::fatal("first draw", &e),
            Err(e) => diag::fail("draw", &e),
        }
    }
}

fn same_file(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn read_document(path: &Path) -> std::result::Result<mdview_core::Document, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let started = std::time::Instant::now();
    let document = parse_bytes(&bytes);
    diag::trace(&format!(
        "parsed {}: {} blocks, {} spans, {} warnings in {:.1} ms",
        path.display(),
        document.blocks.len(),
        document.spans.len(),
        document.warnings.len(),
        started.elapsed().as_secs_f64() * 1000.0
    ));
    Ok(document)
}

/// Every 800 ms: open files later launches sent, and reload the showing file when it changed on
/// disk (keeping the scroll offset).
unsafe extern "system" fn poll_timer(_: *mut c_void, _: u32, _: usize, _: u32) {
    if let Some(shell) = SHELL.with(|cell| cell.borrow().clone()) {
        let sent: Vec<PathBuf> = INCOMING.with(|cell| {
            cell.borrow()
                .as_ref()
                .map(|r| r.try_iter().collect())
                .unwrap_or_default()
        });
        if !sent.is_empty() {
            // SW_RESTORE = 9; Windows may only flash the taskbar button, which is still a signal.
            let hwnd = winui_window() as *mut c_void;
            unsafe {
                ShowWindow(hwnd, 9);
                SetForegroundWindow(hwnd);
            }
        }
        for path in sent {
            if shell.open(&path) {
                shell.show();
            }
        }
        shell.poll_file();
    }
}

/// File > Reload: reads the current tab's file again, in place.
fn reload(shell: &Rc<Shell>) {
    let Some(path) = shell.path.borrow().clone() else {
        return;
    };
    match read_document(&path) {
        Ok(document) => {
            shell.stamp.set(mdview_core::files::stamp(&path));
            shell
                .renderer
                .borrow_mut()
                .set_document(document, path.parent().map(Path::to_path_buf));
            refresh_outline(shell);
            shell.show();
        }
        Err(e) => diag::error_box(&e),
    }
}

/// Rebuilds the Recent menu: entries (missing ones marked, and removed when clicked), a disabled
/// line when there are none, and Clear recent files.
fn refresh_recent(shell: &Shell) {
    let Some(menu) = shell.recent_menu.borrow().clone() else {
        return;
    };
    let Some(target) = SHELL.with(|cell| cell.borrow().clone()) else {
        return;
    };
    let Ok(items) = menu.Items() else {
        return;
    };
    // `Clear` on this collection has been seen to leave the old entries, so remove them one by one.
    while items.Size().unwrap_or(0) > 0 {
        if items.RemoveAt(0).is_err() {
            break;
        }
    }
    let add = |text: &str, click: Option<Box<dyn Fn()>>| {
        let Ok(item) = MenuFlyoutItem::new() else {
            return None;
        };
        _ = item.SetText(text);
        match click {
            Some(click) => {
                if let Ok(revoker) = item.Click(move |_, _| click()) {
                    revoker.forget();
                }
            }
            None => _ = tabs::set_enabled(&item, false),
        }
        if let Ok(base) = item.cast::<MenuFlyoutItemBase>() {
            _ = items.Append(&base);
        }
        Some(item)
    };
    let entries = shell.recent.borrow().entries();
    if entries.is_empty() {
        add(strings::RECENT_EMPTY, None);
    }
    for entry in entries {
        let s = target.clone();
        let path = entry.path.clone();
        let missing = entry.missing;
        let item = add(
            &entry.label,
            Some(Box::new(move || {
                if missing {
                    s.recent.borrow_mut().remove(&path);
                    s.save_session();
                    s.message(strings::RECENT_REMOVED_MISSING);
                } else {
                    s.open(&path);
                }
                s.focus_page();
            })),
        );
        if let Some(item) = item {
            _ = tabs::set_tooltip(&item, &entry.path.display().to_string());
        }
    }
    let s = target.clone();
    add(
        "Clear recent files",
        Some(Box::new(move || {
            s.recent.borrow_mut().clear();
            s.save_session();
            s.message(strings::RECENT_CLEARED);
        })),
    );
}

/// Clipboard formats used by the Win32 fallback; WinRT clipboard needs bindings this shell trims.
const CF_UNICODETEXT: u32 = 13;
const GMEM_MOVEABLE: u32 = 2;

fn set_clipboard_text(text: &str) -> bool {
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return false;
        }
        _ = EmptyClipboard();
        let handle = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2);
        if handle.is_null() {
            _ = CloseClipboard();
            return false;
        }
        let target = GlobalLock(handle);
        if target.is_null() {
            _ = GlobalFree(handle);
            _ = CloseClipboard();
            return false;
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr(), target.cast::<u16>(), wide.len());
        _ = GlobalUnlock(handle);
        if SetClipboardData(CF_UNICODETEXT, handle).is_null() {
            _ = GlobalFree(handle);
            _ = CloseClipboard();
            return false;
        }
        _ = CloseClipboard();
        true
    }
}

fn clipboard_text() -> Option<String> {
    unsafe {
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return None;
        }
        let handle = GetClipboardData(CF_UNICODETEXT);
        if handle.is_null() {
            _ = CloseClipboard();
            return None;
        }
        let source = GlobalLock(handle).cast::<u16>();
        if source.is_null() {
            _ = CloseClipboard();
            return None;
        }
        let mut len = 0;
        while *source.add(len) != 0 {
            len += 1;
        }
        let text = String::from_utf16_lossy(std::slice::from_raw_parts(source, len));
        _ = GlobalUnlock(handle);
        _ = CloseClipboard();
        Some(text)
    }
}

/// Points WinUI's own chrome (the menu labels) at the same light or dark theme as the page, so the
/// menu text is not white on a white bar.
fn set_chrome_theme(root: &Grid, theme: mdview_render::Theme) -> Result<()> {
    let element: FrameworkElement = root.cast()?;
    element.set_requested_theme(if theme.name == "dark" { 2 } else { 1 })?;
    Ok(())
}

/// The menu row sits on the window background, so paint it the page's colour.
fn set_root_background(root: &Grid, theme: mdview_render::Theme) -> Result<()> {
    let brush = SolidColorBrush::new()?;
    let [r, g, b] = theme.bg;
    brush.SetColor(Color { a: 255, r, g, b })?;
    root.cast::<IPanel>()?
        .SetBackground(&brush.cast::<Brush>()?)?;
    Ok(())
}

/// Rebuilds the Outline menu from the renderer's headings. Called after a document loads, because
/// the menu is created once but its items depend on the document.
fn refresh_outline(shell: &Shell) {
    let Some(menu) = shell.outline_menu.borrow().clone() else {
        return;
    };
    let Some(target) = SHELL.with(|cell| cell.borrow().clone()) else {
        return;
    };
    let Ok(items) = menu.Items() else {
        return;
    };
    _ = items.Clear();
    let headings = shell.renderer.borrow().headings();
    for (level, title, block) in &headings {
        let Ok(item) = MenuFlyoutItem::new() else {
            continue;
        };
        let indent = "    ".repeat(level.saturating_sub(1) as usize);
        if item.SetText(&format!("{indent}{title}")).is_err() {
            continue;
        }
        let target = target.clone();
        let block = *block;
        if item
            .Click(move |_, _| {
                target.renderer.borrow_mut().scroll_to_block(block);
                target.show();
            })
            .is_err()
        {
            continue;
        }
        if let Ok(base) = item.cast::<MenuFlyoutItemBase>() {
            _ = items.Append(&base);
        }
    }
    shell.outline_entries.set(headings.len());
}

fn build_menu(shell: &Rc<Shell>) -> Result<()> {
    let menu = MenuBar::new()?;
    let file = MenuBarItem::new()?;
    file.SetTitle("File")?;

    let open = MenuFlyoutItem::new()?;
    open.SetText("Open...\tCtrl+O")?;
    let target = shell.clone();
    open.Click(move |_, _| target.open_dialog())?.forget();
    file.Items()?.Append(&open.cast::<MenuFlyoutItemBase>()?)?;

    let close_tab = MenuFlyoutItem::new()?;
    close_tab.SetText("Close tab\tCtrl+W")?;
    let target = shell.clone();
    close_tab
        .Click(move |_, _| {
            if let Some(current) = target.current.get() {
                target.close_tab(current);
            }
        })?
        .forget();
    file.Items()?
        .Append(&close_tab.cast::<MenuFlyoutItemBase>()?)?;

    let close_all = MenuFlyoutItem::new()?;
    close_all.SetText("Close all tabs\tCtrl+Shift+W")?;
    let target = shell.clone();
    close_all.Click(move |_, _| target.close_all())?.forget();
    file.Items()?
        .Append(&close_all.cast::<MenuFlyoutItemBase>()?)?;

    let refresh = MenuFlyoutItem::new()?;
    refresh.SetText("Reload")?;
    let target = shell.clone();
    refresh.Click(move |_, _| reload(&target))?.forget();
    file.Items()?
        .Append(&refresh.cast::<MenuFlyoutItemBase>()?)?;

    let close = MenuFlyoutItem::new()?;
    close.SetText("Exit")?;
    let window = shell.window.clone();
    close
        .Click(move |_, _| {
            _ = window.Close();
        })?
        .forget();
    file.Items()?.Append(&close.cast::<MenuFlyoutItemBase>()?)?;
    menu.Items()?.Append(&file)?;

    let recent = MenuBarItem::new()?;
    recent.SetTitle("Recent")?;
    menu.Items()?.Append(&recent)?;
    *shell.recent_menu.borrow_mut() = Some(recent);

    let edit = MenuBarItem::new()?;
    edit.SetTitle("Edit")?;
    let find = MenuFlyoutItem::new()?;
    find.SetText("Find...\tCtrl+F")?;
    let target = shell.clone();
    find.Click(move |_, _| target.find_open())?.forget();
    edit.Items()?.Append(&find.cast::<MenuFlyoutItemBase>()?)?;
    let next = MenuFlyoutItem::new()?;
    next.SetText("Find Next\tF3")?;
    let target = shell.clone();
    next.Click(move |_, _| target.find_step(1))?.forget();
    edit.Items()?.Append(&next.cast::<MenuFlyoutItemBase>()?)?;
    let previous = MenuFlyoutItem::new()?;
    previous.SetText("Find Previous\tShift+F3")?;
    let target = shell.clone();
    previous.Click(move |_, _| target.find_step(-1))?.forget();
    edit.Items()?
        .Append(&previous.cast::<MenuFlyoutItemBase>()?)?;
    let copy = MenuFlyoutItem::new()?;
    copy.SetText("Copy\tCtrl+C")?;
    let target = shell.clone();
    copy.Click(move |_, _| target.copy_selection())?.forget();
    edit.Items()?.Append(&copy.cast::<MenuFlyoutItemBase>()?)?;
    menu.Items()?.Append(&edit)?;

    let outline = MenuBarItem::new()?;
    outline.SetTitle("Outline")?;
    menu.Items()?.Append(&outline)?;
    *shell.outline_menu.borrow_mut() = Some(outline);

    let view = MenuBarItem::new()?;
    view.SetTitle("View")?;
    for (title, theme) in [("Light theme", LIGHT), ("Dark theme", DARK)] {
        let item = MenuFlyoutItem::new()?;
        item.SetText(title)?;
        let target = shell.clone();
        item.Click(move |_, _| {
            target.renderer.borrow_mut().set_theme(theme);
            _ = set_root_background(&target.root, theme);
            _ = set_chrome_theme(&target.root, theme);
            if let Some((_, session)) = target.session.borrow_mut().as_mut() {
                session.theme = if theme.name == DARK.name {
                    mdview_core::session::Theme::Dark
                } else {
                    mdview_core::session::Theme::Light
                };
            }
            target.save_session();
            target.show();
        })?
        .forget();
        view.Items()?.Append(&item.cast::<MenuFlyoutItemBase>()?)?;
    }
    menu.Items()?.Append(&view)?;

    place(&shell.root, &menu, 0, 0)?;
    Grid::SetColumnSpan(&menu.cast::<FrameworkElement>()?, 2)?;
    refresh_outline(shell);
    refresh_recent(shell);
    Ok(())
}

/// Shown when there is no file argument, so an empty launch still shows the renderer working.
const WELCOME: &str = "\
# mdview

A Markdown viewer with its own renderer: Rust, WinUI 3 on Windows.

**File > Open** opens a document. The wheel, the scrollbar, and Home/End, the arrows,
Page Up/Down and Space scroll.

- Headings, lists, tables, alerts and code blocks are drawn by `mdview-render`.
- HTML is mapped, not laid out by a browser.
- Nothing here is a webview.
";

/// Options that exist for screenshots and tests rather than for users.
#[derive(Clone, Copy)]
pub struct Startup {
    /// Let a scroll reuse the previous frame on the GPU; off for `--no-shift`.
    pub shift: bool,
    /// Scroll here once the page is laid out, for `--scroll`.
    pub scroll: f32,
    /// Scroll this many 40 px steps after the first frame, for `--scroll-steps`.
    pub steps: u32,
    /// Logical window size, for `--size`.
    pub size: (i32, i32),
    /// Select a word after the first frame, for `--select-test`.
    pub select: bool,
    /// Toggle the first `<details>` after the first frame, for `--details-test`.
    pub details: bool,
    /// Run the find check after the first frame, for `--find-test`.
    pub find: bool,
    /// Run the outline check after the first frame, for `--outline-test`.
    pub outline: bool,
    /// Run the clipboard check after the first frame, for `--copy-test`.
    pub copy: bool,
    /// Run the tabs check after the first frame, for `--tabs-test`.
    pub tabs: bool,
    /// Run the zoom check after the first frame, for `--zoom-test`.
    pub zoom: bool,
}

pub fn build(
    gpu: JoinHandle<Result<Gpu>>,
    files: Vec<PathBuf>,
    settings: Option<PathBuf>,
    theme: Option<mdview_render::Theme>,
    zoom: f32,
    startup: Startup,
) -> Result<()> {
    let theme = theme.unwrap_or_else(system_theme);
    diag::trace(&format!("theme: {}", theme.name));
    diag::trace(&format!(
        "startup: shift={} scroll={} steps={}",
        startup.shift, startup.scroll, startup.steps
    ));
    let window = Window::new()?;
    diag::trace("Window created");
    window.SetTitle("mdview")?;
    set_window_size(startup.size.0, startup.size.1);

    let root = Grid::new()?;
    add_row(&root, GridUnitType::Auto, 1.0)?; // menu
    add_row(&root, GridUnitType::Auto, 1.0)?; // tabs
    add_row(&root, GridUnitType::Star, 1.0)?; // page
    set_root_background(&root, theme)?;
    set_chrome_theme(&root, theme)?;

    let panel = SwapChainPanel::new()?;
    // SwapChainPanel rejects Background. A transparent child is hit-testable, and its pointer
    // events bubble to the panel's wheel handler below.
    let brush = SolidColorBrush::new()?;
    brush.SetColor(Color {
        a: 0,
        r: 0,
        g: 0,
        b: 0,
    })?;
    let hit_target = Grid::new()?;
    hit_target
        .cast::<IPanel>()?
        .SetBackground(&brush.cast::<Brush>()?)?;
    panel
        .cast::<IPanel>()?
        .Children()?
        .Append(&hit_target.cast::<UIElement>()?)?;
    place(&root, &panel, 2, 0)?;

    // Headers only: no tab content, so the one panel below draws whichever page is current.
    let tab_view = TabView::new()?;
    tabs::set_add_tab_button_visible(&tab_view, false)?;
    tabs::set_can_reorder_tabs(&tab_view, true)?;
    tabs::set_visible(&tab_view.cast::<UIElement>()?, false)?;
    place(&root, &tab_view, 1, 0)?;

    let gpu = gpu
        .join()
        .map_err(|_| Error::new(HRESULT(0x8000_FFFFu32 as i32), "device thread panicked"))??;
    let mut renderer = Renderer::new(gpu, panel.clone());
    renderer.set_theme(theme);
    renderer.set_zoom(zoom);
    renderer.set_shift(startup.shift);
    renderer.set_pending_scroll(startup.scroll);
    let session = settings.map(|path| {
        let saved = Session::load(&path);
        diag::trace(&format!(
            "session {}: {} tabs, {} recent",
            path.display(),
            saved.tabs.len(),
            saved.recent.paths().len()
        ));
        (path, saved)
    });
    let shell = Rc::new(Shell {
        window: window.clone(),
        root: root.clone(),
        renderer: RefCell::new(renderer),
        path: RefCell::new(None),
        tab_view: tab_view.clone(),
        page: panel.cast()?,
        pages: RefCell::new(Vec::new()),
        current: Cell::new(None),
        tabs_busy: Cell::new(false),
        recent: RefCell::new(Recents::default()),
        session: RefCell::new(session),
        recent_menu: RefCell::new(None),
        tabs_test: Cell::new(startup.tabs),
        stamp: Cell::new(mdview_core::files::FileStamp::Missing),
        zoom_test: Cell::new(startup.zoom),
        presented: Cell::new(false),
        dragging: Cell::new(None),
        steps: Cell::new(startup.steps),
        selecting: Cell::new(false),
        selection_anchor: Cell::new(None),
        pressed_link: Cell::new(None),
        pressed_footnote: Cell::new(None),
        pressed_details: Cell::new(None),
        selection_moved: Cell::new(false),
        select_test: Cell::new(startup.select),
        details_test: Cell::new(startup.details),
        find_active: Cell::new(false),
        find_test: Cell::new(startup.find),
        outline_test: Cell::new(startup.outline),
        copy_test: Cell::new(startup.copy),
        outline_menu: RefCell::new(None),
        outline_entries: Cell::new(0),
    });
    let framework: FrameworkElement = panel.cast()?;
    {
        let shell = shell.clone();
        framework.SizeChanged(move |_, _| shell.show())?.forget();
    }
    {
        let shell = shell.clone();
        panel
            .CompositionScaleChanged(move |_, _| shell.show())?
            .forget();
    }
    {
        let shell = shell.clone();
        let element: UIElement = panel.cast()?;
        let source = element.clone();
        element
            .PointerWheelChanged(move |_, args| {
                let Some(args) = args.as_ref() else { return };
                let delta = args
                    .GetCurrentPoint(&source)
                    .and_then(|point| point.Properties())
                    .and_then(|properties| properties.MouseWheelDelta())
                    .unwrap_or(0);
                _ = args.SetHandled(true);
                diag::trace(&format!("wheel delta={delta}"));
                shell
                    .renderer
                    .borrow_mut()
                    .scroll_by(-(delta as f32) * 0.75);
                shell.show();
            })?
            .forget();
    }
    // The scrollbar: a press in the gutter jumps there and starts a drag, and the drag follows the
    // pointer. XAML delivers these to the panel's own hit target, which is why the shell does not
    // have to look at window messages for the mouse at all.
    {
        let shell = shell.clone();
        let element: UIElement = panel.cast()?;
        let source = element.clone();
        element
            .pointer_pressed(move |_, args| {
                let Some(args) = args.as_ref() else { return };
                // A press on the page gives it the keyboard back, from the menu bar for instance.
                _ = crate::input::focus(&source, crate::input::FOCUS_PROGRAMMATIC);
                let Some((x, y)) = shell.pointer_at(args, &source) else {
                    return;
                };
                let Some(bar) = shell.renderer.borrow().bar() else {
                    return;
                };
                if x < bar.x {
                    // The page: a press starts a selection, and a link or footnote is followed on
                    // release if the pointer never moved.
                    let hit = shell.renderer.borrow_mut().hit_test(x, y);
                    if let Some(hit) = hit {
                        _ = args.SetHandled(true);
                        shell.selection_anchor.set(Some(hit.offset));
                        shell
                            .renderer
                            .borrow_mut()
                            .set_selection(Some((hit.offset, hit.offset)));
                        shell.pressed_link.set(hit.link);
                        shell.pressed_footnote.set(hit.footnote);
                        shell.pressed_details.set(hit.details);
                        shell.selection_moved.set(false);
                        shell.selecting.set(true);
                        diag::trace(&format!(
                            "select press at {x:.0},{y:.0}: offset {}",
                            hit.offset
                        ));
                    }
                    return;
                }
                _ = args.SetHandled(true);
                let grab = shell.renderer.borrow_mut().press_on_bar(&bar, y);
                shell.dragging.set(Some(grab));
                diag::trace(&format!("bar press at {x:.0},{y:.0}"));
                shell.show();
            })?
            .forget();
    }
    {
        let shell = shell.clone();
        let element: UIElement = panel.cast()?;
        let source = element.clone();
        element
            .pointer_moved(move |_, args| {
                let Some(args) = args.as_ref() else { return };
                let Some((x, y)) = shell.pointer_at(args, &source) else {
                    return;
                };
                if shell.selecting.get() {
                    let Some(hit) = shell.renderer.borrow_mut().hit_test(x, y) else {
                        return;
                    };
                    let anchor = shell.selection_anchor.get().unwrap_or(hit.offset);
                    if hit.offset != anchor {
                        shell.selection_moved.set(true);
                    }
                    _ = args.SetHandled(true);
                    shell
                        .renderer
                        .borrow_mut()
                        .set_selection(Some((anchor, hit.offset)));
                    shell.show();
                    return;
                }
                let Some(grab) = shell.dragging.get() else {
                    return;
                };
                let Some(bar) = shell.renderer.borrow().bar() else {
                    return;
                };
                _ = args.SetHandled(true);
                shell.renderer.borrow_mut().drag_to(&bar, y, grab);
                shell.show();
            })?
            .forget();
    }
    {
        let shell = shell.clone();
        let element: UIElement = panel.cast()?;
        element
            .pointer_released(move |_, args| {
                if let Some(args) = args.as_ref() {
                    _ = args.SetHandled(true);
                }
                if shell.selecting.replace(false) {
                    if !shell.selection_moved.get() {
                        let link = shell.pressed_link.take();
                        let footnote = shell.pressed_footnote.take();
                        let details = shell.pressed_details.take();
                        if link.is_some() || footnote.is_some() {
                            shell.follow(link, footnote);
                        } else if let Some(details) = details {
                            if let Some(open) = shell.renderer.borrow_mut().toggle_details(details)
                            {
                                diag::trace(&format!("details {details} open={open}"));
                                shell.show();
                            }
                        } else {
                            shell.renderer.borrow_mut().set_selection(None);
                            shell.show();
                        }
                    } else {
                        shell.pressed_link.set(None);
                        shell.pressed_footnote.set(None);
                        shell.pressed_details.set(None);
                        diag::trace("selection released");
                    }
                }
                shell.dragging.set(None);
            })?
            .forget();
    }

    SHELL.with(|cell| *cell.borrow_mut() = Some(shell.clone()));
    // The poll runs on this thread's message loop; a null window means no HWND is needed.
    unsafe { SetTimer(std::ptr::null_mut(), 0, 800, poll_timer) };
    build_menu(&shell)?;
    {
        let target = shell.clone();
        tabs::selection_changed(&tab_view, move || target.tab_selected())?.forget();
        let target = shell.clone();
        tabs::tab_close_requested(&tab_view, move |_, args| {
            if let Some(tab) = args.as_ref().and_then(|args| args.tab().ok()) {
                target.tab_close_requested(&tab);
            }
        })?
        .forget();
        let target = shell.clone();
        tabs::tab_drag_completed(&tab_view, move |_, _| target.tabs_reordered())?.forget();
    }

    // Saved tabs come back first, without reordering Recent; files on the command line follow.
    let saved = shell.session.borrow().as_ref().map(|(_, s)| s.clone());
    if let Some(saved) = saved {
        *shell.recent.borrow_mut() = saved.recent.clone();
        for path in &saved.tabs {
            shell.restore_page(path);
        }
        let count = shell.pages.borrow().len();
        if count > 0 {
            shell.switch_to(saved.active.min(count - 1));
        }
        refresh_recent(&shell);
    }
    for path in &files {
        shell.open(path);
    }
    if shell.pages.borrow().is_empty() {
        shell.show_empty(strings::STATUS_ALL_CLOSED);
    }

    window.SetContent(&root.cast::<UIElement>()?)?;
    window.Closed(|_, _| diag::trace("window closed"))?.forget();
    window.Activate()?;
    diag::trace("Window activated");

    // Keys come from XAML's routed events on the root. KeyDown bubbles from whatever has focus, and
    // a control that used a key marks it handled, so the menu bar keeps arrows, Space and Escape.
    // The page panel is a tab stop and takes focus at start and on every press, so the keys reach
    // the root from it when the user is reading.
    let root_element: UIElement = root.cast()?;
    {
        let shell = shell.clone();
        // Shortcuts work from anywhere, including the menu bar. Ctrl+F moves focus to the page, so
        // the query, Enter and Escape that follow go to find rather than to a menu item.
        let page: UIElement = panel.cast()?;
        crate::input::key_down(&root_element, move |_, args| {
            let Some(args) = args.as_ref() else { return };
            let ctrl = unsafe { GetKeyState(0x11) } < 0;
            let Ok(key) = args.key() else { return };
            let find_escape = key == 0x1b && shell.find_active.get();
            if (ctrl || key == 0x72 || key == 0x74 || find_escape) && shell.key(key as usize) {
                _ = args.set_handled(true);
                if ctrl && key == 0x46 {
                    _ = crate::input::focus(&page, crate::input::FOCUS_PROGRAMMATIC);
                }
            }
        })?
        .forget();
    }
    {
        // Everything else only while the page has focus: an open menu does not mark every key it
        // uses as handled, and its Down must not also scroll the page.
        let shell = shell.clone();
        crate::input::key_down(&panel.cast::<UIElement>()?, move |_, args| {
            let Some(args) = args.as_ref() else { return };
            if let Ok(key) = args.key()
                && shell.key(key as usize)
            {
                _ = args.set_handled(true);
            }
        })?
        .forget();
    }
    {
        let shell = shell.clone();
        crate::input::character_received(&root_element, move |_, args| {
            let Some(args) = args.as_ref() else { return };
            if let Ok(unit) = args.character()
                && shell.character(unit as usize)
            {
                _ = args.set_handled(true);
            }
        })?
        .forget();
    }
    let panel_element: UIElement = panel.cast()?;
    crate::input::set_is_tab_stop(&panel_element, true)?;
    // Focus only works once the panel is in the live tree.
    {
        let target = panel_element.clone();
        panel
            .cast::<FrameworkElement>()?
            .Loaded(move |_, _| {
                let focused = crate::input::focus(&target, crate::input::FOCUS_PROGRAMMATIC);
                diag::trace(&format!("page focus on load: {focused:?}"));
            })?
            .forget();
    }
    Ok(())
}
