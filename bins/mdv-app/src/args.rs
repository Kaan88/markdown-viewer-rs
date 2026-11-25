//! Command-line arguments: files to open, start-up overrides, and the flags that render a UI state
//! to a PNG without a window (`--shot`), so the look can be checked without a human.

use mdshared_app::sister::{Rect, Request};
use mdshared_ui::corners::Corners;
use mdview_core::session::PageWidth;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShotMenu {
    Main,
    /// The main menu scrolled to its last row (visible only when the window is too short for it).
    MainEnd,
    /// The main menu opened by keyboard: the access keys are underlined.
    MainKeys,
    Recent,
    /// The main menu with its Open recent submenu beside it.
    Cascade,
    Tab,
    Page,
    Find,
    /// The groups of the notes library, to choose where the open file is added.
    Library,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShotDialog {
    About,
    Shortcuts,
    Recent,
    Edit,
    Settings,
    /// The question before the first file is added to the notes library.
    AddLibrary,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Args {
    pub files: Vec<PathBuf>,
    /// `--light` (false) or `--dark` (true).
    pub dark: Option<bool>,
    pub zoom: Option<u16>,
    /// `--page-font FAMILY` and `--code-font FAMILY`: the same as the Settings rows.
    pub page_font: Option<String>,
    pub code_font: Option<String>,
    /// `--page-width narrow|wide|full`.
    pub page_width: Option<PageWidth>,
    pub folder: Option<PathBuf>,
    /// Do not read or write the user's session (and skip single-instance hand-over).
    pub no_session: bool,
    pub shot: Option<PathBuf>,
    /// Logical window size for `--shot`.
    pub size: Option<(u32, u32)>,
    pub scale: Option<f32>,
    /// `--frame square|small|round`: the shot shows the window as the client-drawn shadow margin
    /// puts it on screen (its shadow and corners, over a flat backdrop) instead of the content
    /// alone.
    pub frame: Option<Corners>,
    pub files_pane: Option<bool>,
    pub outline_pane: Option<bool>,
    pub find: Option<String>,
    /// Logical px.
    pub scroll: Option<f32>,
    pub menu: Option<ShotMenu>,
    pub dialog: Option<ShotDialog>,
    /// `--settings-section <page>`: with `--dialog settings`, the page shown (a page's name, as
    /// it is or with hyphens for spaces, or an old group's name), for a shot.
    pub settings_section: Option<String>,
    /// `--outline-focus <n>`: the keyboard on row n (from 0) of the Outline pane, for a shot.
    pub outline_focus: Option<usize>,
    /// `--palette <query>`: the command palette open with this text (an empty one is `""`).
    pub palette: Option<String>,
    /// `--folder-search <query>`: Find in folder open with this text, searched to the end.
    pub folder_search: Option<String>,
    /// Logical pointer position.
    pub hover: Option<(f32, f32)>,
    /// Show the copy button of the code block under `--hover` with its check mark (for shots).
    pub copied: bool,
    pub focus_mode: bool,
    /// `--notes`: the Files pane shows the notes library (when there is one), for a shot.
    pub notes_face: bool,
    /// `--present N`: start presenting at slide N (1 is the first), for shots and checks.
    pub present: Option<usize>,
    /// `--present-overview`: with `--present`, the list of slides open (for shots).
    pub present_overview: bool,
    /// `--present-select`: with `--present`, the text of the slide selected (for shots).
    pub present_select: bool,
    /// Load the first document's remote images without showing the bar (for checks).
    pub load_remote: bool,
    /// Show the update bar at this stage (`available`, `installing` or `installed`) with a
    /// made-up version, for a shot. Nothing is looked up or downloaded.
    pub update_bar: Option<String>,
    /// `--zoom-diagram N`: open the N-th (from 1) Mermaid diagram of the first document at a
    /// reading size, for a shot.
    pub zoom_diagram: Option<usize>,
    /// `--menu-at <ms>`: with `--menu`, the menu as it is this long into its fade-in, for a shot.
    pub menu_at: Option<u64>,
    /// `--turn <name>`: with `--files`, opens (or closes) the folder of that name at the top of
    /// the Files tree, for a shot; with `--motion-at`, as its chevron turns.
    pub turn: Option<String>,
    /// `--toast <text>`, said once or more: the status messages a shot shows as toasts, the
    /// last one coming in with `--motion-at`, the others already at rest above it.
    pub toasts: Vec<String>,
    /// `--motion-at <ms>`: what a shot's `--turn` or last `--toast` sets moving, that long into
    /// its motion.
    pub motion_at: Option<u64>,
    /// `--zoom-level F`: with `--zoom-diagram`, its drawing size in device px per raster px (1.0,
    /// the default, is twice the page's size at any scale).
    pub zoom_level: Option<f32>,
    /// `--zoom-pan X,Y`: with `--zoom-diagram`, its top left inside the window, logical px.
    pub zoom_pan: Option<(f32, f32)>,
    /// `--demo-untitled <file>`: show the text of `file` as a document mdedit previews
    /// before it has a file (a tab named "Untitled 1", live), for a shot. No editor is
    /// asked or needed.
    pub demo_untitled: Option<PathBuf>,
    /// `--backdrop <off|mica|acrylic>`: the "Window backdrop" setting for this run, for a
    /// shot or a capture. With `--no-session` nothing of it is saved.
    pub backdrop: Option<mdshared_app::backdrop::Backdrop>,
    /// Look for a newer version, print the answer, and exit. No window.
    pub check_update: bool,
    /// Install a newer version as the update bar would, start it, and exit. No window.
    pub install_update: bool,
    /// Scroll this many wheel steps after the first frame, log timings, and exit.
    pub scroll_steps: Option<u32>,
    /// Start maximized (for checking the frameless maximized geometry).
    pub maximize: bool,
    /// `--line N`: scroll to the block holding source line N (counting from 1) of the file.
    pub line: Option<u32>,
    /// `--follow`: a live update from the editor, not an opening. It only ever reaches a window
    /// that is already running and shows the file; it starts nothing.
    pub follow: bool,
    /// `--place x,y,w,h`: move and resize the window, in physical screen px.
    pub place: Option<Rect>,
    /// `--from <name>`: which application sent the launch.
    pub from: Option<String>,
}

fn pair<T: std::str::FromStr>(s: &str, sep: char) -> Option<(T, T)> {
    let (a, b) = s.split_once(sep)?;
    Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
}

impl Args {
    /// What each file asks of a window that is already running, when the launch asked for more than
    /// opening it: see [`Request`]. Empty for an ordinary launch.
    pub fn requests(&self) -> Vec<Request> {
        if self.line.is_none() && !self.follow && self.place.is_none() && self.from.is_none() {
            return Vec::new();
        }
        self.files
            .iter()
            .map(|path| Request {
                path: path.clone(),
                line: self.line,
                column: None,
                follow: self.follow,
                place: self.place,
                from: self.from.clone(),
                ..Request::default()
            })
            .collect()
    }

    /// Parses the arguments after the program name. Unknown flags and bad values are reported on
    /// stderr and ignored, so a stray flag never stops the viewer from opening.
    pub fn parse(args: impl IntoIterator<Item = String>) -> Args {
        let mut out = Args::default();
        let mut it = args.into_iter();
        while let Some(a) = it.next() {
            let mut value = |name: &str| {
                let v = it.next();
                if v.is_none() {
                    eprintln!("mdview: {name} needs a value");
                }
                v
            };
            match a.as_str() {
                "--light" => out.dark = Some(false),
                "--dark" => out.dark = Some(true),
                "--no-session" => out.no_session = true,
                "--files" => out.files_pane = Some(true),
                "--no-files" => out.files_pane = Some(false),
                "--outline" => out.outline_pane = Some(true),
                "--no-outline" => out.outline_pane = Some(false),
                "--focus-mode" => out.focus_mode = true,
                "--notes" => out.notes_face = true,
                "--present" => {
                    out.present = value(&a).and_then(|v| v.parse().ok()).filter(|&n| n >= 1);
                }
                "--present-overview" => out.present_overview = true,
                "--present-select" => out.present_select = true,
                "--load-remote" => out.load_remote = true,
                "--update-bar" => out.update_bar = value(&a),
                "--zoom-diagram" => {
                    out.zoom_diagram = value(&a).and_then(|v| v.parse().ok()).filter(|&n| n >= 1);
                }
                "--zoom-level" => {
                    out.zoom_level = value(&a)
                        .and_then(|v| v.parse().ok())
                        .filter(|f: &f32| *f > 0.0);
                }
                "--zoom-pan" => out.zoom_pan = value(&a).and_then(|v| pair(&v, ',')),
                "--demo-untitled" => out.demo_untitled = value(&a).map(PathBuf::from),
                "--backdrop" => {
                    out.backdrop = value(&a)
                        .as_deref()
                        .and_then(mdshared_app::backdrop::Backdrop::parse)
                }
                "--check-update" => out.check_update = true,
                "--install-update" => out.install_update = true,
                "--maximize" => out.maximize = true,
                "--follow" => out.follow = true,
                "--line" => out.line = value(&a).and_then(|v| v.parse().ok()).filter(|&n| n >= 1),
                "--place" => out.place = value(&a).and_then(|v| Rect::parse(&v)),
                "--from" => out.from = value(&a).filter(|v| !v.is_empty()),
                "--zoom" => out.zoom = value(&a).and_then(|v| v.trim_end_matches('%').parse().ok()),
                "--page-font" => out.page_font = value(&a),
                "--code-font" => out.code_font = value(&a),
                "--page-width" => {
                    out.page_width = value(&a).and_then(|v| PageWidth::from_name(&v));
                }
                "--folder" => out.folder = value(&a).map(PathBuf::from),
                "--shot" => out.shot = value(&a).map(PathBuf::from),
                "--size" => out.size = value(&a).and_then(|v| pair(&v.to_ascii_lowercase(), 'x')),
                "--scale" => {
                    out.scale = value(&a)
                        .and_then(|v| v.parse().ok())
                        .filter(|s: &f32| *s >= 0.5 && *s <= 4.0)
                }
                "--frame" => out.frame = value(&a).and_then(|v| Corners::from_name(&v)),
                "--find" => out.find = value(&a),
                "--scroll" => out.scroll = value(&a).and_then(|v| v.parse().ok()),
                "--scroll-steps" => out.scroll_steps = value(&a).and_then(|v| v.parse().ok()),
                "--copied" => out.copied = true,
                "--menu-at" => out.menu_at = value(&a).and_then(|v| v.parse().ok()),
                "--turn" => out.turn = value(&a),
                "--toast" => out.toasts.extend(value(&a)),
                "--motion-at" => out.motion_at = value(&a).and_then(|v| v.parse().ok()),
                "--hover" => out.hover = value(&a).and_then(|v| pair(&v, ',')),
                "--menu" => {
                    out.menu = match value(&a).as_deref() {
                        Some("main") => Some(ShotMenu::Main),
                        Some("main-end") => Some(ShotMenu::MainEnd),
                        Some("main-keys") => Some(ShotMenu::MainKeys),
                        Some("recent") => Some(ShotMenu::Recent),
                        Some("cascade") => Some(ShotMenu::Cascade),
                        Some("tab") => Some(ShotMenu::Tab),
                        Some("page") => Some(ShotMenu::Page),
                        Some("find") => Some(ShotMenu::Find),
                        Some("library") => Some(ShotMenu::Library),
                        _ => None,
                    }
                }
                "--outline-focus" => out.outline_focus = value(&a).and_then(|v| v.parse().ok()),
                "--palette" => out.palette = value(&a),
                "--folder-search" => out.folder_search = value(&a),
                "--dialog" => {
                    out.dialog = match value(&a).as_deref() {
                        Some("about") => Some(ShotDialog::About),
                        Some("shortcuts") => Some(ShotDialog::Shortcuts),
                        Some("recent") => Some(ShotDialog::Recent),
                        Some("edit") => Some(ShotDialog::Edit),
                        Some("settings") => Some(ShotDialog::Settings),
                        Some("library-add") => Some(ShotDialog::AddLibrary),
                        _ => None,
                    }
                }
                "--settings-section" => out.settings_section = value(&a),
                // The mark of a restart after an update, read before this (`relaunch::FLAG`).
                mdshared_app::relaunch::FLAG => {}
                f if f.starts_with("--") => eprintln!("mdview: ignoring {f}"),
                _ => out.files.push(PathBuf::from(a)),
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Args {
        Args::parse(s.split_whitespace().map(String::from))
    }

    #[test]
    fn the_restart_mark_is_not_a_file() {
        let a = parse("--restarted");
        assert!(a.files.is_empty());
        assert_eq!(a, parse(""));
    }

    #[test]
    fn files_and_flags() {
        let a = parse("a.md --dark --zoom 125 b.md --folder docs --no-session");
        assert_eq!(a.files, [PathBuf::from("a.md"), PathBuf::from("b.md")]);
        assert_eq!(a.dark, Some(true));
        assert_eq!(a.zoom, Some(125));
        assert_eq!(a.folder, Some(PathBuf::from("docs")));
        assert!(a.no_session);
    }

    #[test]
    fn motion_shot_flags() {
        let a = parse("--turn wiki --toast Copied --toast Linked --motion-at 67 --menu-at 21");
        assert_eq!(a.turn.as_deref(), Some("wiki"));
        assert_eq!(a.toasts, ["Copied", "Linked"]);
        assert_eq!(a.motion_at, Some(67));
        assert_eq!(a.menu_at, Some(21));
        let b = parse("--motion-at soon");
        assert_eq!(b.motion_at, None);
        assert!(b.toasts.is_empty() && b.turn.is_none());
    }

    #[test]
    fn shot_flags() {
        let a = parse(
            "--shot x.png --size 700x500 --scale 1.5 --hover 10,20 --menu recent --dialog about --scroll 300 --no-files",
        );
        assert_eq!(a.shot, Some(PathBuf::from("x.png")));
        assert_eq!(a.size, Some((700, 500)));
        assert_eq!(a.scale, Some(1.5));
        assert_eq!(a.hover, Some((10.0, 20.0)));
        assert_eq!(a.menu, Some(ShotMenu::Recent));
        assert_eq!(a.dialog, Some(ShotDialog::About));
        assert_eq!(a.scroll, Some(300.0));
        assert_eq!(a.files_pane, Some(false));
        let a = parse("--shot x.png --palette >zoom --dialog settings");
        assert_eq!(a.palette.as_deref(), Some(">zoom"));
        assert_eq!(parse("--outline-focus 3").outline_focus, Some(3));
        let p = parse("--present 2 --present-overview --present-select");
        assert!(p.present_overview && p.present_select && p.present == Some(2));
        assert_eq!(a.dialog, Some(ShotDialog::Settings));
        let a = parse("--dialog settings --settings-section reading");
        assert_eq!(a.settings_section.as_deref(), Some("reading"));
    }

    #[test]
    fn the_diagram_zoom_shot_flags() {
        let a = parse("--zoom-diagram 2 --zoom-level 1.5 --zoom-pan -120,40");
        assert_eq!(a.zoom_diagram, Some(2));
        assert_eq!(a.zoom_level, Some(1.5));
        assert_eq!(a.zoom_pan, Some((-120.0, 40.0)));
        assert_eq!(parse("--zoom-diagram 0 --zoom-level 0").zoom_diagram, None);
        assert_eq!(parse("--zoom-level 0").zoom_level, None);
    }

    #[test]
    fn frame_names_the_corners_of_a_window_shot() {
        assert_eq!(parse("--frame round").frame, Some(Corners::Round));
        assert_eq!(parse("--frame Square").frame, Some(Corners::Square));
        assert_eq!(parse("--frame 4").frame, Some(Corners::Small));
        assert_eq!(parse("--frame pointy").frame, None);
        assert_eq!(parse("a.md").frame, None);
    }

    #[test]
    fn the_hand_off_flags() {
        let a = parse("--line 200 --follow --place -10,20,960,1000 --from mdedit notes.md");
        assert_eq!(a.line, Some(200));
        assert!(a.follow);
        assert_eq!(
            a.place,
            Some(Rect {
                x: -10,
                y: 20,
                w: 960,
                h: 1000
            })
        );
        assert_eq!(a.from.as_deref(), Some("mdedit"));
        assert_eq!(
            a.requests(),
            [Request {
                line: Some(200),
                follow: true,
                place: a.place,
                from: Some("mdedit".into()),
                ..Request::new("notes.md")
            }]
        );
        // An ordinary launch has no request; bad values are dropped, not guessed at.
        assert!(parse("a.md --dark").requests().is_empty());
        let bad = parse("--line 0 --place 1,2,3 a.md");
        assert_eq!((bad.line, bad.place), (None, None));
        assert!(bad.requests().is_empty());
    }

    #[test]
    fn bad_values_are_ignored() {
        let a = parse("--zoom lots --scale 9 --size big --bogus --menu nope");
        assert_eq!(a, Args { ..Args::default() });
        assert_eq!(parse("--zoom").zoom, None);
    }
}
