//! The window. Tabs and pixels go in the first frame; menu, tree and find box follow it.
use crate::bindings::*;
use crate::diag;
use crate::render::{Gpu, Renderer};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::thread::JoinHandle;
use windows_core::*;
use windows_reference::IReference;

fn boxed(text: &str) -> Result<IInspectable> {
    IReference::<HSTRING>::from(text).cast()
}

fn add_row(grid: &Grid, unit: GridUnitType, value: f64) -> Result<()> {
    let row = RowDefinition::new()?;
    row.SetHeight(GridLength {
        value,
        grid_unit_type: unit,
    })?;
    grid.RowDefinitions()?.Append(&row)
}

fn add_column(grid: &Grid, unit: GridUnitType, value: f64) -> Result<()> {
    let column = ColumnDefinition::new()?;
    column.SetWidth(GridLength {
        value,
        grid_unit_type: unit,
    })?;
    grid.ColumnDefinitions()?.Append(&column)
}

fn place(panel: &Grid, child: &impl Interface, row: i32, column: i32) -> Result<()> {
    let element: FrameworkElement = child.cast()?;
    Grid::SetRow(&element, row)?;
    Grid::SetColumn(&element, column)?;
    panel.cast::<IPanel>()?.Children()?.Append(&child.cast::<UIElement>()?)
}

windows_core::link!("kernel32.dll" "system" fn GetCurrentThreadId() -> u32);
windows_core::link!("user32.dll" "system" fn EnumThreadWindows(thread: u32, callback: unsafe extern "system" fn(*mut core::ffi::c_void, isize) -> i32, param: isize) -> i32);
windows_core::link!("user32.dll" "system" fn GetClassNameW(window: *mut core::ffi::c_void, name: *mut u16, capacity: i32) -> i32);
windows_core::link!("user32.dll" "system" fn GetDpiForWindow(window: *mut core::ffi::c_void) -> u32);
windows_core::link!("user32.dll" "system" fn SetWindowPos(window: *mut core::ffi::c_void, after: *mut core::ffi::c_void, x: i32, y: i32, cx: i32, cy: i32, flags: u32) -> i32);

unsafe extern "system" fn size_if_winui(window: *mut core::ffi::c_void, _: isize) -> i32 {
    const SWP_NOMOVE_NOZORDER_NOACTIVATE: u32 = 0x0002 | 0x0004 | 0x0010;
    let mut name = [0u16; 64];
    let length = unsafe { GetClassNameW(window, name.as_mut_ptr(), name.len() as i32) };
    if String::from_utf16_lossy(&name[..length.max(0) as usize]) != "WinUIDesktopWin32WindowClass" {
        return 1;
    }
    let dpi = unsafe { GetDpiForWindow(window) } as i32;
    let (width, height) = (1220 * dpi / 96, 820 * dpi / 96);
    let moved = unsafe {
        SetWindowPos(window, std::ptr::null_mut(), 0, 0, width, height, SWP_NOMOVE_NOZORDER_NOACTIVATE)
    };
    diag::trace(&format!("window sized {width}x{height} at dpi {dpi}, ok={moved}"));
    0
}

/// Launch figures only compare between PCs at one window size, so this uses the budget's.
fn set_budget_size() {
    unsafe { EnumThreadWindows(GetCurrentThreadId(), size_if_winui, 0) };
}

struct Ui {
    window: Window,
    root: Grid,
    renderer: RefCell<Renderer>,
    presented: Cell<bool>,
}

