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
mod input;
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

/// The command line, read once. Flags that take a value consume the next argument, so
/// `--zoom 2 file.md` opens `file.md`, not a file named `2`. `--trace` and `--measure[=FILE]` are
/// read by `diag`, and only skipped here.
struct Args {
    document: Option<PathBuf>,
    theme: Option<mdview_render::Theme>,
    zoom: f32,
    scroll: f32,
    steps: u32,
    size: (i32, i32),
    flags: Vec<String>,
}

const VALUE_FLAGS: [&str; 4] = ["--zoom", "--scroll", "--scroll-steps", "--size"];

impl Args {
    fn parse(args: impl IntoIterator<Item = String>) -> Self {
        let mut out = Args {
            document: None,
            theme: None,
            zoom: 1.0,
            scroll: 0.0,
            steps: 0,
            size: (1100, 900),
            flags: Vec::new(),
        };
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            if VALUE_FLAGS.contains(&arg.as_str()) {
                let value = args.next().unwrap_or_default();
                match arg.as_str() {
                    "--zoom" => out.zoom = value.parse().unwrap_or(out.zoom),
                    "--scroll" => out.scroll = value.parse().unwrap_or(out.scroll),
                    "--scroll-steps" => out.steps = value.parse().unwrap_or(out.steps),
                    _ => {
                        if let Some((w, h)) = value.split_once('x')
                            && let (Ok(w), Ok(h)) = (w.parse(), h.parse())
                        {
                            out.size = (w, h);
                        }
                    }
                }
            } else if arg == "--dark" {
                out.theme = Some(mdview_render::DARK);
            } else if arg == "--light" {
                out.theme.get_or_insert(mdview_render::LIGHT);
            } else if arg.starts_with("--") {
                out.flags.push(arg);
            } else if out.document.is_none() {
                out.document = Some(PathBuf::from(arg));
            }
        }
        out
    }

    fn has(&self, flag: &str) -> bool {
        self.flags.iter().any(|f| f == flag)
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

    let args = Args::parse(std::env::args().skip(1));
    let mut document = args.document.clone();
    let mut theme = args.theme;
    let mut zoom = args.zoom;
    // The saved session (last document, Recent, theme, zoom) is off for `--no-session` and for the
    // scripted checks, so they start from a known state and never touch the user's file.
    let scripted = args.flags.iter().any(|f| f.ends_with("-test") || f.starts_with("--measure"));
    if !args.has("--no-session") && !scripted {
        let path = std::env::var_os("MDVIEW_SETTINGS")
            .map(PathBuf::from)
            .or_else(mdview_core::session::default_settings_path);
        if let Some(path) = path {
            let saved = mdview_core::session::Session::load(&path);
            if theme.is_none() {
                theme = match saved.theme {
                    mdview_core::session::Theme::Light => Some(mdview_render::LIGHT),
                    mdview_core::session::Theme::Dark => Some(mdview_render::DARK),
                    mdview_core::session::Theme::System => None,
                };
            }
            if (zoom - 1.0).abs() < 1e-6 {
                zoom = f32::from(saved.zoom) / 100.0;
            }
            if document.is_none() {
                document = saved.tabs.get(saved.active).filter(|p| p.is_file()).cloned();
            }
            ui::set_settings(path);
        }
    }
    // A shifted frame has to look exactly like a whole one; `--no-shift` sends every frame whole,
    // which is the picture to compare against.
    let startup = ui::Startup {
        shift: !args.has("--no-shift"),
        scroll: args.scroll,
        steps: args.steps,
        size: args.size,
        select: args.has("--select-test"),
        details: args.has("--details-test"),
        find: args.has("--find-test"),
        outline: args.has("--outline-test"),
        copy: args.has("--copy-test"),
        zoom: args.has("--zoom-test"),
    };
    let gpu = Rc::new(RefCell::new(Some(gpu)));
    // Application and UI state must outlive Start, which returns at process end.
    let keep_alive: Rc<RefCell<Vec<IInspectable>>> = Rc::default();
    Application::Start(&ApplicationInitializationCallback::new(move |_| {
        diag::trace("Start callback");
        let gpu = gpu.clone();
        let document = document.clone();
        let theme = theme;
        let zoom = zoom;
        let app_slot: Rc<RefCell<Option<Application>>> = Rc::default();
        let launch_slot = app_slot.clone();
        let on_launched = Box::new(move || -> Result<()> {
            let app = launch_slot.borrow().clone().expect("application composed");
            let controls: ResourceDictionary = XamlControlsResources::new()?.cast()?;
            app.Resources()?.MergedDictionaries()?.Append(&controls)?;
            diag::trace("XamlControlsResources merged");
            let gpu = gpu.borrow_mut().take().expect("launched once");
            ui::build(gpu, document.clone(), theme, zoom, startup)
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

#[cfg(test)]
mod tests {
    use super::Args;

    fn parse(line: &str) -> Args {
        Args::parse(line.split_whitespace().map(str::to_string))
    }

    #[test]
    fn value_flags_do_not_become_the_document() {
        let args = parse("--zoom 2 file.md --scroll 40 --size 800x600 --trace");
        assert_eq!(
            args.document.as_deref(),
            Some(std::path::Path::new("file.md"))
        );
        assert_eq!(args.zoom, 2.0);
        assert_eq!(args.scroll, 40.0);
        assert_eq!(args.size, (800, 600));
        assert!(args.has("--trace"));
    }

    #[test]
    fn dark_wins_over_light_and_defaults_hold() {
        let args = parse("--light --dark");
        assert!(args.document.is_none());
        assert_eq!(args.zoom, 1.0);
        assert_eq!(args.size, (1100, 900));
        assert!(args.theme.is_some());
    }
}
