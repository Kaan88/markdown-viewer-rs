//! Emoji shortcodes such as `:rocket:`, replaced in running text the way GitHub does. The table is
//! the common set only (about 186 names), sorted so a lookup is a binary search; an unknown
//! name stays as typed. Code spans, code blocks and link destinations never reach this.

use std::borrow::Cow;

static TABLE: &[(&str, &str)] = &[
    ("+1", "👍"),
    ("-1", "👎"),
    ("100", "💯"),
    ("accept", "🉑"),
    ("airplane", "✈️"),
    ("alarm_clock", "⏰"),
    ("alien", "👽"),
    ("anchor", "⚓"),
    ("angry", "😠"),
    ("apple", "🍎"),
    ("arrow_down", "⬇️"),
    ("arrow_left", "⬅️"),
    ("arrow_right", "➡️"),
    ("arrow_up", "⬆️"),
    ("art", "🎨"),
    ("bangbang", "‼️"),
    ("beer", "🍺"),
    ("beers", "🍻"),
    ("bell", "🔔"),
    ("bike", "🚲"),
    ("birthday", "🎂"),
    ("black_heart", "🖤"),
    ("blue_heart", "💙"),
    ("blush", "😊"),
    ("bomb", "💣"),
    ("book", "📖"),
    ("bookmark", "🔖"),
    ("books", "📚"),
    ("boom", "💥"),
    ("brain", "🧠"),
    ("broken_heart", "💔"),
    ("bug", "🐛"),
    ("building_construction", "🏗️"),
    ("bulb", "💡"),
    ("bus", "🚌"),
    ("calendar", "📆"),
    ("camera", "📷"),
    ("cat", "🐱"),
    ("chart_with_upwards_trend", "📈"),
    ("check", "✔️"),
    ("checkered_flag", "🏁"),
    ("cherry_blossom", "🌸"),
    ("clap", "👏"),
    ("clipboard", "📋"),
    ("cloud", "☁️"),
    ("coffee", "☕"),
    ("computer", "💻"),
    ("confetti_ball", "🎊"),
    ("construction", "🚧"),
    ("cookie", "🍪"),
    ("cry", "😢"),
    ("crying_cat_face", "😿"),
    ("crystal_ball", "🔮"),
    ("dart", "🎯"),
    ("dash", "💨"),
    ("disappointed", "😞"),
    ("dizzy", "💫"),
    ("dog", "🐶"),
    ("exclamation", "❗"),
    ("eyeglasses", "👓"),
    ("eyes", "👀"),
    ("facepalm", "🤦"),
    ("fire", "🔥"),
    ("fireworks", "🎆"),
    ("first_place_medal", "🥇"),
    ("flags", "🎏"),
    ("flushed", "😳"),
    ("folder", "📁"),
    ("gear", "⚙️"),
    ("gem", "💎"),
    ("gift", "🎁"),
    ("globe_with_meridians", "🌐"),
    ("green_heart", "💚"),
    ("grey_question", "❔"),
    ("grin", "😁"),
    ("grinning", "😀"),
    ("hammer", "🔨"),
    ("hammer_and_wrench", "🛠️"),
    ("hand", "✋"),
    ("handshake", "🤝"),
    ("heart", "❤️"),
    ("heart_eyes", "😍"),
    ("heavy_check_mark", "✔️"),
    ("heavy_minus_sign", "➖"),
    ("heavy_plus_sign", "➕"),
    ("hourglass", "⌛"),
    ("house", "🏠"),
    ("hugs", "🤗"),
    ("hushed", "😯"),
    ("information_source", "ℹ️"),
    ("innocent", "😇"),
    ("joy", "😂"),
    ("key", "🔑"),
    ("keyboard", "⌨️"),
    ("kissing_heart", "😘"),
    ("laughing", "😆"),
    ("link", "🔗"),
    ("lock", "🔒"),
    ("loudspeaker", "📢"),
    ("mag", "🔍"),
    ("mag_right", "🔎"),
    ("memo", "📝"),
    ("metal", "🤘"),
    ("microscope", "🔬"),
    ("moneybag", "💰"),
    ("monocle_face", "🧐"),
    ("moon", "🌙"),
    ("mortar_board", "🎓"),
    ("muscle", "💪"),
    ("musical_note", "🎵"),
    ("nerd_face", "🤓"),
    ("new", "🆕"),
    ("no_entry", "⛔"),
    ("no_entry_sign", "🚫"),
    ("ok", "🆗"),
    ("ok_hand", "👌"),
    ("open_book", "📖"),
    ("package", "📦"),
    ("page_facing_up", "📄"),
    ("palm_tree", "🌴"),
    ("paperclip", "📎"),
    ("partying_face", "🥳"),
    ("pencil", "📝"),
    ("pencil2", "✏️"),
    ("penguin", "🐧"),
    ("pensive", "😔"),
    ("pizza", "🍕"),
    ("point_down", "👇"),
    ("point_left", "👈"),
    ("point_right", "👉"),
    ("point_up", "👆"),
    ("poop", "💩"),
    ("pray", "🙏"),
    ("purple_heart", "💜"),
    ("pushpin", "📌"),
    ("question", "❓"),
    ("rabbit", "🐰"),
    ("rainbow", "🌈"),
    ("raised_hands", "🙌"),
    ("recycle", "♻️"),
    ("red_circle", "🔴"),
    ("relaxed", "☺️"),
    ("rocket", "🚀"),
    ("rofl", "🤣"),
    ("rose", "🌹"),
    ("sad", "😥"),
    ("scream", "😱"),
    ("see_no_evil", "🙈"),
    ("shield", "🛡️"),
    ("shrug", "🤷"),
    ("skull", "💀"),
    ("sleeping", "😴"),
    ("smile", "😄"),
    ("smiley", "😃"),
    ("smirk", "😏"),
    ("snake", "🐍"),
    ("sob", "😭"),
    ("sparkles", "✨"),
    ("sparkling_heart", "💖"),
    ("speech_balloon", "💬"),
    ("star", "⭐"),
    ("star2", "🌟"),
    ("stopwatch", "⏱️"),
    ("sunglasses", "😎"),
    ("sunny", "☀️"),
    ("sweat_smile", "😅"),
    ("tada", "🎉"),
    ("thinking", "🤔"),
    ("thumbsdown", "👎"),
    ("thumbsup", "👍"),
    ("tm", "™️"),
    ("toolbox", "🧰"),
    ("trophy", "🏆"),
    ("turtle", "🐢"),
    ("unicorn", "🦄"),
    ("unlock", "🔓"),
    ("v", "✌️"),
    ("warning", "⚠️"),
    ("wave", "👋"),
    ("white_check_mark", "✅"),
    ("wink", "😉"),
    ("wrench", "🔧"),
    ("x", "❌"),
    ("yellow_heart", "💛"),
    ("zap", "⚡"),
    ("zzz", "💤"),
];

