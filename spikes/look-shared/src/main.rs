//! Stand-in document page for the look spikes. Parses page.md with pulldown-cmark
//! and paints it with the text spike's renderer (parley + swash, LCD masks).
//! Not the future renderer: layout is one simple top-down pass.
use parley::{
    Alignment, AlignmentOptions, FontContext, FontFamily, FontStyle, FontWeight, Layout,
    LayoutContext, LineHeight, PositionedLayoutItem, StyleProperty,
};
use pulldown_cmark::{self as md, Event, Tag, TagEnd};
use std::collections::BTreeSet;
use std::ops::Range;
use text_spike::{Brush, Fb, Renderer};

const W: usize = 780;
const H: usize = 2400;
const MARGIN: f32 = 40.0;
const SCALES: &[(usize, &str)] = &[(100, "1.0"), (150, "1.5"), (170, "1.7"), (220, "2.2")];
const SANS: &str = "Segoe UI, Yu Gothic UI, sans-serif";
const MONO: &str = "Consolas, Yu Gothic UI, monospace";

type C = [u8; 3];
struct Theme {
    name: &'static str,
    bg: C,
    text: C,
    heading: C,
    muted: C,
    accent: C,
    border: C,
    code_bg: C,
    head_bg: C,
    tint: C,
    kw: C,
    strg: C,
}
const LIGHT: Theme = Theme {
    name: "light",
    bg: [0xfb, 0xfb, 0xfb],
    text: [0x24, 0x24, 0x24],
    heading: [0x1a, 0x1a, 0x1a],
    muted: [0x5c, 0x5c, 0x5c],
    accent: [0x00, 0x5f, 0xb8],
    border: [0xe0, 0xe0, 0xe0],
    code_bg: [0xf3, 0xf3, 0xf3],
    head_bg: [0xf3, 0xf3, 0xf3],
    tint: [0xee, 0xf4, 0xfb],
    kw: [0x8a, 0x2b, 0xb0],
    strg: [0x0f, 0x6e, 0x3c],
};
const DARK: Theme = Theme {
    name: "dark",
    bg: [0x20, 0x20, 0x20],
    text: [0xe4, 0xe4, 0xe4],
    heading: [0xf5, 0xf5, 0xf5],
    muted: [0xa6, 0xa6, 0xa6],
    accent: [0x60, 0xcd, 0xff],
    border: [0x3d, 0x3d, 0x3d],
    code_bg: [0x2b, 0x2b, 0x2b],
    head_bg: [0x2b, 0x2b, 0x2b],
    tint: [0x24, 0x30, 0x3a],
    kw: [0xd2, 0xa8, 0xff],
    strg: [0x8b, 0xd6, 0x9c],
};

// ---------- document model ----------
#[derive(Clone, Copy, Default)]
struct F {
    bold: bool,
    italic: bool,
    strike: bool,
    code: bool,
    link: bool,
    muted: bool,
    kw: bool,
    strg: bool,
}
#[derive(Default, Clone)]
struct Inl {
    t: String,
    sp: Vec<(Range<usize>, F)>,
}
impl Inl {
    fn push(&mut self, s: &str, f: F) {
        let a = self.t.len();
        self.t.push_str(s);
        self.sp.push((a..self.t.len(), f));
    }
}
enum Marker {
    Bullet,
    Num(u64),
    Task(bool),
}
enum Block {
    Heading(u8, Inl),
    Para(Inl),
    Item(Marker, Inl),
    Quote(bool, Vec<Inl>),
    Code(String, String),
    Display(String),
    Table(Vec<md::Alignment>, Vec<Vec<Inl>>),
    Footnote(usize, Inl),
}

