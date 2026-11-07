//! The window: a menu bar over one `SwapChainPanel` that shows the laid-out page, with the wheel
//! scrolling it. Text selection, links and the tree come later.
use crate::bindings::*;
use crate::diag;
use crate::render::{Gpu, Renderer};
use mdview_core::parse_bytes;
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
windows_core::link!("user32.dll" "system" fn GetClassNameW(window: *mut c_void, name: *mut u16, capacity: i32) -> i32);
windows_core::link!("user32.dll" "system" fn GetDpiForWindow(window: *mut c_void) -> u32);
windows_core::link!("user32.dll" "system" fn SetWindowPos(window: *mut c_void, after: *mut c_void, x: i32, y: i32, cx: i32, cy: i32, flags: u32) -> i32);
windows_core::link!("comdlg32.dll" "system" fn GetOpenFileNameW(ofn: *mut OpenFileNameW) -> i32);
windows_core::link!("advapi32.dll" "system" fn RegGetValueW(hkey: *mut c_void, subkey: PCWSTR, value: PCWSTR, flags: u32, ty: *mut u32, data: *mut c_void, size: *mut u32) -> i32);

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

unsafe extern "system" fn size_if_winui(window: *mut c_void, _: isize) -> i32 {
    const SWP_NOMOVE_NOZORDER_NOACTIVATE: u32 = 0x0002 | 0x0004 | 0x0010;
    let mut name = [0u16; 64];
    let length = unsafe { GetClassNameW(window, name.as_mut_ptr(), name.len() as i32) };
    if String::from_utf16_lossy(&name[..length.max(0) as usize]) != WINDOW_CLASS {
        return 1;
    }
    let dpi = unsafe { GetDpiForWindow(window) } as i32;
    let (width, height) = (1100 * dpi / 96, 900 * dpi / 96);
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

fn set_window_size() {
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

struct Shell {
    window: Window,
    root: Grid,
    renderer: RefCell<Renderer>,
    path: RefCell<Option<PathBuf>>,
    presented: Cell<bool>,
}

impl Shell {
    fn show(&self) {
        match self.renderer.borrow_mut().draw() {
            Ok(true) => {
                if !self.presented.replace(true) {
                    diag::trace("first frame presented");
                }
            }
            Ok(false) => {}
            // Nothing has been shown yet, so without this the window stays empty with no message.
            Err(e) if !self.presented.get() => diag::fatal("first draw", &e),
            Err(e) => diag::fail("draw", &e),
        }
    }
}

fn load(shell: &Rc<Shell>, path: &Path) -> std::result::Result<(), String> {
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
    shell.renderer.borrow_mut().set_document(document);
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    shell
        .window
        .SetTitle(&format!("mdview - {name}"))
        .map_err(|e| e.to_string())?;
    *shell.path.borrow_mut() = Some(path.to_path_buf());
    Ok(())
}

fn open_file(shell: &Rc<Shell>) {
    let Some(path) = file_dialog() else {
        return;
    };
    match load(shell, &path) {
        Ok(()) => shell.show(),
        Err(e) => diag::error_box(&e),
    }
}

fn reload(shell: &Rc<Shell>) {
    let Some(path) = shell.path.borrow().clone() else {
        return;
    };
    match load(shell, &path) {
        Ok(()) => shell.show(),
        Err(e) => diag::error_box(&e),
    }
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

fn build_menu(shell: &Rc<Shell>) -> Result<()> {
    let menu = MenuBar::new()?;
    let file = MenuBarItem::new()?;
    file.SetTitle("File")?;

    let open = MenuFlyoutItem::new()?;
    open.SetText("Open...")?;
    let target = shell.clone();
    open.Click(move |_, _| open_file(&target))?.forget();
    file.Items()?.Append(&open.cast::<MenuFlyoutItemBase>()?)?;

    let refresh = MenuFlyoutItem::new()?;
    refresh.SetText("Reload")?;
    let target = shell.clone();
    refresh.Click(move |_, _| reload(&target))?.forget();
    file.Items()?
        .Append(&refresh.cast::<MenuFlyoutItemBase>()?)?;

    let close = MenuFlyoutItem::new()?;
    close.SetText("Close")?;
    let window = shell.window.clone();
    close
        .Click(move |_, _| {
            _ = window.Close();
        })?
        .forget();
    file.Items()?.Append(&close.cast::<MenuFlyoutItemBase>()?)?;
    menu.Items()?.Append(&file)?;

    let view = MenuBarItem::new()?;
    view.SetTitle("View")?;
    for (title, theme) in [("Light theme", LIGHT), ("Dark theme", DARK)] {
        let item = MenuFlyoutItem::new()?;
        item.SetText(title)?;
        let target = shell.clone();
        item.Click(move |_, _| {
            target.renderer.borrow_mut().set_theme(theme);
            _ = set_root_background(&target.root, theme);
            target.show();
        })?
        .forget();
        view.Items()?.Append(&item.cast::<MenuFlyoutItemBase>()?)?;
    }
    menu.Items()?.Append(&view)?;

    place(&shell.root, &menu, 0, 0)?;
    Grid::SetColumnSpan(&menu.cast::<FrameworkElement>()?, 2)?;
    Ok(())
}

/// Shown when there is no file argument, so an empty launch still shows the renderer working.
const WELCOME: &str = "\
# mdview

A Markdown viewer with its own renderer: Rust, WinUI 3 on Windows.

**File > Open** opens a document. The wheel scrolls.

- Headings, lists, tables, alerts and code blocks are drawn by `mdview-render`.
- HTML is mapped, not laid out by a browser.
- Nothing here is a webview.
";

pub fn build(
    gpu: JoinHandle<Result<Gpu>>,
    file: Option<PathBuf>,
    theme: Option<mdview_render::Theme>,
) -> Result<()> {
    let theme = theme.unwrap_or_else(system_theme);
    diag::trace(&format!("theme: {}", theme.name));
    let window = Window::new()?;
    diag::trace("Window created");
    window.SetTitle("mdview")?;
    set_window_size();

    let root = Grid::new()?;
    add_row(&root, GridUnitType::Auto, 1.0)?; // menu
    add_row(&root, GridUnitType::Star, 1.0)?; // page
    set_root_background(&root, theme)?;

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
    place(&root, &panel, 1, 0)?;

    let gpu = gpu
        .join()
        .map_err(|_| Error::new(HRESULT(0x8000_FFFFu32 as i32), "device thread panicked"))??;
    let mut renderer = Renderer::new(gpu, panel.clone());
    renderer.set_theme(theme);
    if file.is_none() {
        renderer.set_document(parse_bytes(WELCOME.as_bytes()));
    }
    let shell = Rc::new(Shell {
        window: window.clone(),
        root: root.clone(),
        renderer: RefCell::new(renderer),
        path: RefCell::new(file.clone()),
        presented: Cell::new(false),
    });
    if let Some(path) = &file
        && let Err(e) = load(&shell, path)
    {
        diag::error_box(&e);
    }

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

    build_menu(&shell)?;
    window.SetContent(&root.cast::<UIElement>()?)?;
    window.Closed(|_, _| diag::trace("window closed"))?.forget();
    window.Activate()?;
    diag::trace("Window activated");
    Ok(())
}
