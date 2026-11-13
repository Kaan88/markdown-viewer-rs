//! Floated images across blocks: the blocks after `<img align="left|right">` run beside it, and
//! the layout is the same whatever order the blocks were measured in. A block beside a float
//! depends on the blocks before it, so every lazy path is checked against the one-pass page:
//! jumps into the middle of a floated run, refinement, band scrolling, an image that arrives
//! late, a toggled `<details>` beside a float and a reload that removes the float.

use mdview_render::{DecodedImage, DocumentRenderer, Jump, LIGHT, Options, PageLayout, render};
use std::path::PathBuf;
use std::time::Duration;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn options(width: f32, scale: f32) -> Options {
    Options {
        width,
        scale,
        theme: LIGHT,
        max_height: None,
        base_dir: Some(fixtures()),
    }
}

fn fixture() -> mdview_core::Document {
    mdview_core::parse_bytes(&std::fs::read(fixtures().join("floats.md")).unwrap())
}

const WORDS: &str = "Words enough to wrap onto a second line beside the picture and a third. ";

/// A tall image floated at `side`, then paragraphs, a heading, a list and more paragraphs.
fn run_doc(side: &str, image: &str) -> mdview_core::Document {
    let mut md = format!(
        "# Top\n\nBefore the float.\n\n<img src=\"{image}\" width=\"200\" align=\"{side}\">\n\n"
    );
    for i in 0..3 {
        md.push_str(&format!("Paragraph {i}. {WORDS}\n\n"));
    }
    md.push_str("## Heading beside\n\n- item one\n- item two with [a link](https://example.com/x) in it\n\n");
    for i in 3..40 {
        md.push_str(&format!("Paragraph {i}. {}\n\n", WORDS.repeat(3)));
    }
    mdview_core::parse(&md)
}

fn refine_all(renderer: &mut DocumentRenderer, doc: &mdview_core::Document, options: &Options) {
    let mut slices = 0;
    while renderer.refine(doc, options, Duration::from_millis(2)) {
        slices += 1;
        assert!(slices < 100_000, "refinement ends");
    }
}

fn eager(doc: &mdview_core::Document, options: &Options) -> PageLayout {
    let mut renderer = DocumentRenderer::new();
    renderer.layout_exact(doc, options).clone()
}

fn same_positions(a: &PageLayout, b: &PageLayout, what: &str) {
    assert_eq!(a.blocks().len(), b.blocks().len());
    for (x, y) in a.blocks().iter().zip(b.blocks()) {
        assert_eq!(x.y.to_bits(), y.y.to_bits(), "{what}: block {} y", x.block);
        assert_eq!(
            x.height.to_bits(),
            y.height.to_bits(),
            "{what}: block {} height",
            x.block
        );
    }
    assert_eq!(
        a.total_height().to_bits(),
        b.total_height().to_bits(),
        "{what}"
    );
}

/// The first row of a frame at `top` that differs from the one-pass page.
fn differs(frame: &[u8], rows: usize, top: usize, full: &mdview_render::Rendered) -> Option<usize> {
    let width = full.bitmap.width as usize * 4;
    let count = rows.min((full.bitmap.height as usize).saturating_sub(top));
    (0..count).find(|row| {
        frame[row * width..(row + 1) * width]
            != full.bitmap.pixels[(top + row) * width..(top + row + 1) * width]
    })
}

/// The last column of row `row` with dark text in it.
fn reach(r: &mdview_render::Rendered, row: usize) -> usize {
    let width = r.bitmap.width as usize;
    (0..width)
        .rev()
        .find(|x| {
            let p = &r.bitmap.pixels[(row * width + x) * 4..][..4];
            p[0] < 0x60 && p[1] < 0x60 && p[2] < 0x60
        })
        .unwrap_or(0)
}

/// The first column of row `row` with dark text in it.
fn start(r: &mdview_render::Rendered, row: usize) -> usize {
    let width = r.bitmap.width as usize;
    (0..width)
        .find(|x| {
            let p = &r.bitmap.pixels[(row * width + x) * 4..][..4];
            p[0] < 0x60 && p[1] < 0x60 && p[2] < 0x60
        })
        .unwrap_or(width)
}