fn parse(src: &str) -> Vec<Block> {
    use md::Options as O;
    let opts = O::ENABLE_TABLES
        | O::ENABLE_TASKLISTS
        | O::ENABLE_STRIKETHROUGH
        | O::ENABLE_FOOTNOTES
        | O::ENABLE_GFM
        | O::ENABLE_MATH;
    let mut out = Vec::new();
    let mut cur = Inl::default();
    let mut f = F::default();
    let mut quote: Option<(bool, Vec<Inl>)> = None;
    let mut lists: Vec<Option<u64>> = Vec::new();
    let mut task = None;
    let mut code: Option<(String, String)> = None;
    let mut table: Option<(Vec<md::Alignment>, Vec<Vec<Inl>>, Vec<Inl>)> = None;
    let mut foot: Vec<String> = Vec::new();
    let mut in_foot = false;
    fn fnum(foot: &mut Vec<String>, l: &str) -> usize {
        if let Some(i) = foot.iter().position(|x| x == l) {
            return i + 1;
        }
        foot.push(l.to_string());
        foot.len()
    }
    for ev in md::Parser::new_ext(src, opts) {
        match ev {
            Event::Start(t) => match t {
                Tag::BlockQuote(k) => quote = Some((k.is_some(), Vec::new())),
                Tag::List(n) => lists.push(n),
                Tag::CodeBlock(k) => {
                    let lang = match k {
                        md::CodeBlockKind::Fenced(l) => l.to_string(),
                        _ => String::new(),
                    };
                    code = Some((lang, String::new()));
                }
                Tag::Table(a) => table = Some((a, Vec::new(), Vec::new())),
                Tag::Emphasis => f.italic = true,
                Tag::Strong => f.bold = true,
                Tag::Strikethrough => f.strike = true,
                Tag::Link { .. } => f.link = true,
                Tag::Image { .. } => {
                    f.muted = true;
                    cur.push("[image: ", f);
                }
                Tag::FootnoteDefinition(_) => in_foot = true,
                _ => {}
            },
            Event::End(t) => match t {
                TagEnd::Heading(l) => {
                    out.push(Block::Heading(l as u8, std::mem::take(&mut cur)));
                }
                TagEnd::Paragraph => {
                    if let Some(q) = quote.as_mut() {
                        q.1.push(std::mem::take(&mut cur));
                    } else if lists.is_empty() && !in_foot {
                        out.push(Block::Para(std::mem::take(&mut cur)));
                    }
                }
                TagEnd::BlockQuote(_) => {
                    let (k, p) = quote.take().unwrap();
                    out.push(Block::Quote(k, p));
                }
                TagEnd::Item => {
                    let m = match (task.take(), lists.last_mut()) {
                        (Some(b), _) => Marker::Task(b),
                        (None, Some(Some(n))) => {
                            *n += 1;
                            Marker::Num(*n - 1)
                        }
                        _ => Marker::Bullet,
                    };
                    out.push(Block::Item(m, std::mem::take(&mut cur)));
                }
                TagEnd::List(_) => {
                    lists.pop();
                }
                TagEnd::CodeBlock => {
                    let (l, c) = code.take().unwrap();
                    out.push(Block::Code(l, c));
                }
                TagEnd::TableCell => {
                    table.as_mut().unwrap().2.push(std::mem::take(&mut cur));
                }
                TagEnd::TableHead | TagEnd::TableRow => {
                    let t = table.as_mut().unwrap();
                    let row = std::mem::take(&mut t.2);
                    t.1.push(row);
                }
                TagEnd::Table => {
                    let (a, r, _) = table.take().unwrap();
                    out.push(Block::Table(a, r));
                }
                TagEnd::Emphasis => f.italic = false,
                TagEnd::Strong => f.bold = false,
                TagEnd::Strikethrough => f.strike = false,
                TagEnd::Link => f.link = false,
                TagEnd::Image => {
                    cur.push("]", f);
                    f.muted = false;
                }
                TagEnd::FootnoteDefinition => {
                    in_foot = false;
                    let n = foot.len().max(1);
                    out.push(Block::Footnote(n, std::mem::take(&mut cur)));
                }
                _ => {}
            },
            Event::Text(s) => {
                if let Some(c) = code.as_mut() {
                    c.1.push_str(&s);
                } else {
                    cur.push(&s, f);
                }
            }
            Event::Code(s) | Event::InlineMath(s) => cur.push(&s, F { code: true, ..f }),
            Event::DisplayMath(s) => out.push(Block::Display(s.trim().to_string())),
            Event::SoftBreak => cur.push(" ", f),
            Event::HardBreak => cur.push("\n", f),
            Event::TaskListMarker(b) => task = Some(b),
            Event::FootnoteReference(l) => {
                let n = fnum(&mut foot, &l);
                cur.push(&format!("[{n}]"), F { link: true, ..f });
            }
            _ => {}
        }
    }
    // An empty paragraph is what is left around a $$ block.
    out.retain(|b| !matches!(b, Block::Para(p) if p.t.trim().is_empty()));
    out
}

