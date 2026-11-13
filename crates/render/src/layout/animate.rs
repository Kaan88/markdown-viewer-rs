//! Animated images in the kept frame: [`DocumentRenderer::animate`] puts up the frames that are
//! due among the images the frame shows and says which rows changed and when to come back.
//!
//! The renderer has no clock and no timer of its own. A shell calls `animate` with the time, waits
//! until the instant it returns (or for the wake it registered, when a frame is still being
//! decoded), and calls again; the next [`view_frame`](DocumentRenderer::view_frame) repaints only
//! the changed rows. An image that is not in the kept frame is never advanced, so a page without
//! a visible animation asks for no wake at all.

use super::DocumentRenderer;
use super::paint::Paint;
use std::sync::Arc;
use std::time::Instant;

/// What [`DocumentRenderer::animate`] did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AnimationTick {
    /// When the next frame already decoded is due. `None` when nothing on screen animates or
    /// the next frame is still being decoded (the wake says when it is ready).
    pub next: Option<Instant>,
    /// Rows of the kept frame whose pixels changed, first and one past the last. The next
    /// `view_frame` paints them (as its `band`, when nothing else changed).
    pub rows: Option<(u32, u32)>,
}

impl DocumentRenderer {
    /// Lets animated GIF and WebP images play (off by default, so tests, goldens and `--shot`
    /// renders show first frames). Turning it off puts local images back on their first frame.
    pub fn set_animation(&mut self, on: bool) {
        if self.store.set_animation(on) {
            self.view.stale = true;
        }
    }

    /// Sets what the decoding thread calls when a frame is ready. It must only post a message
    /// that makes the shell call [`animate`](Self::animate). Set it before the first layout.
    pub fn set_animation_wake(&mut self, wake: impl Fn() + Send + Sync + 'static) {
        self.store.set_waker(Arc::new(wake));
    }

    /// Advances the animated images the kept frame shows to the frames due at `now`. Cheap, and
    /// nothing at all without an animated image in the store.
    pub fn animate(&mut self, now: Instant) -> AnimationTick {
        if !self.store.animating() {
            return AnimationTick::default();
        }
        self.settle();
        let shown = self.animated_shown();
        if shown.is_empty() && self.frame.is_none() {
            return AnimationTick::default();
        }
        let ids: Vec<u32> = shown.iter().map(|s| s.0).collect();
        let (changed, next) = self.store.advance(now, &ids);
        let mut tick = AnimationTick { next, rows: None };
        let Some(frame) = self.frame.as_ref() else {
            return tick;
        };
        let (top, height) = (frame.top, frame.bitmap.height as f32);
        let mut range: Option<(f32, f32)> = None;
        for &(id, y0, y1) in &shown {
            if changed.contains(&id) {
                range = Some(range.map_or((y0, y1), |(a, b)| (a.min(y0), b.max(y1))));
            }
        }
        let Some((y0, y1)) = range else {
            return tick;
        };
        let merged = match self.view.repaint {
            Some((a, b)) => (a.min(y0), b.max(y1)),
            None => (y0, y1),
        };
        self.view.repaint = Some(merged);
        let first = (merged.0 - top).clamp(0.0, height);
        let end = (merged.1 - top).clamp(0.0, height);
        tick.rows = Some((first as u32, end as u32));
        tick
    }

    /// The animated images the kept frame shows: store id and the page rows each covers, a row
    /// to spare on either side.
    fn animated_shown(&self) -> Vec<(u32, f32, f32)> {
        let mut shown = Vec::new();
        let (Some(frame), Some(page)) = (self.frame.as_ref(), self.page.as_ref()) else {
            return shown;
        };
        if self.view.stale {
            // The frame no longer shows the layout; the next one is drawn whole and looked at
            // then.
            return shown;
        }
        let (top, bottom) = (frame.top, frame.top + frame.bitmap.height as f32);
        let mut index = page.first_below(top);
        while index < page.len() && page.spans[index].y < bottom {
            let span = page.spans[index];
            if let Some(cached) = self.cache.get(&span.block) {
                let origin = span.y - cached.phase;
                for paint in &cached.paints {
                    if let Paint::Image {
                        index: id, y, h, ..
                    } = paint
                        && self.store.animated(*id)
                    {
                        let y0 = (y + origin).round() - 1.0;
                        let y1 = y0 + h.ceil() + 3.0;
                        if y1 > top && y0 < bottom {
                            shown.push((*id, y0.floor(), y1.ceil()));
                        }
                    }
                }
            }
            index += 1;
        }
        shown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anim::testgif::{encode, solid};
    use crate::{LIGHT, Options};
    use mdview_core::Document;
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    const VIEW: f32 = 300.0;

    fn options(dir: &Path) -> Options {
        Options {
            width: 400.0,
            scale: 1.0,
            theme: LIGHT,
            max_height: None,
            base_dir: Some(dir.to_path_buf()),
        }
    }

    fn fixtures() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/assets")
    }