fn rows_of(r: &mdview_render::Rendered, index: usize) -> std::ops::Range<usize> {
    let block = r.blocks[index];
    block.y as usize..(block.y + block.height) as usize
}

#[test]
fn the_blocks_after_a_floated_image_run_beside_it_and_then_full_width() {
    let options = options(780.0, 1.0);
    // The column is 40..740; the image is 200 wide and 280 tall.
    let right = render(&run_doc("right", "assets/tall.png"), &options);
    // Top-level blocks: 0 heading, 1 paragraph, 2 the image, 3..5 paragraphs, 6 heading, 7 list.
    assert_eq!(
        right.blocks[2].height, 0.0,
        "the image's paragraph has no line"
    );
    for index in [3, 4, 6, 7] {
        let widest = rows_of(&right, index).map(|row| reach(&right, row)).max();
        assert!(widest.unwrap() < 530, "block {index} beside: {widest:?}");
    }
    let last = right.blocks.len() - 1;
    let widest = rows_of(&right, last).map(|row| reach(&right, row)).max();
    assert!(widest.unwrap() > 640, "full width after it: {widest:?}");
    // The blocks beside it start level with the image, not below it.
    assert!(right.blocks[5].y < right.blocks[2].y + 280.0);

    let left = render(&run_doc("left", "assets/tall.png"), &options);
    for index in [3, 4, 6] {
        let first = rows_of(&left, index).map(|row| start(&left, row)).min();
        assert!(
            first.unwrap() >= 256,
            "block {index} after the image: {first:?}"
        );
    }
    // The list's first item is beside the image, marker and all; its second is below it.
    let top = left.blocks[7].y as usize;
    let first = (top..top + 24).map(|row| start(&left, row)).min();
    assert!(first.unwrap() >= 256, "the first item's marker: {first:?}");
    let first = rows_of(&left, last).map(|row| start(&left, row)).min();
    assert!(
        first.unwrap() < 50,
        "from the left edge after it: {first:?}"
    );
}

#[test]
fn a_block_that_cannot_narrow_starts_below_the_float() {
    let options = options(780.0, 1.0);
    let doc = mdview_core::parse(
        "<img src=\"assets/tall.png\" width=\"200\" align=\"right\">\n\nBeside.\n\n\
         | a | b |\n|---|---|\n| one | two |\n\nAfter.\n",
    );
    let page = render(&doc, &options);
    // 0 the image, 1 the paragraph, 2 the table.
    let table = page.blocks[2];
    let top = rows_of(&page, 2)
        .find(|&row| reach(&page, row) > 0 || start(&page, row) < 780)
        .unwrap_or(table.y as usize);
    assert!(
        top as f32 >= page.blocks[0].y + 280.0,
        "the table's first ink is below the image: {top}"
    );
    // A code block narrows instead: it ends short of the image.
    let doc = mdview_core::parse(
        "<img src=\"assets/tall.png\" width=\"200\" align=\"right\">\n\nBeside.\n\n\
         ```\ncode\n```\n\nAfter.\n",
    );
    let page = render(&doc, &options);
    assert!(
        page.blocks[2].y < page.blocks[0].y + 200.0,
        "the code is beside"
    );
    let width = page.bitmap.width as usize;
    let row = (page.blocks[2].y + 20.0) as usize;
    let background = &page.bitmap.pixels[(row * width + 20) * 4..][..4];
    let boxed = (0..width)
        .rev()
        .find(|x| &page.bitmap.pixels[(row * width + x) * 4..][..4] != background)
        .unwrap();
    assert!(boxed > 540, "the row crosses the image: {boxed}");
    let code_right = (0..540)
        .rev()
        .find(|x| &page.bitmap.pixels[(row * width + x) * 4..][..4] != background)
        .unwrap();
    assert!(code_right < 530 && code_right > 400, "{code_right}");
}

