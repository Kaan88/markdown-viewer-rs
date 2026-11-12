//! Ticking a task-list box in the file it came from.
//!
//! The document model does not keep source positions, and the text the parser sees is not the
//! file's bytes (math and front matter are rewritten first). So a box is found in the file by a
//! small scan of its own, and the scan is trusted only when it agrees with the page: it must find
//! exactly as many boxes as the document shows, in the same states. Anything else is refused and
//! the file is left alone. The write is one byte, in place: the space or `x` between the brackets.

use crate::model::{Document, SpanKind};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

/// A `[ ]` or `[x]` task marker in a file: where its middle byte is and what it says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Marker {
    pub at: usize,
    pub checked: bool,
}

/// Why a box was not ticked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToggleError {
    /// The file is UTF-16, where one mark is not one byte.
    Encoding,
    /// The boxes in the file are not the boxes on the page: it changed on disk, or it holds a
    /// box this scan reads differently from the parser.
    Mismatch,
}

/// Every task box the document shows, in document order: its block and whether it is checked.
pub fn document_boxes(doc: &Document) -> Vec<(u32, bool)> {
    let mut boxes = Vec::new();
    for (index, block) in doc.blocks.iter().enumerate() {
        for span in &doc.spans[block.spans.usize()] {
            if let SpanKind::Checkbox { checked } = span.kind {
                boxes.push((index as u32, checked));
            }
        }
    }
    boxes
}

/// The task markers in a Markdown file's bytes: a list marker (after any quote marks and outer
/// list markers on the line), then `[ ]`, `[x]` or `[X]` and a space. Lines inside a fenced code
/// block are skipped.
pub fn markers(bytes: &[u8]) -> Vec<Marker> {
    let bytes_start = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        3
    } else {
        0
    };
    let mut found = Vec::new();
    // The open fence: its character and length.
    let mut fence: Option<(u8, usize)> = None;
    let mut start = bytes_start;
    while start < bytes.len() {
        let end = bytes[start..]
            .iter()
            .position(|&b| b == b'\n')
            .map_or(bytes.len(), |n| start + n);
        let line = &bytes[start..end];
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        let lead = line
            .iter()
            .position(|b| !matches!(b, b' ' | b'\t' | b'>'))
            .unwrap_or(line.len());
        let body = &line[lead..];
        let run = |c: u8| body.iter().take_while(|&&b| b == c).count();
        match fence {
            Some((c, len)) => {
                let n = run(c);
                if n >= len && body[n..].iter().all(|b| matches!(b, b' ' | b'\t')) {
                    fence = None;
                }
            }
            None => {
                let opener = [b'`', b'~'].into_iter().find(|&c| run(c) >= 3);
                if let Some(c) = opener {
                    fence = Some((c, run(c)));
                } else if let Some(at) = marker_in(line) {
                    found.push(Marker {
                        at: start + at,
                        checked: line[at] != b' ',
                    });
                }
            }
        }
        start = end + 1;
    }
    found
}

/// The index in `line` of a task marker's middle byte, if the line starts a task item.
fn marker_in(line: &[u8]) -> Option<usize> {
    let blank = |b: u8| matches!(b, b' ' | b'\t');
    let mut i = 0;
    let mut items = 0;
    loop {
        while i < line.len() && (blank(line[i]) || line[i] == b'>') {
            i += 1;
        }
        let rest = &line[i..];
        let bullet = match rest.first() {
            Some(b'-' | b'*' | b'+') => 1,
            Some(b'0'..=b'9') => {
                let digits = rest.iter().take_while(|b| b.is_ascii_digit()).count();
                match rest.get(digits) {
                    Some(b'.' | b')') if digits <= 9 => digits + 1,
                    _ => 0,
                }
            }
            _ => 0,
        };
        if bullet == 0 || !rest.get(bullet).copied().is_some_and(blank) {
            break;
        }
        items += 1;
        i += bullet;
    }
    let rest = &line[i..];
    let is_marker = items > 0
        && rest.len() >= 4
        && rest[0] == b'['
        && matches!(rest[1], b' ' | b'x' | b'X')
        && rest[2] == b']'
        && blank(rest[3]);
    is_marker.then_some(i + 1)
}