    /// A paragraph, the images, then enough text to scroll them off the top.
    fn doc(images: &[&str]) -> Document {
        let mut md = String::from("Above the pictures.\n\n");
        for image in images {
            md.push_str(&format!("![animated]({image})\n\n"));
        }
        for i in 0..60 {
            md.push_str(&format!(
                "Paragraph {i} of the text below the pictures.\n\n"
            ));
        }
        mdview_core::parse(&md)
    }

    fn playing(dir: &Path, doc: &Document) -> DocumentRenderer {
        let mut renderer = DocumentRenderer::new();
        renderer.set_animation(true);
        renderer.layout(doc, &options(dir));
        renderer.view_frame(doc, &options(dir), 0.0, VIEW);
        renderer
    }

    /// Calls `animate` as a shell would, jumping the clock to each time it asks for and waiting
    /// on the decoding thread in between, until a frame goes up. `None` if none does.
    fn next_frame(renderer: &mut DocumentRenderer, now: &mut Instant) -> Option<AnimationTick> {
        let give_up = Instant::now() + Duration::from_secs(10);
        while Instant::now() < give_up {
            let tick = renderer.animate(*now);
            if tick.rows.is_some() {
                return Some(tick);
            }
            match tick.next {
                Some(due) => *now = due,
                None if renderer.image_stats().playing == 0 => return None,
                None => std::thread::sleep(Duration::from_millis(2)),
            }
        }
        None
    }

    #[test]
    fn nothing_animates_until_a_shell_asks() {
        let dir = fixtures();
        let doc = doc(&["anim.gif", "anim.webp"]);
        let mut renderer = DocumentRenderer::new();
        renderer.layout(&doc, &options(&dir));
        renderer.view_frame(&doc, &options(&dir), 0.0, VIEW);
        assert_eq!(renderer.image_stats().playing, 0);
        assert_eq!(
            renderer.animate(Instant::now() + Duration::from_secs(60)),
            AnimationTick::default()
        );
        assert_eq!(renderer.image_stats().frames, 0);
    }

    #[test]
    fn a_frame_repaints_only_the_rows_of_its_image() {
        let dir = fixtures();
        let doc = doc(&["anim.gif"]);
        let mut renderer = playing(&dir, &doc);
        assert_eq!(renderer.image_stats().playing, 1);
        let before = renderer.frame().unwrap().bitmap.pixels.clone();
        let width = renderer.frame().unwrap().bitmap.width as usize;

        let mut now = Instant::now();
        let tick = next_frame(&mut renderer, &mut now).expect("the second frame goes up");
        let (first, end) = tick.rows.unwrap();
        // A 16 px image and the spare rows around it, not the viewport.
        assert!(end > first && end - first <= 24, "{first}..{end}");
        assert_eq!(renderer.image_stats().frames, 1);

        let frame = renderer.view_frame(&doc, &options(&dir), 0.0, VIEW);
        assert_eq!(frame.band, Some((first, end - first)));
        assert_eq!(frame.shift, 0);
        let (a, b) = (first as usize * width * 4, end as usize * width * 4);
        assert_eq!(frame.bitmap.pixels[..a], before[..a], "rows above are kept");
        assert_eq!(frame.bitmap.pixels[b..], before[b..], "rows below are kept");
        assert_ne!(frame.bitmap.pixels[a..b], before[a..b], "the image changed");

        // The band is exactly what a whole repaint of the same state draws.
        let partial = frame.bitmap.pixels.clone();
        renderer.view.stale = true;
        let whole = renderer.view_frame(&doc, &options(&dir), 0.0, VIEW);
        assert_eq!(whole.band, None);
        assert_eq!(whole.bitmap.pixels, partial);

        // Nothing changed since: nothing to send.
        let frame = renderer.view_frame(&doc, &options(&dir), 0.0, VIEW);
        assert_eq!(frame.band, Some((0, 0)));
    }

