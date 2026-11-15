//! Hover cards: what the pointer resting on a footnote mark or an internal link shows. A footnote
//! or a heading of the open page is read from the parsed document at once; another local Markdown
//! file is read on a helper thread started only then (never at launch, never for a web address),
//! and a few answers are kept so a second rest on the same link costs nothing.

use crate::gfx::{Face, Painter, Rect};
use crate::theme::Palette;
use mdview_core::preview::Preview;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime};

/// What a card could show for the thing under the pointer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// Footnote `n` of the open page (an index into its footnotes).
    Footnote(u32),
    /// The heading at this block of the open page.
    Heading(u32),
    /// The `<abbr title>` span at this index of the open page's spans.
    Abbr(u32),
    /// A local Markdown file, at a heading when the link names one.
    File {
        path: PathBuf,
        fragment: Option<String>,
    },
}

/// A card waiting to show, or showing. Only one exists, and only while the pointer rests on a
/// footnote mark or a link.
#[derive(Debug)]
pub struct Card {
    pub target: Target,
    /// Where the pointer was when it came to rest, in logical px.
    pub at: (f32, f32),
    pub due: Instant,
    /// The text is being read on a helper thread.
    pub loading: bool,
    /// The text, once there is some. The card shows when it is there and `due` has passed.
    pub content: Option<Preview>,
    pub shown: bool,
}

/// How much of a file is read for its card: a title and a few lines never lie further in.
pub const READ_BYTES: u64 = 128 * 1024;
/// Answers kept.
const KEPT: usize = 12;

type Key = (PathBuf, Option<String>);

struct Kept {
    key: Key,
    modified: Option<SystemTime>,
    preview: Option<Preview>,
}

type Done = Arc<Mutex<Vec<Kept>>>;

/// The cache of what files showed, and the helper threads reading new ones.
#[derive(Default)]
pub struct Cache {
    kept: Vec<Kept>,
    pending: Vec<Key>,
    done: Done,
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
}

/// The answer for a file.
pub enum Lookup {
    Ready(Option<Preview>),
    /// A thread is reading it; the wake says when it is done.
    Loading,
}

impl Cache {
    /// What to call when a thread has an answer.
    pub fn set_wake(&mut self, wake: impl Fn() + Send + Sync + 'static) {
        self.wake = Some(Arc::new(wake));
    }

    /// The card for a file: the kept answer when the file has not changed since, else a thread is
    /// started (once per file) to read it.
    pub fn get(&mut self, path: &Path, fragment: Option<&str>) -> Lookup {
        let key: Key = (path.to_path_buf(), fragment.map(str::to_string));
        let modified = modified(path);
        if let Some(i) = self.kept.iter().position(|k| k.key == key) {
            if self.kept[i].modified == modified {
                // The newest stays last; the oldest goes first.
                let k = self.kept.remove(i);
                let answer = k.preview.clone();
                self.kept.push(k);
                return Lookup::Ready(answer);
            }
            self.kept.remove(i);
        }
        if !self.pending.contains(&key) {
            self.pending.push(key.clone());
            let done = self.done.clone();
            let wake = self.wake.clone();
            let spawned = std::thread::Builder::new()
                .name("preview".into())
                .spawn(move || {
                    let preview = load(&key.0, key.1.as_deref());
                    if let Ok(mut d) = done.lock() {
                        d.push(Kept {
                            key,
                            modified,
                            preview,
                        });
                    }
                    if let Some(w) = wake {
                        w();
                    }
                });
            if spawned.is_err() {
                self.pending.clear();
                return Lookup::Ready(None);
            }
        }
        Lookup::Loading
    }

    /// Takes in what the threads finished. True if there was anything.
    pub fn collect(&mut self) -> bool {
        let finished = match self.done.lock() {
            Ok(mut d) => std::mem::take(&mut *d),
            Err(_) => return false,
        };
        let any = !finished.is_empty();
        for k in finished {
            self.pending.retain(|p| *p != k.key);
            self.kept.retain(|o| o.key != k.key);
            self.kept.push(k);
        }
        if self.kept.len() > KEPT {
            let extra = self.kept.len() - KEPT;
            self.kept.drain(..extra);
        }
        any
    }
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// The card of a Markdown file: its heading `fragment` with what follows when the link names one
/// that exists, else the start of the file. `None` when it cannot be read. Reads at most
/// [`READ_BYTES`].
pub fn load(path: &Path, fragment: Option<&str>) -> Option<Preview> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(READ_BYTES)
        .read_to_end(&mut bytes)
        .ok()?;
    let doc = mdview_core::parse_bytes(&bytes);
    fragment
        .and_then(|f| doc.resolve_fragment(f))
        .and_then(|b| mdview_core::preview::heading(&doc, b))
        .or_else(|| mdview_core::preview::start(&doc))
}

// ---- layout and drawing ----------------------------------------------------------------------

const TEXT: f32 = 13.0;
const PAD_X: f32 = 12.0;
const PAD_Y: f32 = 9.0;
const LINE: f32 = 18.0;
const TITLE_GAP: f32 = 3.0;
/// The card's widest and its most body lines.
const MAX_W: f32 = 380.0;
const MAX_LINES: usize = 6;
/// Below and right of the pointer, so the card does not sit on what is read.
const OFFSET: (f32, f32) = (12.0, 20.0);

/// A card laid out: where it is and the lines in it.
pub struct Layout {
    pub rect: Rect,
    title: Option<String>,
    lines: Vec<String>,
}

