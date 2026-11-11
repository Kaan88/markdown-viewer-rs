//! Images and engine-rendered blocks: file image loading and sizing, Mermaid diagram and display
//! math blocks, and the labelled code box (syntax-highlighted) they fall back to.

use super::hit::SourceBox;
use super::inline::{Inline, Style};
use super::pager::{Pager, Txt};
use super::paint::Paint;
use super::store::Found;
use super::{Source, SourceKind};
use crate::highlight;
use mdview_core::model::BlockKind;
use std::path::PathBuf;

/// The box, in logical px, that stands in for a local image a refinement pass may not load, and
/// the heights for a formula and a diagram block. Such a block is measured again with everything
/// loaded before it is drawn, so these only steer the page height until then; they are typical
/// sizes, not anything the document said.
pub(super) const GUESS_IMAGE: (f32, f32) = (400.0, 240.0);
pub(super) const GUESS_MATH: f32 = 44.0;
pub(super) const GUESS_DIAGRAM: f32 = 300.0;

fn is_remote(src: &str) -> bool {
    ["http://", "https://"].iter().any(|scheme| {
        src.get(..scheme.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(scheme))
    })
}

impl<'a, 'b> Pager<'a, 'b> {
    /// Finds document image `index` in the store, loading it when this pass may. The id is
    /// `u32::MAX` when there is nothing to draw (an empty src, or a file this pass did not load).
    /// `Failed` covers a missing file, a blocked format or anything over the plan's limits.
    pub(super) fn image(&mut self, index: u32) -> (u32, Found) {
        let src = self.image_src(index);
        if src.is_empty() {
            return (u32::MAX, Found::Failed);
        }
        let trimmed = src.trim();
        let remote = is_remote(trimmed);
        let (id, found) = if remote {
            self.store.remote(trimmed, self.allow)
        } else {
            let path = PathBuf::from(src);
            let path = match &self.base_dir {
                Some(dir) if path.is_relative() => dir.join(path),
                _ => path,
            };
            self.store.file(&path, self.scale, self.allow)
        };
        self.note(id, &found, !remote);
        (id, found)
    }