pub fn build(gpu: JoinHandle<Result<Gpu>>) -> Result<()> {
    let window = Window::new()?;
    diag::trace("Window created");
    window.SetTitle("mdview WinUI skeleton")?;
    set_budget_size();

    let root = Grid::new()?;
    add_row(&root, GridUnitType::Auto, 1.0)?; // menu
    add_row(&root, GridUnitType::Auto, 1.0)?; // tabs
    add_row(&root, GridUnitType::Star, 1.0)?; // tree and pixels
    add_row(&root, GridUnitType::Auto, 1.0)?; // find box
    add_column(&root, GridUnitType::Pixel, 240.0)?;
    add_column(&root, GridUnitType::Star, 1.0)?;

    let tabs = TabView::new()?;
    for title in ["showcase.md", "linked.md"] {
        let tab = TabViewItem::new()?;
        tab.SetHeader(&boxed(title)?)?;
        tabs.TabItems()?.Append(&tab.cast::<IInspectable>()?)?;
    }
    place(&root, &tabs, 1, 0)?;
    Grid::SetColumnSpan(&tabs.cast::<FrameworkElement>()?, 2)?;

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
    hit_target.cast::<IPanel>()?.SetBackground(&brush.cast::<Brush>()?)?;
    panel
        .cast::<IPanel>()?
        .Children()?
        .Append(&hit_target.cast::<UIElement>()?)?;
    place(&root, &panel, 2, 1)?;

    let gpu = gpu
        .join()
        .map_err(|_| Error::new(HRESULT(0x8000FFFFu32 as i32), "device thread panicked"))??;
    let ui = Rc::new(Ui {
        window: window.clone(),
        root: root.clone(),
        renderer: RefCell::new(Renderer::new(gpu, panel.clone())),
        presented: Cell::new(false),
    });

    let framework: FrameworkElement = panel.cast()?;
    let redraw = {
        let ui = ui.clone();
        let fail_first_draw = std::env::args().any(|arg| arg == "--test-fail-draw");
        move || {
            let drawn = if fail_first_draw {
                Err(Error::new(HRESULT(0x80004005u32 as i32), "--test-fail-draw"))
            } else {
                ui.renderer.borrow_mut().draw()
            };
            match drawn {
                Ok(true) => {
                    if !ui.presented.replace(true) {
                        diag::trace("first Present returned");
                        watch_first_frame(ui.clone());
                    }
                }
                Ok(false) => {}
                // Nothing has been shown yet, so without this the window stays empty with no message.
                Err(e) if !ui.presented.get() => diag::fatal("first draw", &e),
                Err(e) => diag::fail("draw", &e),
            }
        }
    };
    {
        let redraw = redraw.clone();
        framework.SizeChanged(move |_, _| redraw())?.forget();
    }
    {
        let redraw = redraw.clone();
        panel
            .CompositionScaleChanged(move |_, _| redraw())?
            .forget();
    }
    {
        let ui = ui.clone();
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
                ui.renderer.borrow_mut().scroll -= delta / 3;
                if let Err(e) = ui.renderer.borrow_mut().draw() {
                    diag::fail("wheel draw", &e);
                }
            })?
            .forget();
    }

    window.SetContent(&root.cast::<UIElement>()?)?;
    window.Closed(|_, _| diag::trace("window closed"))?.forget();
    window.Activate()?;
    diag::trace("Window activated");
    Ok(())
}

/// Calls `then` on the second composition tick from now, then stops ticking. A live subscription
/// keeps XAML rendering every frame, so it must not outlive this.
fn after_two_ticks(then: impl FnOnce() + 'static) -> Result<()> {
    let ticks = Rc::new(Cell::new(0));
    let then = RefCell::new(Some(then));
    let revoker: Rc<RefCell<Option<EventRevoker>>> = Rc::default();
    let slot = revoker.clone();
    let registered = CompositionTarget::Rendering(move |_, _| {
        ticks.set(ticks.get() + 1);
        if ticks.get() == 2 {
            if let Some(then) = then.borrow_mut().take() {
                then();
            }
            // Revoke outside the event being raised.
            let slot = slot.clone();
            if let Ok(queue) = DispatcherQueue::GetForCurrentThread() {
                _ = queue.TryEnqueueWithPriority(
                    DispatcherQueuePriority::Low,
                    &DispatcherQueueHandler::new(move || drop(slot.borrow_mut().take())),
                );
            }
        }
    })?;
    *revoker.borrow_mut() = Some(registered);
    Ok(())
}