#[test]
fn a_float_stays_in_its_list_item_and_the_page_ends_below_the_last_one() {
    let options = options(780.0, 1.0);
    // 60 px wide, so 84 tall.
    let doc = mdview_core::parse(
        "- <img src=\"assets/tall.png\" width=\"60\" align=\"right\"> short\n- next\n\nAfter the list.\n",
    );
    let page = render(&doc, &options);
    assert!(
        page.blocks[0].height > 84.0 + 20.0,
        "{}",
        page.blocks[0].height
    );
    let widest = rows_of(&page, 1)
        .map(|row| reach(&page, row))
        .max()
        .unwrap();
    assert!(widest > 100, "the paragraph after the list has its text");
    // The last block of the page holds its float.
    let doc = mdview_core::parse(
        "Before.\n\n<img src=\"assets/tall.png\" width=\"200\" align=\"right\"> Short text.\n",
    );
    let page = render(&doc, &options);
    assert!(page.blocks[1].height >= 280.0, "{}", page.blocks[1].height);
    assert_eq!(
        eager(&doc, &options).total_height(),
        page.content_height,
        "the lazy layout ends at the same row"
    );
}

#[test]
fn a_second_float_stands_beside_the_first_or_below_it() {
    let options = options(780.0, 1.0);
    let blue = |r: &mdview_render::Rendered| {
        // Rows and columns holding the panel's border colour.
        let width = r.bitmap.width as usize;
        let mut rows = Vec::new();
        for (i, p) in r.bitmap.pixels.as_chunks::<4>().0.iter().enumerate() {
            if (p[2], p[1], p[0]) == (40, 90, 160) {
                rows.push((i / width, i % width));
            }
        }
        rows
    };
    let two = |second: &str| {
        mdview_core::parse(&format!(
            "<img src=\"assets/tall.png\" width=\"100\" align=\"right\">\n\nOne line.\n\n\
             <img src=\"assets/tall.png\" width=\"100\" align=\"{second}\">\n\n{}\n",
            WORDS.repeat(12)
        ))
    };
    // Same side: the second image is below the first (140 tall each).
    let same = render(&two("right"), &options);
    let border = blue(&same);
    let (top, bottom) = (border.first().unwrap().0, border.last().unwrap().0);
    assert!(bottom - top > 280, "one under the other: {top}..{bottom}");
    assert!(border.iter().all(|&(_, x)| x >= 640), "both at the right");
    // Other side: beside it, in the same rows.
    let other = render(&two("left"), &options);
    let border = blue(&other);
    let (top, bottom) = (border.first().unwrap().0, border.last().unwrap().0);
    assert!(bottom - top < 200, "side by side: {top}..{bottom}");
    assert!(border.iter().any(|&(_, x)| x < 140) && border.iter().any(|&(_, x)| x >= 640));
}

/// Every top-level block as the first thing laid out: the frame a jump lands on equals the rows
/// of the one-pass page, although nothing above was ever shown.
#[test]
fn a_jump_into_a_floated_run_shows_what_the_page_shows() {
    for (width, scale) in [(780.0, 1.0), (600.0, 1.5)] {
        let options = options(width, scale);
        for doc in [fixture(), run_doc("left", "assets/tall.png")] {
            let full = render(&doc, &options);
            let exact = eager(&doc, &options);
            let (margin, rows) = (24.0, 300.0);
            let mut checked = 0;
            for span in exact.blocks() {
                let mut renderer = DocumentRenderer::new();
                renderer.layout(&doc, &options);
                let top = renderer
                    .jump_to(&doc, &options, Jump::Block(span.block), margin, rows)
                    .unwrap();
                let page = renderer.page_layout().unwrap();
                let y = page.y_of(&doc, span.block).unwrap();
                if y - top != margin {
                    // Clamped at the start or the end of the page.
                    continue;
                }
                let frame = renderer.view_frame(&doc, &options, top, rows);
                let at = (span.y - margin) as usize;
                assert_eq!(
                    differs(&frame.bitmap.pixels, rows as usize, at, &full),
                    None,
                    "{width}@{scale}: jump to block {}",
                    span.block
                );
                checked += 1;
            }
            assert!(checked > 10, "{checked} jumps compared");
        }
    }
}

