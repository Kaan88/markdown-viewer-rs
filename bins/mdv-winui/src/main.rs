//! mdview's Windows shell: a WinUI 3 window drawn from raw windows-bindgen output, showing the
//! page `mdview-render` lays out. No XAML, C# or C++.
//!
//! Usage: `mdview-winui [FILE] [--trace] [--measure[=FILE]]`. `--measure` logs first-frame time and
//! memory, then exits; `--trace` writes startup checkpoints to `mdview.trace.log` beside the exe.
#![windows_subsystem = "windows"]

#[allow(
    clippy::all,
    dead_code,
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    unused_imports
)]
mod bindings;
mod bootstrap;
mod diag;
#[allow(non_snake_case)]
mod render;
mod ui;

use bindings::*;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use windows_core::*;

implement_decl! {
    impl App as pub App_Impl: [IApplicationOverrides, IXamlMetadataProvider]
}

/// Application override object, composed over Microsoft.UI.Xaml.Application.
pub struct App {
    provider: RefCell<Option<XamlControlsXamlMetaDataProvider>>,
    on_launched: RefCell<Option<Box<dyn FnOnce() -> Result<()>>>>,
}

impl App {
    fn provider(&self) -> Result<XamlControlsXamlMetaDataProvider> {
        if let Some(provider) = self.provider.borrow().as_ref() {
            return Ok(provider.clone());
        }
        let provider = XamlControlsXamlMetaDataProvider::new()?;
        *self.provider.borrow_mut() = Some(provider.clone());
        Ok(provider)
    }
}

impl IApplicationOverrides_Impl for App_Impl {
    fn OnLaunched(&self, _args: Ref<LaunchActivatedEventArgs>) -> Result<()> {
        diag::trace("OnLaunched");
        if let Some(launch) = self.on_launched.borrow_mut().take() {
            // An error returned from here ends in a fail-fast crash with no message, or, if XAML
            // swallowed it, a process with no window. Report it and exit instead.
            if let Err(e) = launch() {
                diag::fatal("OnLaunched", &e);
            }
        }
        Ok(())
    }
}

impl IXamlMetadataProvider_Impl for App_Impl {
    fn GetXamlType(&self, r#type: &TypeName) -> Result<IXamlType> {
        self.provider()?.GetXamlType(r#type)
    }
    fn GetXamlTypeByFullName(&self, full_name: &HSTRING) -> Result<IXamlType> {
        self.provider()?
            .GetXamlTypeByFullName(&full_name.to_string_lossy())
    }
    fn GetXmlnsDefinitions(&self) -> Result<Array<XmlnsDefinition>> {
        self.provider()?.GetXmlnsDefinitions()
    }
}

/// The first positional argument is the document to open; flags are handled by `diag`.
fn document_argument() -> Option<PathBuf> {
    std::env::args()
        .skip(1)
        .find(|arg| !arg.starts_with("--"))
        .map(PathBuf::from)
}

/// `--light` and `--dark` override the Windows app theme, for screenshots and for testing both.
fn theme_argument() -> Option<mdview_render::Theme> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|arg| arg == "--dark") {
        Some(mdview_render::DARK)
    } else if args.iter().any(|arg| arg == "--light") {
        Some(mdview_render::LIGHT)
    } else {
        None
    }
}

fn run() -> Result<()> {
    // DPI awareness comes from the embedded manifest, so it is set before any code runs.
    diag::trace_dpi_awareness();
    // The device is ready sooner when created while the main thread loads XAML.
    let gpu = std::thread::spawn(render::create_gpu);
    bootstrap::bootstrap()?;
    diag::trace("bootstrap done");
    unsafe {
        CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32).ok()?;
    }
    diag::trace("STA ready, calling Application::Start");

    let document = document_argument();
    let theme = theme_argument();
    let gpu = Rc::new(RefCell::new(Some(gpu)));
    // Application and UI state must outlive Start, which returns at process end.
    let keep_alive: Rc<RefCell<Vec<IInspectable>>> = Rc::default();
    Application::Start(&ApplicationInitializationCallback::new(move |_| {
        diag::trace("Start callback");
        let gpu = gpu.clone();
        let document = document.clone();
        let theme = theme;
        let app_slot: Rc<RefCell<Option<Application>>> = Rc::default();
        let launch_slot = app_slot.clone();
        let on_launched = Box::new(move || -> Result<()> {
            let app = launch_slot.borrow().clone().expect("application composed");
            let controls: ResourceDictionary = XamlControlsResources::new()?.cast()?;
            app.Resources()?.MergedDictionaries()?.Append(&controls)?;
            diag::trace("XamlControlsResources merged");
            let gpu = gpu.borrow_mut().take().expect("launched once");
            ui::build(gpu, document.clone(), theme)
        });
        let composed = Application::compose(App {
            provider: RefCell::new(None),
            on_launched: RefCell::new(Some(on_launched)),
        });
        match composed {
            Ok(app) => {
                diag::trace("Application composed");
                keep_alive.borrow_mut().push(app.clone().into());
                let handler = app.UnhandledException(|_, args| {
                    let message = args
                        .as_ref()
                        .and_then(|a| a.Message().ok())
                        .unwrap_or_default();
                    diag::fatal_text(&format!("XAML unhandled exception: {message}"));
                });
                match handler {
                    Ok(handler) => handler.forget(),
                    Err(e) => diag::fatal("UnhandledException handler", &e),
                }
                *app_slot.borrow_mut() = Some(app);
            }
            Err(e) => diag::fatal("Application::compose", &e),
        }
    }))
}

fn main() {
    diag::init();
    diag::trace("main");
    if let Err(e) = run() {
        diag::fatal("run", &e);
    }
    diag::trace("Application::Start returned");
}