// ---------- painting ----------
struct Txt {
    mono: bool,
    size: f32,
    lh: f32,
    weight: f32,
    color: C,
    max: Option<f32>,
}
struct Page<'a> {
    fc: &'a mut FontContext,
    lc: &'a mut LayoutContext<Brush>,
    r: Renderer,
    fb: Fb,
    s: f32,
    th: &'a Theme,
    fonts: &'a mut BTreeSet<String>,
    y: f32,
}
fn br(c: C) -> Brush {
    [c[0], c[1], c[2], 255]
}

impl Page<'_> {
    fn px(&self, v: f32) -> f32 {
        (v * self.s).round()
    }
    fn hair(&self) -> f32 {
        self.s.round().max(1.0)
    }
    fn lay(&mut self, i: &Inl, o: &Txt) -> Layout<Brush> {
        let th = self.th;
        let mut b = self.lc.ranged_builder(self.fc, &i.t, self.s, true);
        let base = if o.mono { MONO } else { SANS };
        b.push_default(StyleProperty::FontFamily(FontFamily::from(base)));
        b.push_default(StyleProperty::FontSize(o.size));
        b.push_default(StyleProperty::LineHeight(LineHeight::FontSizeRelative(o.lh)));
        b.push_default(StyleProperty::FontWeight(FontWeight::new(o.weight)));
        b.push_default(StyleProperty::Brush(br(o.color)));
        for (r, f) in &i.sp {
            let r = r.clone();
            if f.bold {
                b.push(StyleProperty::FontWeight(FontWeight::BOLD), r.clone());
            }
            if f.italic {
                b.push(StyleProperty::FontStyle(FontStyle::Italic), r.clone());
            }
            if f.strike {
                b.push(StyleProperty::Strikethrough(true), r.clone());
            }
            if f.link {
                b.push(StyleProperty::Underline(true), r.clone());
                b.push(StyleProperty::Brush(br(th.accent)), r.clone());
            }
            if f.muted {
                b.push(StyleProperty::Brush(br(th.muted)), r.clone());
            }
            if f.kw {
                b.push(StyleProperty::Brush(br(th.kw)), r.clone());
            }
            if f.strg {
                b.push(StyleProperty::Brush(br(th.strg)), r.clone());
            }
            if f.code {
                b.push(StyleProperty::FontFamily(FontFamily::from(MONO)), r.clone());
                b.push(StyleProperty::FontSize(o.size * 0.92), r.clone());
            }
        }
        let mut l = b.build(&i.t);
        l.break_all_lines(o.max);
        l.align(Alignment::Start, AlignmentOptions::default());
        for ln in l.lines() {
            for run in ln.runs() {
                let f = run.font();
                if let Some(fr) = swash::FontRef::from_index(f.data.as_ref(), f.index as usize) {
                    if let Some(n) = fr.localized_strings().find_by_id(swash::StringId::Full, None) {
                        self.fonts.insert(n.to_string());
                    }
                }
            }
        }
        l
    }
    /// Inline-code chips, then glyphs, then decorations. The renderer draws neither of the others.
    fn draw(&mut self, l: &Layout<Brush>, i: &Inl, x: f32, y: f32) {
        let mut decos = Vec::new();
        for ln in l.lines() {
            for it in ln.items() {
                let PositionedLayoutItem::GlyphRun(g) = it else { continue };
                let sz = g.run().font_size();
                let (gx, base, adv) = (x + g.offset(), y + g.baseline(), g.advance());
                let start = g.run().text_range().start;
                let chip = i.sp.iter().any(|(r, f)| f.code && r.contains(&start));
                if chip {
                    let pad = self.px(3.0);
                    let (t, b) = ((base - sz * 0.92).round(), (base + sz * 0.32).round());
                    let c = self.th.code_bg;
                    let e = self.th.border;
                    self.rrect(gx - pad, t, adv + 2.0 * pad, b - t, self.px(3.0), e);
                    let h = self.hair();
                    self.rrect(gx - pad + h, t + h, adv + 2.0 * pad - 2.0 * h, b - t - 2.0 * h, self.px(2.0), c);
                }
                let st = g.style();
                let th = (sz * 0.065).round().max(1.0);
                if let Some(d) = &st.underline {
                    decos.push((gx, (base + sz * 0.13).round(), adv, th, d.brush));
                }
                if let Some(d) = &st.strikethrough {
                    decos.push((gx, (base - sz * 0.3).round(), adv, th, d.brush));
                }
            }
        }
        self.r.draw_layout(&mut self.fb, l, x, y);
        for (dx, dy, w, t, b) in decos {
            self.rrect(dx, dy, w, t, 0.0, [b[0], b[1], b[2]]);
        }
    }
    /// Antialiased rounded rectangle.
    fn rrect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, c: C) {
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let (fw, fh) = (self.fb.w as i32, self.fb.h as i32);
        let (x0, y0) = ((x.floor() as i32).max(0), (y.floor() as i32).max(0));
        let (x1, y1) = (((x + w).ceil() as i32).min(fw), ((y + h).ceil() as i32).min(fh));
        let (cx, cy) = (x + w / 2.0, y + h / 2.0);
        for py in y0..y1 {
            for px in x0..x1 {
                let qx = (px as f32 + 0.5 - cx).abs() - (w / 2.0 - r);
                let qy = (py as f32 + 0.5 - cy).abs() - (h / 2.0 - r);
                let d = qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - r;
                self.blend(px, py, c, (0.5 - d).clamp(0.0, 1.0));
            }
        }
    }
    fn blend(&mut self, x: i32, y: i32, c: C, a: f32) {
        if a <= 0.0 {
            return;
        }
        let o = (y as usize * self.fb.w + x as usize) * 4;
        for (i, v) in [c[2], c[1], c[0]].iter().enumerate() {
            let d = self.fb.px[o + i] as f32;
            self.fb.px[o + i] = (d + (*v as f32 - d) * a + 0.5) as u8;
        }
    }
    /// Antialiased thick line, for the check mark.
    fn line(&mut self, a: (f32, f32), b: (f32, f32), t: f32, c: C) {
        let (x0, x1) = ((a.0.min(b.0) - t).floor() as i32, (a.0.max(b.0) + t).ceil() as i32);
        let (y0, y1) = ((a.1.min(b.1) - t).floor() as i32, (a.1.max(b.1) + t).ceil() as i32);
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        for py in y0..y1 {
            for px in x0..x1 {
                let (qx, qy) = (px as f32 + 0.5 - a.0, py as f32 + 0.5 - a.1);
                let u = ((qx * dx + qy * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
                let d = (qx - u * dx).hypot(qy - u * dy) - t / 2.0;
                self.blend(px, py, c, (0.5 - d).clamp(0.0, 1.0));
            }
        }
    }
    fn body(&self, max: f32) -> Txt {
        Txt { mono: false, size: 16.0, lh: 1.6, weight: 400.0, color: self.th.text, max: Some(max) }
    }
    fn gap(&mut self, v: f32) {
        self.y += self.px(v);
    }

    fn page(&mut self, blocks: &[Block]) {
        let left = self.px(MARGIN);
        let cw = self.fb.w as f32 - 2.0 * left;
        self.y = self.px(36.0);
        for (idx, b) in blocks.iter().enumerate() {
            let next_item = matches!(blocks.get(idx + 1), Some(Block::Item(..)));
            match b {
                Block::Heading(lv, i) => {
                    let (size, top, bot) = match lv {
                        1 => (34.0, 0.0, 14.0),
                        2 => (24.0, 22.0, 12.0),
                        _ => (18.0, 12.0, 8.0),
                    };
                    self.gap(top);
                    let o = Txt { size, lh: 1.25, weight: 600.0, color: self.th.heading, ..self.body(cw) };
                    let l = self.lay(i, &o);
                    self.draw(&l, i, left, self.y);
                    self.y += l.height().round();
                    self.gap(bot);
                }
                Block::Para(i) => {
                    let o = self.body(cw);
                    let l = self.lay(i, &o);
                    self.draw(&l, i, left, self.y);
                    self.y += l.height().round();
                    self.gap(16.0);
                }
                Block::Item(m, i) => {
                    let ind = self.px(26.0);
                    let o = self.body(cw - ind);
                    let l = self.lay(i, &o);
                    let base = self.y + l.get(0).map_or(0.0, |x| x.metrics().baseline);
                    self.marker(m, left, base);
                    self.draw(&l, i, left + ind, self.y);
                    self.y += l.height().round();
                    self.gap(if next_item { 4.0 } else { 16.0 });
                }
                Block::Quote(alert, ps) => self.quote(*alert, ps, left, cw),
                Block::Code(lang, c) => self.code(lang, c.trim_end(), left, cw),
                Block::Display(s) => {
                    let mut i = Inl::default();
                    i.push(s, F::default());
                    let o = Txt { mono: true, size: 15.0, lh: 1.5, ..self.body(cw) };
                    let l = self.lay(&i, &o);
                    self.draw(&l, &i, left + ((cw - l.width()) / 2.0).round(), self.y);
                    self.y += l.height().round();
                    self.gap(16.0);
                }
                Block::Table(a, rows) => self.table(a, rows, left, cw),
                Block::Footnote(n, i) => {
                    self.gap(8.0);
                    let h = self.hair();
                    let c = self.th.border;
                    self.rrect(left, self.y, cw, h, 0.0, c);
                    self.gap(12.0);
                    let mut t = Inl::default();
                    t.push(&format!("{n}. "), F::default());
                    let d = t.t.len();
                    t.t.push_str(&i.t);
                    for (r, f) in &i.sp {
                        t.sp.push((r.start + d..r.end + d, *f));
                    }
                    let o = Txt { size: 14.0, color: self.th.muted, ..self.body(cw) };
                    let l = self.lay(&t, &o);
                    self.draw(&l, &t, left, self.y);
                    self.y += l.height().round();
                }
            }
        }
    }
    fn marker(&mut self, m: &Marker, x: f32, base: f32) {
        let th = self.th;
        match m {
            Marker::Bullet => {
                let d = self.px(5.0);
                let (bx, by) = (x + self.px(8.0), (base - self.px(5.0) - d / 2.0).round());
                self.rrect(bx, by, d, d, d / 2.0, th.muted);
            }
            Marker::Num(n) => {
                let mut i = Inl::default();
                i.push(&format!("{n}."), F::default());
                let o = Txt { color: th.muted, max: None, ..self.body(0.0) };
                let l = self.lay(&i, &o);
                let b0 = l.get(0).map_or(0.0, |x| x.metrics().baseline);
                self.draw(&l, &i, x, (base - b0).round());
            }
            Marker::Task(on) => {
                let d = self.px(16.0);
                let (bx, by) = (x, (base - self.px(13.0)).round());
                let r = self.px(3.0);
                if *on {
                    self.rrect(bx, by, d, d, r, th.accent);
                    let c = th.bg;
                    let w = self.px(1.7).max(1.5);
                    let p = |u: f32, v: f32| (bx + d * u, by + d * v);
                    self.line(p(0.24, 0.52), p(0.42, 0.70), w, c);
                    self.line(p(0.42, 0.70), p(0.76, 0.32), w, c);
                } else {
                    let h = self.hair();
                    self.rrect(bx, by, d, d, r, th.muted);
                    self.rrect(bx + h, by + h, d - 2.0 * h, d - 2.0 * h, (r - h).max(0.0), th.bg);
                }
            }
        }
    }
    fn quote(&mut self, alert: bool, ps: &[Inl], left: f32, cw: f32) {
        let th = self.th;
        let pad = self.px(if alert { 12.0 } else { 2.0 });
        let inx = self.px(18.0);
        let mut ls = Vec::new();
        let mut h = 2.0 * pad;
        let tw = cw - inx - self.px(14.0);
        let title = if alert {
            let mut t = Inl::default();
            t.push("Note", F::default());
            let o = Txt { weight: 600.0, color: th.accent, ..self.body(tw) };
            let l = self.lay(&t, &o);
            h += l.height().round();
            Some((t, l))
        } else {
            None
        };
        let color = if alert { th.text } else { th.muted };
        for p in ps {
            let o = Txt { color, ..self.body(tw) };
            let l = self.lay(p, &o);
            h += l.height().round();
            ls.push(l);
        }
        if alert {
            self.rrect(left, self.y, cw, h, self.px(6.0), th.tint);
        }
        let bar = self.px(3.0);
        let c = if alert { th.accent } else { th.border };
        self.rrect(left, self.y, bar, h, bar / 2.0, c);
        let mut y = self.y + pad;
        if let Some((t, l)) = &title {
            self.draw(l, t, left + inx, y);
            y += l.height().round();
        }
        for (p, l) in ps.iter().zip(&ls) {
            self.draw(l, p, left + inx, y);
            y += l.height().round();
        }
        self.y += h;
        self.gap(16.0);
    }
    fn code(&mut self, lang: &str, c: &str, left: f32, cw: f32) {
        let th = self.th;
        let i = highlight(lang, c);
        let (px_, py) = (self.px(16.0), self.px(13.0));
        let o = Txt { mono: true, size: 14.0, lh: 1.5, max: None, ..self.body(0.0) };
        let l = self.lay(&i, &o);
        assert!(l.width() <= cw - 2.0 * px_, "code line too wide");
        let h = l.height().round() + 2.0 * py;
        let (r, hr) = (self.px(6.0), self.hair());
        self.rrect(left, self.y, cw, h, r, th.border);
        self.rrect(left + hr, self.y + hr, cw - 2.0 * hr, h - 2.0 * hr, (r - hr).max(0.0), th.code_bg);
        self.draw(&l, &i, left + px_, self.y + py);
        if !lang.is_empty() {
            let mut t = Inl::default();
            t.push(lang, F::default());
            let o = Txt { size: 12.0, lh: 1.3, color: th.muted, max: None, ..self.body(0.0) };
            let tl = self.lay(&t, &o);
            let x = left + cw - self.px(12.0) - tl.width().ceil();
            self.draw(&tl, &t, x, self.y + self.px(8.0));
        }
        self.y += h;
        self.gap(16.0);
    }
    fn table(&mut self, aligns: &[md::Alignment], rows: &[Vec<Inl>], left: f32, cw: f32) {
        let th = self.th;
        let (cx, cy, hr) = (self.px(13.0), self.px(8.0), self.hair());
        let n = aligns.len();
        let mut lay = Vec::new();
        let mut nat = vec![0f32; n];
        for (ri, row) in rows.iter().enumerate() {
            let mut out = Vec::new();
            for (ci, cell) in row.iter().enumerate() {
                let w = if ri == 0 { 600.0 } else { 400.0 };
                let o = Txt { size: 15.0, lh: 1.45, weight: w, max: None, ..self.body(0.0) };
                let l = self.lay(cell, &o);
                nat[ci] = nat[ci].max(l.width().ceil());
                out.push(l);
            }
            lay.push(out);
        }
        let total: f32 = nat.iter().sum::<f32>() + 2.0 * cx * n as f32;
        assert!(total <= cw - 2.0 * hr, "table too wide");
        let extra = ((cw - 2.0 * hr - total) / n as f32).floor();
        let col: Vec<f32> = nat.iter().map(|w| w + 2.0 * cx + extra).collect();
        let hs: Vec<f32> = lay
            .iter()
            .map(|r| r.iter().map(|l| l.height().round()).fold(0.0, f32::max) + 2.0 * cy)
            .collect();
        let h = hs.iter().sum::<f32>() + hs.len() as f32 * hr + hr;
        let r = self.px(6.0);
        let (y0, inner) = (self.y, (r - hr).max(0.0));
        self.rrect(left, y0, cw, h, r, th.border);
        self.rrect(left + hr, y0 + hr, cw - 2.0 * hr, h - 2.0 * hr, inner, th.bg);
        self.rrect(left + hr, y0 + hr, cw - 2.0 * hr, hs[0], inner, th.head_bg);
        self.rrect(left + hr, y0 + hr + inner, cw - 2.0 * hr, hs[0] - inner, 0.0, th.head_bg);
        let mut y = y0 + hr;
        for (ri, row) in lay.iter().enumerate() {
            let mut x = left + hr;
            for (ci, l) in row.iter().enumerate() {
                let dx = match aligns[ci] {
                    md::Alignment::Center => ((col[ci] - l.width()) / 2.0).round(),
                    md::Alignment::Right => (col[ci] - cx - l.width()).round(),
                    _ => cx,
                };
                self.draw(l, &rows[ri][ci], x + dx, y + cy);
                x += col[ci];
            }
            y += hs[ri];
            if ri + 1 < lay.len() {
                self.rrect(left + hr, y, cw - 2.0 * hr, hr, 0.0, th.border);
            }
            y += hr;
        }
        self.y += h;
        self.gap(16.0);
    }
}

/// Just enough colouring for the Rust sample; a stand-in, not a highlighter.
fn highlight(lang: &str, c: &str) -> Inl {
    let mut i = Inl::default();
    if lang != "rust" {
        i.push(c, F::default());
        return i;
    }
    let mut rest = c;
    while !rest.is_empty() {
        if let Some(q) = rest.strip_prefix('"') {
            let e = q.find('"').map_or(rest.len(), |e| e + 2);
            i.push(&rest[..e], F { strg: true, ..F::default() });
            rest = &rest[e..];
            continue;
        }
        let e = rest
            .find(|ch: char| !(ch.is_alphanumeric() || ch == '_' || ch == '!'))
            .unwrap_or(rest.len());
        if e > 0 {
            let w = &rest[..e];
            let kw = matches!(w, "fn" | "let" | "println!");
            i.push(w, F { kw, ..F::default() });
            rest = &rest[e..];
        } else {
            let n = rest.chars().next().unwrap().len_utf8();
            i.push(&rest[..n], F::default());
            rest = &rest[n..];
        }
    }
    i
}

fn main() {
    let root = env!("CARGO_MANIFEST_DIR");
    let src = std::fs::read_to_string(format!("{root}/page.md")).unwrap();
    let blocks = parse(&src);
    let out = root;
    let mut fc = FontContext::new();
    let mut lc = LayoutContext::<Brush>::new();
    let mut fonts = BTreeSet::new();
    for th in [&LIGHT, &DARK] {
        for &(pct, label) in SCALES {
            let (w, h) = (W * pct / 100, H * pct / 100);
            let mut fb = Fb::new(w, h);
            fb.clear(th.bg);
            let mut p = Page {
                fc: &mut fc,
                lc: &mut lc,
                r: Renderer::new(64 << 20),
                fb,
                s: pct as f32 / 100.0,
                th,
                fonts: &mut fonts,
                y: 0.0,
            };
            p.page(&blocks);
            let end = p.y / p.s;
            println!("{} @{label}: {w}x{h}, content ends at {:.1} logical px of {H}", th.name, end);
            assert!(p.y <= h as f32, "content overflows the page");
            p.fb.write_png(&format!("{out}/content-{}@{label}.png", th.name));
            std::fs::write(format!("{out}/content-{}@{label}-{w}x{h}.bgra", th.name), &p.fb.px).unwrap();
        }
    }
    println!("fonts used: {fonts:?}");
}