#[test]
fn refinement_and_scrolling_converge_to_the_eager_layout_and_pixels() {
    for (width, scale) in [(780.0, 1.0), (520.0, 1.5)] {
        let doc = fixture();
        let options = options(width, scale);
        let what = format!("{width}@{scale}");
        let full = render(&doc, &options);
        let exact = eager(&doc, &options);
        assert_eq!(exact.total_height(), full.content_height, "{what}");
        // Refinement alone: the blocks of floated runs are measured with their images.
        let mut renderer = DocumentRenderer::new();
        renderer.layout(&doc, &options);
        refine_all(&mut renderer, &doc, &options);
        same_positions(renderer.page_layout().unwrap(), &exact, &what);
        // From the bottom up, in bands: frames start inside floated runs, with the image hanging
        // in from a block above the frame.
        let rows = 260.0;
        let mut renderer = DocumentRenderer::new();
        renderer.layout(&doc, &options);
        let mut top = f32::MAX;
        loop {
            let frame = renderer.view_frame(&doc, &options, top.min(1e7), rows);
            top = frame.top;
            if top <= 0.0 {
                break;
            }
            top = (top - 60.0).max(0.0);
        }
        refine_all(&mut renderer, &doc, &options);
        same_positions(renderer.page_layout().unwrap(), &exact, &what);
        let mut top = renderer.page_layout().unwrap().total_height();
        loop {
            let frame = renderer.view_frame(&doc, &options, top, rows);
            let at = frame.top as usize;
            assert_eq!(
                differs(&frame.bitmap.pixels, rows as usize, at, &full),
                None,
                "{what}: frame at {at}"
            );
            if at == 0 {
                break;
            }
            top = (frame.top - 60.0).max(0.0);
        }
        // And down again in small steps.
        let mut top = 0.0;
        let mut last = usize::MAX;
        loop {
            let frame = renderer.view_frame(&doc, &options, top, rows);
            let at = frame.top as usize;
            if at == last {
                break;
            }
            last = at;
            assert_eq!(
                differs(&frame.bitmap.pixels, rows as usize, at, &full),
                None,
                "{what}: frame at {at} going down"
            );
            top = frame.top + 40.0;
        }
    }
}

#[test]
fn refinement_keeps_the_view_still_below_a_floated_run() {
    let doc = run_doc("right", "assets/tall.png");
    let options = options(780.0, 1.0);
    let exact = eager(&doc, &options);
    let target = exact.blocks()[20].block;
    let mut renderer = DocumentRenderer::new();
    renderer.layout(&doc, &options);
    let top = renderer
        .jump_to(&doc, &options, Jump::Block(target), 10.0, 500.0)
        .unwrap();
    let shown = renderer
        .view_frame(&doc, &options, top, 500.0)
        .bitmap
        .pixels
        .clone();
    refine_all(&mut renderer, &doc, &options);
    let after = renderer.top().unwrap();
    let frame = renderer.view_frame(&doc, &options, after, 500.0);
    assert_eq!(frame.bitmap.pixels, shown, "same text, same rows");
    assert_eq!(after, exact.y_of(&doc, target).unwrap() - 10.0);
}

fn red(width: u32, height: u32) -> DecodedImage {
    DecodedImage {
        width,
        height,
        baseline: height,
        rgba: (0..width * height)
            .flat_map(|_| [220, 30, 30, 255])
            .collect(),
    }
}

