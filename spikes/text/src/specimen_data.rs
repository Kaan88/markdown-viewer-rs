// Shared by the Rust renderer and the Qt reference (include!d by both).
pub const BODY: &str = "Noto Sans";
pub const MONO: &str = "Noto Sans Mono";
pub const W: f32 = 640.0;
pub const H: f32 = 390.0;
pub const MARGIN: f32 = 20.0;
pub const SCALES: &[(u32, f32)] = &[(100, 1.0), (150, 1.5), (170, 1.7), (220, 2.2)];
// (name, background rgb, text rgb)
pub const THEMES: &[(&str, [u8; 3], [u8; 3])] = &[
    ("light", [0xff, 0xff, 0xff], [0x1f, 0x23, 0x28]),
    ("dark", [0x0d, 0x11, 0x17], [0xe6, 0xed, 0xf3]),
];
// (mono?, logical px, baseline y, text)
pub const LINES: &[(bool, f32, f32, &str)] = &[
    (false, 28.0, 44.0, "Reading comfort 28px"),
    (false, 16.0, 78.0, "The quick brown fox jumps over the lazy dog, then naps."),
    (false, 16.0, 104.0, "Sphinx of black quartz, judge my vow: 0123456789 (fi fl ffi)."),
    (false, 13.0, 128.0, "Small print at thirteen pixels keeps its shape, or it does not."),
    (false, 16.0, 160.0, "Türkçe: İıĞğŞş. Işık ılık süt içti; İSTANBUL ĞÜŞİÖÇ ğüşiöç."),
    (false, 16.0, 192.0, "日本語: 吾輩は猫である。名前はまだ無い。ひらがな カタカナ 漢字"),
    (false, 16.0, 224.0, "Emoji: 😀 🎉 🚀 👍🏽 👨‍👩‍👧‍👦 🇹🇷 ❤️ ✅ fin."),
    (false, 18.0, 258.0, "مرحبا بالعالم، هذا نص تجريبي للقراءة باللغة العربية"),
    (false, 16.0, 288.0, "Mixed: Arabic العربية inside English, 2026 stays ordered."),
    (true, 14.0, 322.0, "fn main() { println!(\"Hello, {}!\", name); } // 0O1lI"),
    (true, 14.0, 344.0, "let x: Vec<u8> = a.iter().map(|b| b ^ 0x5f).collect();"),
    (true, 14.0, 366.0, "if (a != b && c >= d) return x -> y; // İıĞğŞş"),
];