fn lookup(name: &str) -> Option<&'static str> {
    TABLE
        .binary_search_by(|(n, _)| (*n).cmp(name))
        .ok()
        .map(|i| TABLE[i].1)
}

/// Replaces every known `:name:` in `text`. Borrows when there is nothing to replace.
pub fn replace(text: &str) -> Cow<'_, str> {
    if !text.contains(':') {
        return Cow::Borrowed(text);
    }
    let mut out: Option<String> = None;
    let mut last = 0;
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b':' {
            let start = i + 1;
            let mut end = start;
            while end < bytes.len()
                && (bytes[end].is_ascii_alphanumeric() || matches!(bytes[end], b'_' | b'+' | b'-'))
            {
                end += 1;
            }
            if end > start && end < bytes.len() && bytes[end] == b':' {
                if let Some(emoji) = lookup(&text[start..end]) {
                    let buf = out.get_or_insert_with(String::new);
                    buf.push_str(&text[last..i]);
                    buf.push_str(emoji);
                    last = end + 1;
                    i = end + 1;
                    continue;
                }
            }
        }
        i += 1;
    }
    match out {
        Some(mut buf) => {
            buf.push_str(&text[last..]);
            Cow::Owned(buf)
        }
        None => Cow::Borrowed(text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_sorted_and_unique() {
        assert!(TABLE.windows(2).all(|w| w[0].0 < w[1].0));
    }

    #[test]
    fn known_names_are_replaced_and_unknown_ones_kept() {
        assert_eq!(replace("ship it :rocket:!"), "ship it 🚀!");
        assert_eq!(replace(":+1: and :-1:"), "👍 and 👎");
        assert_eq!(replace("12:30:45 and :nope: and a:b"), "12:30:45 and :nope: and a:b");
        assert_eq!(replace(":tada::tada:"), "🎉🎉");
        assert!(matches!(replace("no colons"), Cow::Borrowed(_)));
    }
}