/// A remote image with no declared size is alt text until it arrives; then it floats, and the
/// blocks after its paragraph are laid out again beside it, wherever the viewport is.
#[test]
fn an_image_that_arrives_late_reflows_the_blocks_beside_it() {
    let url = "https://example.com/float.png";
    let doc = run_doc("right", url);
    let options = Options {
        base_dir: None,
        ..options(780.0, 1.0)
    };
    let fresh = |arrived: bool| {
        let mut renderer = DocumentRenderer::new();
        if arrived {
            renderer.image_arrived(url, Some(red(200, 300)));
        }
        renderer.layout_exact(&doc, &options);
        renderer
    };
    let before = fresh(false);
    let mut expected = fresh(true);
    let blocks = before.page_layout().unwrap().blocks().to_vec();
    // Looking at the top, at the middle of the run (the image's block is above the frame) and
    // far below it.
    for (index, lazy) in [(0, false), (5, false), (5, true), (30, true)] {
        let mut renderer = DocumentRenderer::new();
        if lazy {
            renderer.layout(&doc, &options);
        } else {
            renderer.layout_exact(&doc, &options);
        }
        let top = renderer
            .jump_to(&doc, &options, Jump::Block(blocks[index].block), 0.0, 300.0)
            .unwrap();
        renderer.view_frame(&doc, &options, top, 300.0);
        assert!(renderer.image_arrived(url, Some(red(200, 300))));
        let top = renderer.top().unwrap();
        let frame = renderer.view_frame(&doc, &options, top, 300.0);
        let (top, pixels) = (frame.top, frame.bitmap.pixels.clone());
        if lazy {
            refine_all(&mut renderer, &doc, &options);
        }
        let what = format!("looking at block {index}, lazy {lazy}");
        same_positions(
            renderer.page_layout().unwrap(),
            expected.page_layout().unwrap(),
            &what,
        );
        let top = if lazy { renderer.top().unwrap() } else { top };
        let frame = renderer.view_frame(&doc, &options, top, 300.0);
        if !lazy {
            assert_eq!(frame.bitmap.pixels, pixels, "{what}: drawn once");
        }
        let top = frame.top;
        let view = expected.view(&doc, &options, top, 300.0);
        assert_eq!(
            renderer.frame().unwrap().bitmap.pixels,
            view.bitmap.pixels,
            "{what}: the frame matches a fresh view"
        );
    }
    // The run is narrower with the image than without it.
    let (a, b) = (
        before.page_layout().unwrap().blocks()[3].height,
        expected.page_layout().unwrap().blocks()[3].height,
    );
    assert!(b > a, "{a} {b}");
}

#[test]
fn a_details_toggled_beside_a_float_moves_the_blocks_after_it() {
    let doc = mdview_core::parse(&format!(
        "<img src=\"assets/tall.png\" width=\"200\" align=\"right\">\n\nFirst.\n\n\
         <details>\n<summary>More</summary>\n\n{0}\n\n{0}\n\n</details>\n\n{0}\n\n{0}\n\n{0}\n\nEnd.\n",
        WORDS.repeat(3)
    ));
    let details = (0..doc.blocks.len() as u32)
        .find(|&b| {
            matches!(
                doc.blocks[b as usize].kind,
                mdview_core::model::BlockKind::Details { .. }
            )
        })
        .unwrap();
    let options = options(780.0, 1.0);
    let mut renderer = DocumentRenderer::new();
    renderer.layout_exact(&doc, &options);
    renderer.view_frame(&doc, &options, 0.0, 700.0);
    for open in [true, false, true] {
        renderer.set_details(&doc, details, open);
        renderer.view_frame(&doc, &options, 0.0, 700.0);
        let mut expected = DocumentRenderer::new();
        expected.set_details(&doc, details, open);
        expected.layout_exact(&doc, &options);
        same_positions(
            renderer.page_layout().unwrap(),
            expected.page_layout().unwrap(),
            &format!("open {open}"),
        );
        let view = expected.view(&doc, &options, 0.0, 700.0);
        assert_eq!(
            renderer.frame().unwrap().bitmap.pixels,
            view.bitmap.pixels,
            "open {open}"
        );
    }
}

#[test]
fn a_reload_without_the_float_lays_out_like_a_fresh_page() {
    let options = options(780.0, 1.0);
    let with = run_doc("right", "assets/tall.png");
    let without = mdview_core::parse(&{
        let mut md = String::from("# Top\n\nBefore the float.\n\n");
        for i in 0..40 {
            md.push_str(&format!("Paragraph {i}. {}\n\n", WORDS.repeat(2)));
        }
        md
    });
    let mut renderer = DocumentRenderer::new();
    renderer.layout(&with, &options);
    renderer.view_frame(&with, &options, 150.0, 500.0);
    refine_all(&mut renderer, &with, &options);
    for doc in [&without, &with, &without] {
        renderer.layout(doc, &options);
        let frame = renderer.view_frame(doc, &options, 150.0, 500.0);
        let (top, pixels) = (frame.top, frame.bitmap.pixels.clone());
        let full = render(doc, &options);
        assert_eq!(differs(&pixels, 500, top as usize, &full), None);
        refine_all(&mut renderer, doc, &options);
        same_positions(
            renderer.page_layout().unwrap(),
            &eager(doc, &options),
            "after a reload",
        );
    }
}