/// Where to write and which byte, to flip box number `index` of a file whose boxes should be in
/// the states `expected` (what the page shows).
pub fn toggle(bytes: &[u8], index: usize, expected: &[bool]) -> Result<(usize, u8), ToggleError> {
    if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        return Err(ToggleError::Encoding);
    }
    let found = markers(bytes);
    let agrees =
        found.len() == expected.len() && found.iter().zip(expected).all(|(m, &e)| m.checked == e);
    match found.get(index) {
        Some(marker) if agrees => Ok((marker.at, if marker.checked { b' ' } else { b'x' })),
        _ => Err(ToggleError::Mismatch),
    }
}

/// Flips box number `index` in the file at `path` and returns its new state. The file is read
/// again here, so a file that changed since the page was drawn is caught by [`toggle`]. Only the
/// one byte is written; the text of a failure is for the reader.
pub fn toggle_in_file(path: &Path, index: usize, expected: &[bool]) -> Result<bool, String> {
    let name = path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let failed = |e: std::io::Error| format!("Could not change {name}: {e}");
    if std::fs::metadata(path)
        .map_err(failed)?
        .permissions()
        .readonly()
    {
        return Err(format!("{name} is read-only"));
    }
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(failed)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(failed)?;
    let (at, byte) = toggle(&bytes, index, expected).map_err(|e| match e {
        ToggleError::Encoding => format!("{name} is UTF-16; its boxes cannot be changed here"),
        ToggleError::Mismatch => {
            format!("{name} does not match the page; nothing was changed")
        }
    })?;
    file.seek(SeekFrom::Start(at as u64)).map_err(failed)?;
    file.write_all(&[byte]).map_err(failed)?;
    file.sync_data().map_err(failed)?;
    Ok(byte == b'x')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn states(text: &str) -> Vec<bool> {
        markers(text.as_bytes()).iter().map(|m| m.checked).collect()
    }

    #[test]
    fn finds_boxes_in_lists_quotes_and_nested_items() {
        let text =
            "- [ ] a\n* [x] b\n  + [X] c\n> - [ ] quoted\n1. [x] one\n12) [ ] two\n- - [ ] both\n";
        assert_eq!(states(text), [false, true, true, false, true, false, false]);
        for marker in markers(text.as_bytes()) {
            assert_eq!(text.as_bytes()[marker.at - 1], b'[');
            assert_eq!(text.as_bytes()[marker.at + 1], b']');
        }
    }

    #[test]
    fn skips_what_is_not_a_task_item() {
        let text = "[ ] no list\n- [y] no\n- [ ]tight\n-[ ] no space\n- text [ ] later\n\
                    ```\n- [ ] in a fence\n```\n~~~~\n- [x] too\n```\n~~~~\n- [x] real\n";
        assert_eq!(states(text), [true]);
    }

    #[test]
    fn crlf_and_a_bom_do_not_move_the_marker() {
        let text = "\u{feff}- [ ] a\r\n- [x] b\r\n";
        let found = markers(text.as_bytes());
        assert_eq!(found.len(), 2);
        assert_eq!(text.as_bytes()[found[0].at], b' ');
        assert_eq!(text.as_bytes()[found[1].at], b'x');
    }

    #[test]
    fn toggle_needs_the_file_to_agree_with_the_page() {
        let text = b"- [ ] a\n- [x] b\n";
        assert_eq!(toggle(text, 0, &[false, true]), Ok((3, b'x')));
        assert_eq!(toggle(text, 1, &[false, true]), Ok((11, b' ')));
        assert_eq!(toggle(text, 1, &[false, false]), Err(ToggleError::Mismatch));
        assert_eq!(toggle(text, 0, &[false]), Err(ToggleError::Mismatch));
        assert_eq!(toggle(text, 2, &[false, true]), Err(ToggleError::Mismatch));
        assert_eq!(
            toggle(&[0xFF, 0xFE, b'-', 0], 0, &[]),
            Err(ToggleError::Encoding)
        );
    }

    #[test]
    fn the_scan_agrees_with_the_parser() {
        let text = "# Tasks\n\n- [ ] one\n- [x] two\n  - [ ] nested\n\n> - [x] quoted\n\n\
                    ```md\n- [ ] code\n```\n\n1. [ ] numbered\n- plain\n";
        let doc = crate::parse(text);
        let page: Vec<bool> = document_boxes(&doc).iter().map(|b| b.1).collect();
        assert_eq!(page, states(text));
        assert_eq!(page.len(), 5);
    }
}
