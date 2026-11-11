//! Images and engine-rendered blocks: file image loading and sizing, Mermaid diagram and display
//! math blocks, and the labelled code box (syntax-highlighted) they fall back to.

use super::inline::{Inline, Style, diagram_index, math_index};
use super::pager::{Pager, Txt};
use super::paint::Paint;
use crate::engine;
use crate::highlight;
use crate::image::{self, DecodedImage};
use mdview_core::model::BlockKind;
use std::path::PathBuf;
use std::rc::Rc;

impl<'a, 'b> Pager<'a, 'b> {
    /// Loads an image once per render. `None` for an empty src, a missing file, a blocked format
    /// or anything over the plan's limits.
    pub(super) fn image(&mut self, index: u32) -> Option<Rc<DecodedImage>> {
        if let Some(cached) = self.images.get(&index) {
            return cached.clone();
        }
        let info = self.doc.images[index as usize];
        // `<picture>` may name a source per colour scheme; the matching one wins when present.
        let themed = if self.theme.name == "dark" {
            self.doc.str(info.dark_src)
        } else {
            self.doc.str(info.light_src)
        };
        let src = if themed.is_empty() {
            self.doc.str(info.src)
        } else {
            themed
        };
        let decoded = if src.is_empty() {
            None
        } else {
            let path = PathBuf::from(src);
            let path = match &self.base_dir {
                Some(dir) if path.is_relative() => dir.join(path),
                _ => path,
            };
            let decoded = image::load(&path);
            decoded.map(Rc::new)
        };
        self.images.insert(index, decoded.clone());
        decoded
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
        let key = diagram_index(start);
        if !self.images.contains_key(&key) {
            // Twice the device scale, so 150% zoom and high DPI stay sharp; the box is halved.
            let scale = 2.0 * self.scale;
            let dark = self.theme.name == "dark";
            let decoded = engine::render_mermaid(source, scale, dark).map(Rc::new);
            self.images.insert(key, decoded);
        }
        let Some((image_w, image_h)) = self
            .images
            .get(&key)
            .and_then(|slot| slot.as_ref())
            .map(|image| (image.width as f32 / 2.0, image.height as f32 / 2.0))
        else {
            return false;
        };
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
        let key = math_index(start);
        if !self.images.contains_key(&key) {
            // Display style runs a little larger than body text, and twice that keeps it crisp.
            let size = self.theme.body_size * 1.15 * 2.0 * self.scale;
            let decoded = engine::render_math(source, true, size, self.theme.text).map(Rc::new);
            self.images.insert(key, decoded);
        }
        let Some((image_w, image_h)) = self
            .images
            .get(&key)
            .and_then(|slot| slot.as_ref())
            .map(|image| (image.width as f32 / 2.0, image.height as f32 / 2.0))
        else {
            return false;
        };
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
        let txt = Txt {
            size: 14.0,
            line_height: 1.5,
            weight: 400.0,
            mono: true,
            color: self.theme.text,
            max: Some((width - 2.0 * pad_x).max(1.0)),
        };
        let layout = self.text_layout(&inline, &txt);
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
        if let Some(label) = label.filter(|l| !l.is_empty()) {
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
            let x = left + width - self.px(12.0) - layout.width();
            let size = txt.size;
            self.emit(&tag, layout, x, y0 + self.px(8.0), size);
        }
        self.y = y0 + height + self.px(16.0);
    }
}