fn watch_first_frame(ui: Rc<Ui>) {
    let scheduled = after_two_ticks(move || {
        let first_frame_ms = diag::since_process_start_ms();
        let (private_first, _) = diag::memory();
        diag::trace(&format!("first frame at {first_frame_ms:.1} ms"));
        if let Err(e) = build_rest(&ui) {
            diag::fatal("build_rest", &e);
        }
        if !diag::measuring() {
            return;
        }
        let scheduled = after_two_ticks(move || {
            let (private_full, working_set) = diag::memory();
            diag::measure_line(&format!(
                "result=ok first_frame_ms={first_frame_ms:.1} full_ui_ms={:.1} \
                 private_bytes_first_frame={private_first} private_bytes_full_ui={private_full} \
                 working_set_full_ui={working_set}",
                diag::since_process_start_ms()
            ));
            std::process::exit(0);
        });
        if let Err(e) = scheduled {
            diag::fatal("full UI tick", &e);
        }
    });
    if let Err(e) = scheduled {
        diag::fatal("first frame tick", &e);
    }
}

/// Everything not needed for the first frame.
fn build_rest(ui: &Rc<Ui>) -> Result<()> {
    let menu = MenuBar::new()?;
    let file = MenuBarItem::new()?;
    file.SetTitle("File")?;
    let close = MenuFlyoutItem::new()?;
    close.SetText("Close")?;
    let window = ui.window.clone();
    close
        .Click(move |_, _| {
            _ = window.Close();
        })?
        .forget();
    file.Items()?.Append(&close.cast::<MenuFlyoutItemBase>()?)?;
    menu.Items()?.Append(&file)?;
    place(&ui.root, &menu, 0, 0)?;
    Grid::SetColumnSpan(&menu.cast::<FrameworkElement>()?, 2)?;

    let tree = TreeView::new()?;
    let folder = TreeViewNode::new()?;
    folder.SetContent(&boxed("fixtures (lazy)")?)?;
    folder.SetHasUnrealizedChildren(true)?;
    tree.RootNodes()?.Append(&folder)?;
    tree.Expanding(|_, args| {
        let Some(args) = args.as_ref() else { return };
        let result = (|| -> Result<()> {
            let node = args.Node()?;
            if node.HasUnrealizedChildren()? {
                for name in ["showcase.md", "linked.md", "assets"] {
                    let child = TreeViewNode::new()?;
                    child.SetContent(&boxed(name)?)?;
                    node.Children()?.Append(&child)?;
                }
                node.SetHasUnrealizedChildren(false)?;
                diag::trace("lazy tree children added");
            }
            Ok(())
        })();
        if let Err(e) = result {
            diag::fail("tree expanding", &e);
        }
    })?
    .forget();

    let flyout = MenuFlyout::new()?;
    let open = MenuFlyoutItem::new()?;
    open.SetText("Open in new tab")?;
    let window = ui.window.clone();
    open.Click(move |_, _| {
        diag::trace("context flyout item clicked");
        _ = window.SetTitle("mdview WinUI skeleton: context flyout clicked");
    })?
    .forget();
    flyout.Items()?.Append(&open.cast::<MenuFlyoutItemBase>()?)?;
    tree.cast::<IUIElement>()?
        .SetContextFlyout(&flyout.cast::<FlyoutBase>()?)?;
    place(&ui.root, &tree, 2, 0)?;

    let find = TextBox::new()?;
    find.SetPlaceholderText("Find")?;
    place(&ui.root, &find, 3, 0)?;
    Grid::SetColumnSpan(&find.cast::<FrameworkElement>()?, 2)?;
    diag::trace("deferred controls added");
    Ok(())
}
