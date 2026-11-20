//! An editor showing some blocks as source, in place (`DocumentRenderer::set_gap`): the blocks
//! it covers become one empty room of the asked height, the rest of the page moves by the
//! difference and is drawn as before, and clearing it puts the blocks back.

use mdview_render::{DocumentRenderer, Gap, LIGHT, Options};

const TEXT: &str = "# Title\n\
\n\
First paragraph.\n\
\n\
- one\n\
- two\n\
- three\n\
\n\
Second paragraph,\n\
on two lines.\n\
\n\
Last paragraph.\n";

fn options() -> Options {
    Options {
        width: 700.0,
        scale: 1.0,
        theme: LIGHT,
        max_height: None,
        base_dir: None,
    }
}

/// `(first line, y, height)` of each top-level block after a frame.
fn spans(r: &mut DocumentRenderer, doc: &mdview_core::Document) -> Vec<(u32, f32, f32)> {
    r.view_frame(doc, &options(), 0.0, 2000.0);
    r.page_layout()
        .unwrap()
        .blocks()
        .iter()
        .map(|b| (doc.blocks[b.block as usize].line, b.y, b.height))
        .collect()
}

fn shown() -> (DocumentRenderer, mdview_core::Document) {
    let doc = mdview_core::parse(TEXT);
    let mut r = DocumentRenderer::new();
    r.layout(&doc, &options());
    (r, doc)
}

#[test]
fn the_covered_blocks_become_one_room_and_the_page_below_moves_by_the_difference() {
    let (mut r, doc) = shown();
    let before = spans(&mut r, &doc);
    let lines: Vec<u32> = before.iter().map(|s| s.0).collect();
    assert_eq!(lines, [1, 3, 5, 9, 12]);

    // The list and the paragraph after it (lines 5 to 10) as one source box 150 px tall.
    r.set_gap(
        &doc,
        Some(Gap {
            first_line: 5,
            last_line: 10,
            height: 150.0,
        }),
    );
    let after = spans(&mut r, &doc);
    assert_eq!(after[0], before[0]);
    assert_eq!(after[1], before[1]);
    assert_eq!(
        after[2].1, before[2].1,
        "the room starts where the list did"
    );
    assert_eq!(after[2].2, 150.0);
    assert_eq!(after[3].2, 0.0, "the second covered block takes no room");
    assert_eq!(after[4].1, after[2].1 + 150.0);
    assert_eq!(r.gap_y(&doc), Some(before[2].1));

    // Cleared: the page is as it was.
    r.set_gap(&doc, None);
    assert_eq!(spans(&mut r, &doc), before);
    assert_eq!(r.gap_y(&doc), None);
}

#[test]
fn nothing_is_drawn_or_hit_in_the_room() {
    let (mut r, doc) = shown();
    let before = spans(&mut r, &doc);
    let (y, h) = (before[2].1, 150.0);
    r.set_gap(
        &doc,
        Some(Gap {
            first_line: 5,
            last_line: 8,
            height: h,
        }),
    );
    let frame = r.view_frame(&doc, &options(), 0.0, 2000.0);
    let bmp = &frame.bitmap;
    let row = bmp.width as usize * 4;
    let bg = [LIGHT.bg[2], LIGHT.bg[1], LIGHT.bg[0]];
    for yy in (y as usize + 2)..(y + h) as usize - 2 {
        for px in bmp.pixels[yy * row..(yy + 1) * row].as_chunks::<4>().0 {
            assert_eq!(&px[..3], &bg, "ink in the room at row {yy}");
        }
    }
    let hit = r.hit_test(&doc, &options(), 350.0, y + h / 2.0);
    assert!(hit.is_none_or(|h| !h.on_text), "{hit:?}");
}

#[test]
fn a_block_gives_its_source_lines() {
    let (mut r, doc) = shown();
    spans(&mut r, &doc);
    let blocks: Vec<u32> = r
        .page_layout()
        .unwrap()
        .blocks()
        .iter()
        .map(|b| b.block)
        .collect();
    assert_eq!(r.block_lines(&doc, blocks[2]), Some((5, Some(9))));
    assert_eq!(r.block_lines(&doc, blocks[4]), Some((12, None)));
}