    /// The source document image `index` shows in this theme: `<picture>` may name one per
    /// colour scheme, and the matching one wins when present.
    fn image_src(&self, index: u32) -> &'a str {
        let doc: &'a mdview_core::Document = self.doc;
        let info = doc.images[index as usize];
        let themed = if self.theme.name == "dark" {
            doc.str(info.dark_src)
        } else {
            doc.str(info.light_src)
        };
        if themed.is_empty() {
            doc.str(info.src)
        } else {
            themed
        }
    }

    /// Whether image `index` is a local file this pass did not load: what a refinement pass lays
    /// out with a guessed box (see [`GUESS_IMAGE`]).
    pub(super) fn unloaded_local(&self, index: u32, found: &Found) -> bool {
        matches!(found, Found::Unknown) && !is_remote(self.image_src(index).trim())
    }

    /// Records that the block being laid out uses image `id`, and whether it stands in for a
    /// local image, formula or diagram this pass was not allowed to load. (An undecided remote
    /// image is a final state for layout: its arrival re-measures the blocks that use it.)
    pub(super) fn note(&mut self, id: u32, found: &Found, local: bool) {
        if id != u32::MAX && !self.used.contains(&id) {
            self.used.push(id);
        }
        if local && matches!(found, Found::Unknown) {
            self.provisional = true;
        }
    }

    /// The drawn size of an image whose HTML gave `width` and/or `height`. One of them keeps the
    /// aspect ratio; both stretch to that box. Neither is allowed to pass the content width.
    pub(super) fn sized_image(
        &self,
        natural: (f32, f32),
        width: Option<mdview_core::model::Length>,
        height: Option<mdview_core::model::Length>,
    ) -> (f32, f32) {
        use mdview_core::model::Length;
        let content = self.content_width().max(1.0);
        let wide = |l: Length| match l {
            Length::Px(p) => p as f32 * self.scale,
            Length::Percent(p) => content * f32::from(p) / 100.0,
        };
        let tall = |l: Length| match l {
            Length::Px(p) => p as f32 * self.scale,
            // A percentage of a height has no reference box in running text.
            Length::Percent(_) => natural.1,
        };
        let (mut w, mut h) = match (width, height) {
            (None, None) => natural,
            (Some(w), None) => {
                let w = wide(w);
                (w, natural.1 * w / natural.0.max(1.0))
            }
            (None, Some(h)) => {
                let h = tall(h);
                (natural.0 * h / natural.1.max(1.0), h)
            }
            (Some(w), Some(h)) => (wide(w), tall(h)),
        };
        if w > content && width.is_some() {
            h *= content / w;
            w = content;
        }
        (w.max(1.0), h.max(1.0))
    }

    /// Draws a Mermaid diagram block as a centred image. `false` hands it back to the source view
    /// when the engine is missing or the source does not parse.
    /// Remembers where a formula or diagram block's picture is, for [`Hit::source`].
    fn note_source(&mut self, kind: SourceKind, block: u32, (x, y, w, h): (f32, f32, f32, f32)) {
        if self.record {
            let text = self.doc.blocks[block as usize].text;
            self.sources.push(SourceBox {
                x,
                y,
                w,
                h,
                source: Source {
                    kind,
                    start: text.start,
                    end: text.end,
                },
            });
        }
    }

    pub(super) fn diagram_block(&mut self, b: u32, left: f32, width: f32) -> bool {
        let block = self.doc.blocks[b as usize];
        if matches!(
            block.kind,
            BlockKind::Diagram {
                oversize: Some(_),
                ..
            }
        ) {
            return false;
        }
        let (start, end) = (block.text.start, block.text.end);
        let source = self.doc.text[start as usize..end as usize].trim();
        if source.is_empty() {
            return false;
        }
        // Twice the device scale, so 150% zoom and high DPI stay sharp; the box is halved.
        let scale = 2.0 * self.scale;
        let dark = self.theme.name == "dark";
        let (key, found) = self.store.mermaid(source, scale, dark, self.allow);
        self.note(key, &found, true);
        let Found::Ready(image, _) = found else {
            if matches!(found, Found::Unknown) {
                // Not rendered, and this pass may not ask (see `GUESS_DIAGRAM`).
                self.y += self.px(8.0 + GUESS_DIAGRAM + 16.0);
                return true;
            }
            return false;
        };
        let (image_w, image_h) = (image.width as f32 / 2.0, image.height as f32 / 2.0);
        let avail = (width - 2.0 * self.px(16.0)).max(1.0);
        let shrink = (avail / image_w).min(1.0);
        let (w, h) = (image_w * shrink, image_h * shrink);
        let x = left + ((width - w) / 2.0).round();
        let y = self.y + self.px(8.0);
        self.paints.push(Paint::Image {
            index: key,
            x,
            y,
            w,
            h,
        });
        self.note_source(SourceKind::Diagram, b, (x, y, w, h));
        self.y = y + h + self.px(16.0);
        true
    }

    /// Draws a `$$...$$` block as a centred formula. `false` hands it back to the source view when
    /// there is no engine or TeX will not parse.
    pub(super) fn math_block(&mut self, b: u32, left: f32, width: f32) -> bool {
        let block = self.doc.blocks[b as usize];
        if matches!(block.kind, BlockKind::Math { oversize: Some(_) }) {
            return false;
        }
        let (start, end) = (block.text.start, block.text.end);
        let source = self.doc.text[start as usize..end as usize].trim();
        if source.is_empty() {
            return false;
        }
        // Display style runs a little larger than body text, and twice that keeps it crisp.
        let size = self.theme.body_size * 1.15 * 2.0 * self.scale;
        let (key, found) = self
            .store
            .math(source, size, self.theme.text, true, self.allow);
        self.note(key, &found, true);
        let Found::Ready(image, _) = found else {
            if matches!(found, Found::Unknown) {
                self.y += self.px(10.0 + GUESS_MATH + 18.0);
                return true;
            }
            return false;
        };
        let (image_w, image_h) = (image.width as f32 / 2.0, image.height as f32 / 2.0);
        let avail = (width - 2.0 * self.px(16.0)).max(1.0);
        let shrink = (avail / image_w).min(1.0);
        let (w, h) = (image_w * shrink, image_h * shrink);
        let x = left + ((width - w) / 2.0).round();
        let y = self.y + self.px(10.0);
        self.paints.push(Paint::Image {
            index: key,
            x,
            y,
            w,
            h,
        });
        self.note_source(SourceKind::Formula, b, (x, y, w, h));
        self.y = y + h + self.px(18.0);
        true
    }

    pub(super) fn code_block(&mut self, b: u32, left: f32, width: f32, label: Option<String>) {
        let source = self.doc.block_text(b).trim_end_matches('\n').to_string();
        let text_start = self.doc.blocks[b as usize].text.start;
        let mut inline = Inline::default();
        let dark = self.theme.name == "dark";
        if let Some(tokens) = highlight::tokens(&source, label.as_deref(), dark) {
            for token in tokens {
                inline.push_doc(
                    &source[token.range.clone()],
                    Style {
                        code: true,
                        color: Some(token.color),
                        ..Style::default()
                    },
                    text_start + token.range.start as u32,
                );
            }
        } else {
            inline.push_doc(
                &source,
                Style {
                    code: true,
                    ..Style::default()
                },
                text_start,
            );
        }
        let pad_x = self.px(16.0);
        let pad_y = self.px(13.0);
        let tag = label.filter(|l| !l.is_empty()).map(|label| {
            let mut tag = Inline::default();
            tag.push(
                &label,
                Style {
                    muted: true,
                    ..Style::default()
                },
            );
            let txt = Txt {
                size: 12.0,
                line_height: 1.3,
                weight: 400.0,
                mono: false,
                color: self.theme.muted,
                max: None,
            };
            let layout = self.text_layout(&tag, &txt);
            (tag, layout)
        });
        let mut txt = Txt {
            size: 14.0,
            line_height: 1.5,
            weight: 400.0,
            mono: true,
            color: self.theme.text,
            max: Some((width - 2.0 * pad_x).max(1.0)),
        };
        let mut layout = self.text_layout(&inline, &txt);
        if let Some((_, tag)) = &tag {
            // The label sits at the right of the first line. Code that would run under it wraps
            // short of it instead.
            let label_left = width - self.px(12.0) - tag.width() - self.px(8.0);
            let first_line_end = layout.lines().next().map_or(0.0, |line| {
                let m = line.metrics();
                m.offset + m.advance
            });
            if pad_x + first_line_end > label_left {
                txt.max = Some((label_left - pad_x).max(1.0));
                layout = self.text_layout(&inline, &txt);
            }
        }
        let size = txt.size;
        let text_height = layout.height().round().max(txt.size * self.scale);
        let height = text_height + 2.0 * pad_y;
        let radius = self.px(6.0);
        let hr = self.hair();
        let y0 = self.y;
        self.paints.push(Paint::Rect {
            x: left,
            y: y0,
            w: width,
            h: height,
            radius,
            color: self.theme.border,
        });
        self.paints.push(Paint::Rect {
            x: left + hr,
            y: y0 + hr,
            w: width - 2.0 * hr,
            h: height - 2.0 * hr,
            radius: (radius - hr).max(0.0),
            color: self.theme.code_bg,
        });
        if !source.is_empty() {
            self.emit(&inline, layout, left + pad_x, y0 + pad_y, size);
        }
        if let Some((tag, layout)) = tag {
            let x = left + width - self.px(12.0) - layout.width();
            self.emit(&tag, layout, x, y0 + self.px(8.0), 12.0);
        }
        self.y = y0 + height + self.px(16.0);
    }
}