#[test]
fn hits_and_selections_follow_the_narrowed_lines() {
    let options = options(780.0, 1.0);
    let doc = mdview_core::parse(&format!(
        "<a href=\"https://example.com/image\"><img src=\"assets/tall.png\" width=\"200\" align=\"left\"></a>\n\n\
         First paragraph. {WORDS}\n\n\
         Second with [a link](https://example.com/x) in it. {WORDS}\n\nEnd.\n"
    ));
    for lazy in [false, true] {
        let mut renderer = DocumentRenderer::new();
        if lazy {
            renderer.layout(&doc, &options);
        } else {
            renderer.layout_exact(&doc, &options);
        }
        // The image is at 40..240 and the text starts 16 px after it.
        let at = doc.text.find("a link").unwrap() as u32;
        let rects = renderer.selection_rects(&doc, &options, at, at + 6);
        assert_eq!(rects.len(), 1);
        let rect = rects[0];
        assert!(rect.x >= 256.0 && rect.x + rect.w < 740.0, "{rect:?}");
        let hit = renderer
            .hit_test(&doc, &options, rect.x + rect.w / 2.0, rect.y + rect.h / 2.0)
            .unwrap();
        assert!(hit.on_text && (at..at + 6).contains(&hit.offset), "{hit:?}");
        let link = hit.link.expect("the link under the point");
        assert_eq!(
            doc.str(doc.links[link as usize].dest),
            "https://example.com/x"
        );
        // A whole-paragraph selection stays right of the image.
        let from = doc.text.find("Second").unwrap() as u32;
        let rects = renderer.selection_rects(&doc, &options, from, from + 80);
        assert!(rects.len() >= 2);
        assert!(rects.iter().all(|r| r.x >= 256.0), "{rects:?}");
        // A point on the image, in the rows of the second paragraph, is the image: its block,
        // not text, and the link it sits in.
        let hit = renderer
            .hit_test(&doc, &options, 140.0, rect.y + rect.h / 2.0)
            .unwrap();
        assert!(!hit.on_text, "{hit:?}");
        assert_eq!(hit.block, 0, "{hit:?}");
        let link = hit.link.expect("the image's link");
        assert_eq!(
            doc.str(doc.links[link as usize].dest),
            "https://example.com/image"
        );
        // Beside the image nothing changed for the text of its neighbours.
        let page = renderer.page_layout().unwrap().clone();
        same_positions(&page, &eager(&doc, &options), "after hits");
    }
}

