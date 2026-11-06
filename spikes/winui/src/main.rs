//! G8 skeleton: code-only WinUI 3 window from raw windows-bindgen output. No XAML, C# or C++.
//! Flags: --measure[=FILE] logs first-frame time and private bytes, then exits. --trace logs
//! startup checkpoints to mdview-winui.trace.log beside the exe.
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
        // Lets a script prove the failure path below without breaking the build.
        if std::env::args().any(|arg| arg == "--test-fail-launch") {
            diag::fatal("OnLaunched", &Error::new(HRESULT(0x80004005u32 as i32), "--test-fail-launch"));
        }
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

    let gpu = Rc::new(RefCell::new(Some(gpu)));
    // Application and UI state must outlive Start, which returns at process end.
    let keep_alive: Rc<RefCell<Vec<IInspectable>>> = Rc::default();
    Application::Start(&ApplicationInitializationCallback::new(move |_| {
        diag::trace("Start callback");
        let gpu = gpu.clone();
        let app_slot: Rc<RefCell<Option<Application>>> = Rc::default();
        let launch_slot = app_slot.clone();
        let on_launched = Box::new(move || -> Result<()> {
            let app = launch_slot.borrow().clone().expect("application composed");
            let controls: ResourceDictionary = XamlControlsResources::new()?.cast()?;
            app.Resources()?.MergedDictionaries()?.Append(&controls)?;
            diag::trace("XamlControlsResources merged");
            let gpu = gpu.borrow_mut().take().expect("launched once");
            ui::build(gpu)
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
