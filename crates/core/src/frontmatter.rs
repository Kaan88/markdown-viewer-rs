//! YAML front matter, shown the way GitHub shows it: a two-column table of the top-level
//! `key: value` pairs. Anything more elaborate than that (nested maps, lists, block scalars) is
//! shown as a `yaml` code block instead, so nothing is dropped and nothing is guessed at.
//!
//! This runs on the source text before parsing and returns replacement Markdown, so the rest of
//! the pipeline never sees the `---` fences.

/// How much of the start of a file may be front matter. A bigger block is read as ordinary text.
const MAX_BYTES: usize = 16 * 1024;

/// If `src` opens with a front matter block, returns the Markdown that replaces it and the number of
/// source bytes it used. The block is `---` on the first line, any lines, then `---` or `...`.
pub fn take(src: &str) -> Option<(String, usize)> {
    let rest = src.strip_prefix("---")?;
    let rest = rest
        .strip_prefix("\r\n")
        .or_else(|| rest.strip_prefix('\n'))?;
    let head = &rest[..rest.len().min(MAX_BYTES)];
    let mut body_end = None;
    let mut consumed = src.len() - rest.len();
    let mut offset = 0;
    for line in head.split_inclusive('\n') {
        let text = line.trim_end_matches(['\n', '\r']);
        if text == "---" || text == "..." {
            body_end = Some(offset);
            consumed += offset + line.len();
            break;
        }
        offset += line.len();
    }
    let body = &rest[..body_end?];
    if body.trim().is_empty() {
        return None;
    }
    // The first real line has to be `key:` or `key: value`, so `---` / `Foo` / `---` stays a rule
    // and a heading, as CommonMark says.
    let first = body
        .lines()
        .map(str::trim_end)
        .find(|l| !l.is_empty() && !l.starts_with('#'))?;
    let key_ok = first.split_once(':').is_some_and(|(key, value)| {
        !key.is_empty()
            && key
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'))
            && (value.is_empty() || value.starts_with(' '))
    });
    if !key_ok {
        return None;
    }
    let mut out = match table(body) {
        Some(table) => table,
        None => format!("```yaml\n{}\n```\n", body.trim_end()),
    };
    out.push('\n');
    Some((out, consumed))
}

fn table(body: &str) -> Option<String> {
    let mut rows = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim_end();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with([' ', '\t', '-']) {
            return None; // nested, a list or a continuation
        }
        let (key, value) = trimmed.split_once(':')?;
        if key.is_empty()
            || !key
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'))
        {
            return None;
        }
        let value = value.trim();
        if value.is_empty() || matches!(value, "|" | ">" | "|-" | ">-" | "|+" | ">+") {
            return None; // a nested value or a block scalar starts here
        }
        rows.push((key.to_string(), unquote(value)));
    }
    if rows.is_empty() {
        return None;
    }
    let mut out = String::from("| Property | Value |\n| --- | --- |\n");
    for (key, value) in rows {
        out.push_str(&format!("| {} | {} |\n", cell(&key), cell(&value)));
    }
    Some(out)
}

fn unquote(value: &str) -> String {
    let bytes = value.as_bytes();
    if bytes.len() >= 2 && matches!(bytes[0], b'"' | b'\'') && bytes[bytes.len() - 1] == bytes[0] {
        return value[1..value.len() - 1].to_string();
    }
    value.to_string()
}

fn cell(text: &str) -> String {
    text.replace('\\', "\\\\").replace('|', "\\|")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_mapping_becomes_a_table() {
        let (md, used) =
            take("---\ntitle: \"Hello | world\"\ntags: [a, b]\n---\n# Body\n").unwrap();
        assert!(md.contains("| title | Hello \\| world |"));
        assert!(md.contains("| tags | [a, b] |"));
        assert_eq!(
            &"---\ntitle: \"Hello | world\"\ntags: [a, b]\n---\n# Body\n"[used..],
            "# Body\n"
        );
    }

    #[test]
    fn anything_nested_is_shown_as_yaml() {
        let (md, _) = take("---\ntags:\n  - a\n  - b\n---\ntext").unwrap();
        assert!(md.starts_with("```yaml\ntags:\n  - a\n  - b\n```"));
    }

    #[test]
    fn crlf_and_the_dots_closer_work() {
        let src = "---\r\nname: x\r\n...\r\nbody";
        let (md, used) = take(src).unwrap();
        assert!(md.contains("| name | x |"));
        assert_eq!(&src[used..], "body");
    }

    #[test]
    fn things_that_are_not_front_matter_are_left_alone() {
        assert!(take("# Title\n---\na: b\n---\n").is_none()); // not at the start
        assert!(take("---\na: b\n").is_none()); // never closed
        assert!(take("---\n---\nbody").is_none()); // empty
        assert!(take("----\na: b\n---\n").is_none()); // four dashes is a rule
        assert!(take("---\nno colon here\n---\n").is_none());
        assert!(take("---\nFoo\n---\nbar\n---\nbaz\n").is_none());
    }
}