/// Floats that open while others are still open, floats inside containers and a `<details>`,
/// measured in a scrambled order (jumps, hit tests, selections and refinement slices mixed):
/// the layout ends where the eager one does, and every frame shows the page's rows.
#[test]
fn any_order_of_jumps_hits_and_refinement_gives_the_same_page() {
    let mut md = String::from(
        "# Many floats

",
    );
    for i in 0..60 {
        let side = if i % 3 == 0 { "left" } else { "right" };
        let width = 60 + (i * 37) % 150;
        match i % 6 {
            0 | 3 => md.push_str(&format!(
                "<img src=\"assets/tall.png\" width=\"{width}\" align=\"{side}\">

"
            )),
            1 => md.push_str(&format!(
                "<img src=\"assets/badge.png\" width=\"{width}\" align=\"{side}\"> {}

",
                WORDS.repeat(i % 4 + 1)
            )),
            2 => md.push_str(&format!(
                "- item {i}
- <img src=\"assets/tall.png\" width=\"50\" align=\"{side}\"> floated in an item
- {WORDS}

"
            )),
            4 => md.push_str(
                "```
code beside or below
```

> A quote. Words in it.

",
            ),
            _ => md.push_str(&format!(
                "### Heading {i}

| a | b |
|---|---|
| {i} | x |

"
            )),
        }
        for n in 0..(i * 7) % 4 {
            md.push_str(&format!(
                "Paragraph {i}.{n}. {}

",
                WORDS.repeat((i + n) % 3 + 1)
            ));
        }
        if i % 10 == 9 {
            md.push_str(
                "<details open>
<summary>More</summary>

Inside the details.

</details>

---

",
            );
        }
    }
    let doc = mdview_core::parse(&md);
    for (width, scale) in [(780.0, 1.0), (560.0, 1.25)] {
        let options = options(width, scale);
        let full = render(&doc, &options);
        let exact = eager(&doc, &options);
        assert_eq!(exact.total_height(), full.content_height);
        let count = exact.blocks().len();
        for seed in 1..=4u32 {
            let mut state = seed.wrapping_mul(2_654_435_761);
            let mut next = |n: usize| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (state >> 8) as usize % n
            };
            let mut renderer = DocumentRenderer::new();
            renderer.layout(&doc, &options);
            let rows = 300.0;
            for _ in 0..40 {
                let block = exact.blocks()[next(count)].block;
                match next(4) {
                    0 => {
                        renderer.refine(&doc, &options, Duration::from_micros(300));
                    }
                    1 => {
                        let y = renderer.page_layout().unwrap().y_of(&doc, block).unwrap();
                        renderer.hit_test(&doc, &options, 60.0 + next(600) as f32, y + 5.0);
                    }
                    2 => {
                        let text = doc.blocks[block as usize].text;
                        renderer.selection_rects(&doc, &options, text.start, text.end);
                    }
                    _ => {
                        let top = renderer
                            .jump_to(&doc, &options, Jump::Block(block), 12.0, rows)
                            .unwrap();
                        let page = renderer.page_layout().unwrap();
                        let landed = page.y_of(&doc, block).unwrap() - top == 12.0;
                        let frame = renderer.view_frame(&doc, &options, top, rows);
                        if landed {
                            let at = (exact.y_of(&doc, block).unwrap() - 12.0) as usize;
                            assert_eq!(
                                differs(&frame.bitmap.pixels, rows as usize, at, &full),
                                None,
                                "{width}@{scale} seed {seed}: jump to block {block}"
                            );
                        }
                    }
                }
            }
            refine_all(&mut renderer, &doc, &options);
            let what = format!("{width}@{scale} seed {seed}");
            same_positions(renderer.page_layout().unwrap(), &exact, &what);
            let mut top = 0.0;
            let mut last = usize::MAX;
            loop {
                let frame = renderer.view_frame(&doc, &options, top, rows);
                let at = frame.top as usize;
                if at == last {
                    break;
                }
                last = at;
                assert_eq!(
                    differs(&frame.bitmap.pixels, rows as usize, at, &full),
                    None,
                    "{what}: frame at {at}"
                );
                top = frame.top + 170.0;
            }
        }
    }
}

/// A remote image with a declared size keeps its box while it loads, so its arrival moves
/// nothing; the frame still has to show it when the image's paragraph is above the frame and
/// only the image hangs in.
#[test]
fn an_arrived_float_is_drawn_in_a_frame_below_its_paragraph() {
    let url = "https://example.com/sized.png";
    let doc = mdview_core::parse(&format!(
        "Before.

<img src=\"{url}\" width=\"200\" height=\"300\" align=\"right\">

{}",
        format!(
            "{}

",
            WORDS.repeat(2)
        )
        .repeat(12)
    ));
    let options = Options {
        base_dir: None,
        ..options(780.0, 1.0)
    };
    let mut renderer = DocumentRenderer::new();
    renderer.layout_exact(&doc, &options);
    let before = renderer.page_layout().unwrap().clone();
    let top = before.blocks()[3].y;
    let waiting = renderer
        .view_frame(&doc, &options, top, 200.0)
        .bitmap
        .pixels
        .clone();
    assert!(renderer.image_arrived(url, Some(red(200, 300))));
    let frame = renderer.view_frame(&doc, &options, top, 200.0);
    assert_ne!(frame.bitmap.pixels, waiting, "the image replaced its box");
    let pixels = frame.bitmap.pixels.clone();
    same_positions(renderer.page_layout().unwrap(), &before, "same box");
    let mut expected = DocumentRenderer::new();
    expected.image_arrived(url, Some(red(200, 300)));
    expected.layout_exact(&doc, &options);
    assert_eq!(
        pixels,
        expected.view(&doc, &options, top, 200.0).bitmap.pixels
    );
}