impl Layout {
    /// What drawing it touches, shadow included.
    pub fn bounds(&self) -> Rect {
        let r = self.rect;
        Rect::new(r.x - 18.0, r.y - 16.0, r.w + 36.0, r.h + 38.0)
    }

    /// A text that identifies the layout for the overlay signature.
    pub fn key(&self) -> (u32, u32, usize) {
        (
            self.rect.x.to_bits(),
            self.rect.y.to_bits(),
            self.lines.len() + self.title.is_some() as usize,
        )
    }
}

/// Splits `text` into lines no wider than `max`, breaking at spaces (a longer word is cut).
fn wrap(p: &mut Painter, face: Face, text: &str, max: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in text.split(' ') {
        let joined = if cur.is_empty() {
            word.to_string()
        } else {
            format!("{cur} {word}")
        };
        if cur.is_empty() || p.measure(face, TEXT, &joined) <= max {
            cur = joined;
        } else {
            lines.push(std::mem::take(&mut cur));
            cur = word.to_string();
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

/// Lays `preview` out as a card of at most [`MAX_LINES`] body lines, the last ending in an
/// ellipsis when there was more, placed near the pointer at `at` inside a `win_w` x `win_h` window.
pub fn layout(
    p: &mut Painter,
    preview: &Preview,
    at: (f32, f32),
    win_w: f32,
    win_h: f32,
) -> Layout {
    let max_w = MAX_W.min(win_w - 16.0);
    let inner = max_w - 2.0 * PAD_X;
    let mut lines = Vec::new();
    let mut more = false;
    for para in preview.body.lines() {
        if lines.len() >= MAX_LINES {
            more = true;
            break;
        }
        let wrapped = wrap(p, Face::Ui, para, inner);
        let room = MAX_LINES - lines.len();
        if wrapped.len() > room {
            more = true;
        }
        lines.extend(wrapped.into_iter().take(room));
    }
    if let Some(last) = lines.last_mut() {
        if more && !last.ends_with('…') {
            // The ellipsis follows a whole word where one fits.
            while p.measure(Face::Ui, TEXT, &format!("{last}…")) > inner {
                let Some(space) = last.rfind(' ') else { break };
                last.truncate(space);
            }
            let kept = last.trim_end_matches([',', ';', ':', ' ']).len();
            last.truncate(kept);
            last.push('…');
        }
        *last = p.elide(Face::Ui, TEXT, last, inner);
    }
    let title = Some(preview.title.clone())
        .filter(|t| !t.is_empty())
        .map(|t| p.elide(Face::Semibold, TEXT, &t, inner));
    let mut widest = title
        .as_deref()
        .map_or(0.0, |t| p.measure(Face::Semibold, TEXT, t));
    for l in &lines {
        widest = widest.max(p.measure(Face::Ui, TEXT, l));
    }
    let w = (widest + 2.0 * PAD_X).ceil().min(max_w);
    let mut h = 2.0 * PAD_Y + lines.len() as f32 * LINE;
    if title.is_some() {
        h += LINE + if lines.is_empty() { 0.0 } else { TITLE_GAP };
    }
    let h = h.max(2.0 * PAD_Y + LINE);
    let x = (at.0 + OFFSET.0).min(win_w - w - 8.0).max(8.0);
    let mut y = at.1 + OFFSET.1;
    if y + h > win_h - 8.0 {
        // No room below: above the pointer.
        y = (at.1 - 12.0 - h).max(8.0);
    }
    Layout {
        rect: Rect::new(x.round(), y.round(), w, h.ceil()),
        title,
        lines,
    }
}

pub fn draw(l: &Layout, p: &mut Painter, pal: &Palette) {
    let r = l.rect;
    p.shadow(r, 8.0, pal.dark);
    p.frame(r, 8.0, pal.popup, pal.popup_line);
    let mut y = r.y + PAD_Y;
    let inner = r.w - 2.0 * PAD_X;
    if let Some(t) = &l.title {
        p.text(
            Face::Semibold,
            TEXT,
            r.x + PAD_X,
            y + LINE / 2.0,
            t,
            pal.text,
            inner,
        );
        y += LINE + TITLE_GAP;
    }
    for line in &l.lines {
        p.text(
            Face::Ui,
            TEXT,
            r.x + PAD_X,
            y + LINE / 2.0,
            line,
            pal.text,
            inner,
        );
        y += LINE;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_shows_its_start_or_the_heading_the_link_names() {
        let dir = std::env::temp_dir().join(format!("mdview-preview-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("n.md");
        std::fs::write(&f, "# Notes\n\nOpening.\n\n## Part two\n\nDeeper text.\n").unwrap();
        let p = load(&f, None).unwrap();
        assert_eq!((p.title.as_str(), p.body.as_str()), ("Notes", "Opening."));
        let p = load(&f, Some("part-two")).unwrap();
        assert_eq!(
            (p.title.as_str(), p.body.as_str()),
            ("Part two", "Deeper text.")
        );
        // An unknown heading falls back to the start of the file.
        assert_eq!(load(&f, Some("nope")).unwrap().title, "Notes");
        assert!(load(&dir.join("missing.md"), None).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_the_start_of_a_big_file_is_read() {
        let dir = std::env::temp_dir().join(format!("mdview-preview-big-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("big.md");
        let text = format!("# Big\n\nFirst.\n\n{}", "filler line\n\n".repeat(40_000));
        assert!(text.len() as u64 > READ_BYTES);
        std::fs::write(&f, text).unwrap();
        let p = load(&f, None).unwrap();
        assert_eq!(p.title, "Big");
        assert!(p.body.starts_with("First.\nfiller line"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