    #[test]
    fn frames_follow_the_file_delays() {
        let dir = std::env::temp_dir().join("mdv-anim-delays");
        std::fs::create_dir_all(&dir).unwrap();
        let frames = [
            solid((0, 0, 8, 8), 1, 30),
            solid((0, 0, 8, 8), 2, 7),
            solid((0, 0, 8, 8), 3, 0),
        ];
        std::fs::write(
            dir.join("three.gif"),
            encode(8, 8, Some(gif::Repeat::Infinite), &frames),
        )
        .unwrap();
        let doc = doc(&["three.gif"]);
        let mut renderer = playing(&dir, &doc);
        // The clock of the frame on screen starts when the image is first seen playing.
        let start = Instant::now();
        let mut now = start;
        let mut at = Vec::new();
        for _ in 0..6 {
            next_frame(&mut renderer, &mut now).expect("plays");
            at.push(now.duration_since(start).as_millis());
        }
        // 300 ms, 70 ms, then a zero delay played as 100 ms, round and round.
        assert_eq!(at, [300, 370, 470, 770, 840, 940]);
    }

    #[test]
    fn an_image_off_screen_stands_still_and_asks_for_no_wake() {
        let dir = fixtures();
        let doc = doc(&["anim.gif", "anim.webp"]);
        let mut renderer = playing(&dir, &doc);
        let mut now = Instant::now();
        next_frame(&mut renderer, &mut now).expect("plays while shown");
        let shown = renderer.image_stats().frames;

        renderer.view_frame(&doc, &options(&dir), 900.0, VIEW);
        let give_up = Instant::now() + Duration::from_millis(300);
        while Instant::now() < give_up {
            now += Duration::from_secs(5);
            assert_eq!(renderer.animate(now), AnimationTick::default());
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(renderer.image_stats().frames, shown);

        // Back in view, it goes on.
        renderer.view_frame(&doc, &options(&dir), 0.0, VIEW);
        next_frame(&mut renderer, &mut now).expect("plays again");
        assert!(renderer.image_stats().frames > shown);
    }

    #[test]
    fn a_finite_loop_ends_and_then_costs_nothing() {
        let dir = std::env::temp_dir().join("mdv-anim-once");
        std::fs::create_dir_all(&dir).unwrap();
        let frames = [
            solid((0, 0, 8, 8), 1, 5),
            solid((0, 0, 8, 8), 2, 5),
            solid((0, 0, 8, 8), 3, 5),
        ];
        std::fs::write(dir.join("once.gif"), encode(8, 8, None, &frames)).unwrap();
        let doc = doc(&["once.gif"]);
        let mut renderer = playing(&dir, &doc);
        let mut now = Instant::now();
        let mut frames = 0;
        while next_frame(&mut renderer, &mut now).is_some() {
            frames += 1;
            assert!(frames < 10, "plays once");
        }
        assert_eq!(frames, 2);
        assert_eq!(renderer.image_stats().playing, 0);
        assert!(!renderer.store.animating());
        now += Duration::from_secs(60);
        assert_eq!(renderer.animate(now), AnimationTick::default());
        // The last frame (blue) stays up.
        let frame = renderer.view_frame(&doc, &options(&dir), 0.0, VIEW);
        let blue = frame
            .bitmap
            .pixels
            .chunks_exact(4)
            .filter(|p| p[..3] == [255, 0, 0])
            .count();
        assert_eq!(blue, 64);
    }

    #[test]
    fn turning_it_off_goes_back_to_the_first_frame() {
        let dir = fixtures();
        let doc = doc(&["anim.gif"]);
        let mut renderer = playing(&dir, &doc);
        let first = renderer.frame().unwrap().bitmap.pixels.clone();
        let mut now = Instant::now();
        next_frame(&mut renderer, &mut now).expect("plays");
        let moved = renderer
            .view_frame(&doc, &options(&dir), 0.0, VIEW)
            .bitmap
            .pixels
            .clone();
        assert_ne!(moved, first);

        renderer.set_animation(false);
        assert_eq!(renderer.image_stats().playing, 0);
        assert_eq!(renderer.animate(now), AnimationTick::default());
        let frame = renderer.view_frame(&doc, &options(&dir), 0.0, VIEW);
        assert_eq!(frame.bitmap.pixels, first);

        // And on again: what is loaded plays without a reload.
        renderer.set_animation(true);
        renderer.view_frame(&doc, &options(&dir), 0.0, VIEW);
        next_frame(&mut renderer, &mut now).expect("plays again");
    }

    #[test]
    fn a_loop_shown_small_is_kept_and_plays_without_a_decoder() {
        let dir = std::env::temp_dir().join("mdv-anim-kept");
        std::fs::create_dir_all(&dir).unwrap();
        // Three times the width of the column, so it is drawn below half size.
        let frames = [
            solid((0, 0, 1200, 60), 1, 20),
            solid((0, 0, 1200, 60), 2, 7),
            solid((300, 10, 600, 40), 3, 11),
        ];
        std::fs::write(
            dir.join("wide.gif"),
            encode(1200, 60, Some(gif::Repeat::Infinite), &frames),
        )
        .unwrap();
        let doc = doc(&["wide.gif"]);
        let still = {
            let mut renderer = DocumentRenderer::new();
            renderer.layout(&doc, &options(&dir));
            let frame = renderer.view_frame(&doc, &options(&dir), 0.0, VIEW);
            frame.bitmap.pixels.clone()
        };
        let mut renderer = playing(&dir, &doc);
        assert_eq!(renderer.store.decoders(), 1);
        let start = Instant::now();
        let mut now = start;
        let mut at = Vec::new();
        let mut first_again = Vec::new();
        for frame in 1..=12 {
            next_frame(&mut renderer, &mut now).expect("plays");
            at.push(now.duration_since(start).as_millis());
            let pixels = &renderer
                .view_frame(&doc, &options(&dir), 0.0, VIEW)
                .bitmap
                .pixels;
            if frame % 3 == 0 {
                first_again.push(*pixels == still);
            } else {
                assert_ne!(*pixels, still, "frame {frame}");
            }
        }
        // The delays hold through the hand-over from the decoder to the kept frames.
        assert_eq!(
            at,
            [
                200, 270, 380, 580, 650, 760, 960, 1030, 1140, 1340, 1410, 1520
            ]
        );
        // The first frame, scaled by the worker or kept, is the still image to the byte.
        assert_eq!(first_again, [true; 4]);
        // After the second loop every frame is in hand at the size shown: the decoder is gone
        // and the animation goes on.
        assert_eq!(renderer.store.decoders(), 0);
        assert_eq!(renderer.image_stats().playing, 1);

        // At another size the kept frames are of no use: a decoder starts again.
        let wider = Options {
            width: 520.0,
            ..options(&dir)
        };
        renderer.layout(&doc, &wider);
        renderer.view_frame(&doc, &wider, 0.0, VIEW);
        renderer.animate(now);
        assert_eq!(renderer.store.decoders(), 1);
        let give_up = Instant::now() + Duration::from_secs(10);
        let mut shown = 0;
        while shown < 3 && Instant::now() < give_up {
            let tick = renderer.animate(now);
            if tick.rows.is_some() {
                shown += 1;
                renderer.view_frame(&doc, &wider, 0.0, VIEW);
            }
            match tick.next {
                Some(due) => now = due,
                None => std::thread::sleep(Duration::from_millis(2)),
            }
        }
        assert_eq!(shown, 3, "plays at the new size");
    }

    #[test]
    fn the_budget_decides_how_many_play() {
        let dir = std::env::temp_dir().join("mdv-anim-budget");
        std::fs::create_dir_all(&dir).unwrap();
        let bytes = std::fs::read(fixtures().join("anim.gif")).unwrap();
        for name in ["one.gif", "two.gif"] {
            std::fs::write(dir.join(name), &bytes).unwrap();
        }
        let cost = bytes.len() as u64 + 16 * 16 * 12;
        let doc = doc(&["one.gif", "two.gif"]);
        let mut renderer = DocumentRenderer::new();
        renderer.set_animation(true);
        // Room for one player and a half.
        renderer.store.set_animation_budget(cost + cost / 2);
        renderer.layout(&doc, &options(&dir));
        renderer.view_frame(&doc, &options(&dir), 0.0, VIEW);
        assert_eq!(renderer.image_stats().playing, 1);
        let mut now = Instant::now();
        for _ in 0..4 {
            next_frame(&mut renderer, &mut now).expect("one plays");
            // Both are on screen, so neither gives way to the other.
            assert_eq!(renderer.image_stats().playing, 1);
        }
        // With room, the one left out starts when it is next looked at.
        renderer.store.set_animation_budget(cost * 2);
        renderer.animate(now);
        assert_eq!(renderer.image_stats().playing, 2);

        // Off screen, a player gives way to one that is shown.
        renderer.store.set_animation_budget(cost + cost / 2);
        std::fs::write(dir.join("three.gif"), &bytes).unwrap();
        let other = super::tests::doc(&["three.gif"]);
        renderer.layout(&other, &options(&dir));
        renderer.view_frame(&other, &options(&dir), 0.0, VIEW);
        renderer.animate(now);
        assert_eq!(renderer.image_stats().playing, 1);
        next_frame(&mut renderer, &mut now).expect("the shown one plays");
    }
}
