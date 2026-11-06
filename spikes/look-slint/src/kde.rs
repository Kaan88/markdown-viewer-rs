//! Reads colours, accent and the UI font from ~/.config/kdeglobals when present.
pub type Rgb = (u8, u8, u8);

#[derive(Clone, Debug)]
pub struct KdeColors {
    pub dark: bool,
    pub window: Rgb,
    pub text: Rgb,
    pub text_dim: Rgb,
    pub accent: Rgb,
    pub hairline: Rgb,
    pub popup: Rgb,
    pub font: Option<String>,
}

fn parse_rgb(s: &str) -> Option<Rgb> {
    let mut it = s.split(',').map(|v| v.trim().parse::<u8>().ok());
    Some((it.next()??, it.next()??, it.next()??))
}

pub fn luminance(c: Rgb) -> f32 {
    let f = |v: u8| {
        let v = v as f32 / 255.0;
        if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * f(c.0) + 0.7152 * f(c.1) + 0.0722 * f(c.2)
}

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    (m(a.0, b.0), m(a.1, b.1), m(a.2, b.2))
}

fn contrast(a: Rgb, b: Rgb) -> f32 {
    let (x, y) = (luminance(a) + 0.05, luminance(b) + 0.05);
    x.max(y) / x.min(y)
}

/// The accent for text: moved towards white or black until it reads at 4.5:1 on the page.
pub fn readable_accent(accent: Rgb, dark: bool) -> Rgb {
    let (bg, to) = if dark { ((0x20, 0x20, 0x20), (255, 255, 255)) } else { ((0xFB, 0xFB, 0xFB), (0, 0, 0)) };
    let mut t = 0.0;
    while t < 1.0 {
        let c = mix(accent, to, t);
        if contrast(c, bg) >= 4.5 { return c; }
        t += 0.05;
    }
    to
}

pub fn parse(text: &str) -> Option<KdeColors> {
    let mut section = "";
    let (mut window, mut text_c, mut inactive, mut view_bg, mut sel, mut accent, mut font) = (None, None, None, None, None, None, None);
    for line in text.lines() {
        let line = line.trim();
        if let Some(s) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = s;
            continue;
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        match (section, k) {
            ("Colors:Window", "BackgroundNormal") => window = parse_rgb(v),
            ("Colors:Window", "ForegroundNormal") => text_c = parse_rgb(v),
            ("Colors:Window", "ForegroundInactive") => inactive = parse_rgb(v),
            ("Colors:View", "BackgroundNormal") => view_bg = parse_rgb(v),
            ("Colors:Selection", "BackgroundNormal") => sel = parse_rgb(v),
            ("General", "AccentColor") => accent = parse_rgb(v),
            ("General", "font") => font = v.split(',').next().map(|s| s.to_string()),
            _ => {}
        }
    }
    let window = window?;
    let text_c = text_c?;
    let dark = luminance(window) < 0.2;
    Some(KdeColors {
        dark,
        window,
        text: text_c,
        text_dim: inactive.unwrap_or(mix(text_c, window, 0.4)),
        accent: accent.or(sel).unwrap_or(if dark { (0x60, 0xCD, 0xFF) } else { (0x00, 0x5F, 0xB8) }),
        hairline: mix(window, text_c, 0.14),
        // Raised surfaces read lighter than the window in a dark scheme.
        popup: if dark { mix(window, (255, 255, 255), 0.06) } else { view_bg.unwrap_or(window) },
        font,
    })
}

pub fn load() -> Option<KdeColors> {
    let home = std::env::var_os("HOME")?;
    let path = std::path::Path::new(&home).join(".config/kdeglobals");
    parse(&std::fs::read_to_string(path).ok()?)
}
