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
    Recent,
    /// The main menu with its Open recent submenu beside it.
    Cascade,
    Tab,
    Page,
    Find,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShotDialog {
    About,
    Shortcuts,
    Recent,
    Edit,
    Settings,
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
    /// `--palette <query>`: the command palette open with this text (an empty one is `""`).
    pub palette: Option<String>,
    /// Logical pointer position.
    pub hover: Option<(f32, f32)>,
    pub focus_mode: bool,
    /// Load the first document's remote images without showing the bar (for checks).
    pub load_remote: bool,
    /// Show the update bar at this stage (`available`, `installing` or `installed`) with a
    /// made-up version, for a shot. Nothing is looked up or downloaded.
    pub update_bar: Option<String>,
    /// Look for a newer version, print the answer, and exit. No window.
    pub check_update: bool,
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
                "--load-remote" => out.load_remote = true,
                "--update-bar" => out.update_bar = value(&a),
                "--check-update" => out.check_update = true,
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
                "--hover" => out.hover = value(&a).and_then(|v| pair(&v, ',')),
                "--menu" => {
                    out.menu = match value(&a).as_deref() {
                        Some("main") => Some(ShotMenu::Main),
                        Some("main-end") => Some(ShotMenu::MainEnd),
                        Some("recent") => Some(ShotMenu::Recent),
                        Some("cascade") => Some(ShotMenu::Cascade),
                        Some("tab") => Some(ShotMenu::Tab),
                        Some("page") => Some(ShotMenu::Page),
                        Some("find") => Some(ShotMenu::Find),
                        _ => None,
                    }
                }
                "--palette" => out.palette = value(&a),
                "--dialog" => {
                    out.dialog = match value(&a).as_deref() {
                        Some("about") => Some(ShotDialog::About),
                        Some("shortcuts") => Some(ShotDialog::Shortcuts),
                        Some("recent") => Some(ShotDialog::Recent),
                        Some("edit") => Some(ShotDialog::Edit),
                        Some("settings") => Some(ShotDialog::Settings),
                        _ => None,
                    }
                }
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
    fn files_and_flags() {
        let a = parse("a.md --dark --zoom 125 b.md --folder docs --no-session");
        assert_eq!(a.files, [PathBuf::from("a.md"), PathBuf::from("b.md")]);
        assert_eq!(a.dark, Some(true));
        assert_eq!(a.zoom, Some(125));
        assert_eq!(a.folder, Some(PathBuf::from("docs")));
        assert!(a.no_session);
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
        assert_eq!(a.dialog, Some(ShotDialog::Settings));
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
