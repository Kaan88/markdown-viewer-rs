//! The window's pixels: lays the document out with `mdview-render`, then brings the GPU's copy of
//! the viewport up to date at 1:1 with physical pixels.
//!
//! The page lives in a pair of D3D11 textures rather than in a CPU buffer, because the renderer
//! reports what changed between frames ([`mdview_render::Frame::band`] and
//! [`mdview_render::Frame::shift`]). A scroll frame therefore sends only the band that came into
//! view and moves the rest with a GPU-side copy; the back buffer is filled from the page texture
//! every frame, since a flip-model swap chain does not keep its contents.
use crate::bindings::*;
use crate::diag;
use mdview_core::Document;
use mdview_render::{DocumentRenderer, Hit, Options, PageLayout, Rect, Theme};
use std::ffi::c_void;
use std::time::Instant;
use windows_core::*;

/// Width of the scrollbar gutter, in logical px. The page is laid out this much narrower.
const BAR: f32 = 14.0;

// From microsoft.ui.xaml.media.dxinterop.h; the WinUI IID differs from the UWP one.
interface_decl! {
    unsafe trait ISwapChainPanelNative(ISwapChainPanelNative_Vtbl, ISwapChainPanelNative_Impl)
        : IUnknown = 0x63aad0b8_7c24_40ff_85a8_640d944cc325
    {
        unsafe fn SetSwapChain(&self, swap_chain: *mut c_void) -> HRESULT;
    }
}

pub struct Gpu {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    factory: IDXGIFactory2,
}
// A device is free-threaded; the context moves to the UI thread once and stays there.
unsafe impl Send for Gpu {}

pub fn create_gpu() -> Result<Gpu> {
    let levels = [D3D_FEATURE_LEVEL_11_0];
    let create = |driver| unsafe {
        let mut device = std::ptr::null_mut();
        D3D11CreateDevice(
            std::ptr::null_mut(),
            driver,
            std::ptr::null_mut(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT as u32,
            levels.as_ptr(),
            levels.len() as u32,
            D3D11_SDK_VERSION as u32,
            &mut device,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
        .ok()?;
        Ok::<_, Error>(ID3D11Device::from_raw(device))
    };
    // No GPU, as in a VM: hardware creation can fail, and WARP then renders in software.
    let (device, driver) = match create(D3D_DRIVER_TYPE_HARDWARE) {
        Ok(device) => (device, "hardware"),
        Err(e) => {
            diag::trace(&format!("hardware D3D11 failed: {:#010x}", e.code().0));
            (create(D3D_DRIVER_TYPE_WARP)?, "WARP")
        }
    };
    unsafe {
        let context = device.GetImmediateContext()?;
        let adapter = device.cast::<IDXGIDevice>()?.GetAdapter()?;
        let factory = adapter.GetParent::<IDXGIFactory2>()?;
        let mut desc = DXGI_ADAPTER_DESC::default();
        _ = adapter.GetDesc(&mut desc);
        let name = String::from_utf16_lossy(&desc.Description);
        diag::trace(&format!(
            "D3D11 device ready: driver={driver} adapter={}",
            name.trim_end_matches('\0')
        ));
        Ok(Gpu {
            device,
            context,
            factory,
        })
    }
}

/// How a frame reached the GPU, for the trace log.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Sent {
    /// Everything, because the caller had nothing to reuse.
    Whole,
    /// Nothing: the frame did not change.
    Nothing,
    /// Just this many rows of the page, at this row.
    Band(u32, u32),
}

pub struct Renderer {
    gpu: Gpu,
    panel: SwapChainPanel,
    swap_chain: Option<IDXGISwapChain1>,
    width: u32,
    height: u32,
    /// The page as the GPU has it: two textures, because a scroll moves one into the other.
    pages: [Option<ID3D11Texture2D>; 2],
    page_size: (u32, u32),
    /// The texture holding the page this frame; the other one is the shift destination.
    front: usize,
    /// The scrollbar gutter, as a strip beside the page.
    gutter: Vec<u32>,
    gutter_x: u32,
    gutter_width: u32,
    /// A panel-sized background, for the frames before a document has been laid out.
    blank: Vec<u32>,
    page_layout: Option<PageLayout>,
    renderer: DocumentRenderer,
    document: Option<Document>,
    base_dir: Option<std::path::PathBuf>,
    /// The options of the last frame, so hit tests and selection use the same page.
    options: Option<Options>,
    /// The selection, as two offsets into `Document::text`.
    selection: Option<(u32, u32)>,
    theme: Theme,
    zoom: f32,
    scroll: f32,
    /// Whether a scroll may reuse the previous frame on the GPU. `--no-shift` turns that off, which
    /// is the reference a shifted frame has to match.
    shift: bool,
    /// An offset to scroll to as soon as the first layout exists, for `--scroll` and screenshots.
    pending_scroll: Option<f32>,
    /// Device px per logical px of the last frame, for the scrollbar gutter.
    scale: f32,
}

impl Renderer {
    pub fn new(gpu: Gpu, panel: SwapChainPanel) -> Self {
        Self {
            gpu,
            panel,
            swap_chain: None,
            width: 0,
            height: 0,
            pages: [None, None],
            page_size: (0, 0),
            front: 0,
            gutter: Vec::new(),
            gutter_x: 0,
            gutter_width: 0,
            blank: Vec::new(),
            page_layout: None,
            renderer: DocumentRenderer::new(),
            document: None,
            base_dir: None,
            options: None,
            selection: None,
            theme: mdview_render::LIGHT,
            zoom: 1.0,
            scroll: 0.0,
            shift: true,
            pending_scroll: None,
            scale: 1.0,
        }
    }

    /// Turns the GPU-side shift off, so every frame is sent whole. Used by `--no-shift` to check
    /// that a shifted frame is the same picture.
    pub fn set_shift(&mut self, shift: bool) {
        self.shift = shift;
    }

    /// Scrolls here once the page has been laid out, for `--scroll` and screenshots.
    pub fn set_pending_scroll(&mut self, top: f32) {
        self.pending_scroll = Some(top.max(0.0));
    }

    pub fn set_document(&mut self, document: Document, base_dir: Option<std::path::PathBuf>) {
        self.document = Some(document);
        self.base_dir = base_dir;
        self.page_layout = None;
        self.options = None;
        self.selection = None;
        self.scroll = 0.0;
    }

    pub fn set_zoom(&mut self, zoom: f32) {
        self.zoom = zoom.clamp(0.5, 3.0);
        self.page_layout = None;
        self.options = None;
        self.selection = None;
    }

    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
        self.page_layout = None;
        self.options = None;
        self.selection = None;
    }

    pub fn scroll_by(&mut self, delta: f32) {
        self.scroll_to(self.scroll + delta);
    }

    /// Device px per logical px, as the last frame saw it.
    pub fn device_scale(&self) -> f32 {
        self.scale
    }

    /// The current scroll offset in device px.
    pub fn offset(&self) -> f32 {
        self.scroll
    }

    pub fn page_height(&self) -> f32 {
        self.page_layout
            .as_ref()
            .map_or(0.0, |layout| layout.total_height())
    }

    pub fn max_scroll(&self) -> f32 {
        (self.page_height() - self.height as f32).max(0.0)
    }

    pub fn scroll_to(&mut self, value: f32) {
        self.scroll = value.clamp(0.0, self.max_scroll());
    }

    /// What is under a panel position, in device px: the document offset and a link or footnote.
    pub fn hit_test(&mut self, x: f32, y: f32) -> Option<Hit> {
        let document = self.document.as_ref()?;
        let options = self.options.as_ref()?;
        let layout = self.page_layout.as_ref()?;
        self.renderer
            .hit_test(document, options, layout, x, y + self.scroll)
    }

    pub fn set_selection(&mut self, selection: Option<(u32, u32)>) {
        self.selection = selection;
    }

    /// Selects the first occurrence of a word in the document, for `--select-test`.
    pub fn select_word(&mut self, word: &str) -> Option<(u32, u32)> {
        let start = {
            let document = self.document.as_ref()?;
            document.text.find(word)? as u32
        };
        let range = (start, start + word.len() as u32);
        self.selection = Some(range);
        Some(range)
    }

    /// The first page rectangle of the current selection, for logging the test's position.
    pub fn selection_bounds(&mut self) -> Option<Rect> {
        self.selection_rects().into_iter().next()
    }

    /// The selected text's page rectangles in device px.
    pub fn selection_rects(&mut self) -> Vec<Rect> {
        let Some((from, to)) = self.selection else {
            return Vec::new();
        };
        if from == to {
            return Vec::new();
        }
        let Some(document) = self.document.as_ref() else {
            return Vec::new();
        };
        let Some(options) = self.options.as_ref() else {
            return Vec::new();
        };
        let Some(layout) = self.page_layout.as_ref() else {
            return Vec::new();
        };
        self.renderer
            .selection_rects(document, options, layout, from.min(to), from.max(to))
    }

    /// Scrolls to a `#fragment` in the current document. False when no heading or anchor matches.
    pub fn scroll_to_fragment(&mut self, fragment: &str) -> bool {
        let row = {
            let Some(document) = self.document.as_ref() else {
                return false;
            };
            let Some(layout) = self.page_layout.as_ref() else {
                return false;
            };
            document
                .resolve_fragment(fragment)
                .and_then(|block| layout.y_of(document, block))
        };
        let Some(row) = row else {
            return false;
        };
        self.scroll_to(row - 24.0 * self.scale);
        true
    }

    /// Scrolls to a footnote entry, by index into `Document::footnotes`.
    pub fn follow_footnote(&mut self, index: u32) -> bool {
        let row = {
            let Some(document) = self.document.as_ref() else {
                return false;
            };
            let Some(layout) = self.page_layout.as_ref() else {
                return false;
            };
            document
                .footnotes
                .get(index as usize)
                .and_then(|note| layout.footnote_y(note.number))
        };
        let Some(row) = row else {
            return false;
        };
        self.scroll_to(row - 24.0 * self.scale);
        true
    }

    /// A link's destination as written, for the shell to open or navigate.
    pub fn link_destination(&self, index: u32) -> Option<String> {
        let document = self.document.as_ref()?;
        let link = document.links.get(index as usize)?;
        Some(document.str(link.dest).to_string())
    }

    /// Home, End, the arrows, Page Up/Down and Space, in device px. False when the key is not ours.
    pub fn key(&mut self, code: u32) -> bool {
        const VK_HOME: u32 = 0x24;
        const VK_END: u32 = 0x23;
        const VK_LEFT: u32 = 0x25;
        const VK_UP: u32 = 0x26;
        const VK_RIGHT: u32 = 0x27;
        const VK_DOWN: u32 = 0x28;
        const VK_PRIOR: u32 = 0x21;
        const VK_NEXT: u32 = 0x22;
        const VK_SPACE: u32 = 0x20;
        const STEP: f32 = 48.0;
        let page = self.height as f32;
        match code {
            VK_DOWN | VK_RIGHT => self.scroll_to(self.scroll + STEP),
            VK_UP | VK_LEFT => self.scroll_to(self.scroll - STEP),
            VK_NEXT | VK_SPACE => self.scroll_to(self.scroll + page * 0.9),
            VK_PRIOR => self.scroll_to(self.scroll - page * 0.9),
            VK_HOME => self.scroll_to(0.0),
            VK_END => self.scroll_to(f32::MAX),
            _ => return false,
        }
        true
    }

    /// The scrollbar gutter and thumb, in panel device px, or `None` before the first layout.
    pub fn bar(&self) -> Option<Bar> {
        if self.page_layout.is_none() || self.width == 0 || self.height == 0 {
            return None;
        }
        let w = (BAR * self.scale).round().max(8.0);
        let pad = (3.0 * self.scale).round().max(2.0);
        let height = self.height as f32;
        let travel = (height - pad * 2.0).max(1.0);
        let page = self.page_height();
        let visible = (height / page.max(1.0)).clamp(0.02, 1.0);
        let thumb_height = (travel * visible).clamp(28.0 * self.scale, travel);
        let max = self.max_scroll().max(1.0);
        let progress = (self.scroll / max).clamp(0.0, 1.0);
        Some(Bar {
            x: self.width as f32 - w,
            width: w,
            pad,
            travel,
            thumb_y: pad + (travel - thumb_height) * progress,
            thumb_height,
            max,
        })
    }

    /// Scroll offset for a press at `y` in the gutter, and how far into the thumb it lands.
    pub fn press_on_bar(&mut self, bar: &Bar, y: f32) -> f32 {
        let thumb_top = bar.thumb_y;
        let thumb_bottom = bar.thumb_y + bar.thumb_height;
        let grab = if y >= thumb_top && y <= thumb_bottom {
            y - thumb_top
        } else {
            bar.thumb_height / 2.0
        };
        let travel = (bar.travel - bar.thumb_height).max(1.0);
        let target = ((y - grab - bar.pad) / travel).clamp(0.0, 1.0);
        self.scroll_to(target * bar.max);
        grab
    }

    /// Scroll offset while dragging the thumb with a grab of `grab` px.
    pub fn drag_to(&mut self, bar: &Bar, y: f32, grab: f32) {
        let travel = (bar.travel - bar.thumb_height).max(1.0);
        let target = ((y - grab - bar.pad) / travel).clamp(0.0, 1.0);
        self.scroll_to(target * bar.max);
    }

    /// Resizes to the panel's physical size if needed, redraws and presents. Returns false while
    /// the panel has no size yet.
    pub fn draw(&mut self) -> Result<bool> {
        let element: FrameworkElement = self.panel.cast()?;
        let (scale_x, scale_y) = (
            self.panel.CompositionScaleX()?,
            self.panel.CompositionScaleY()?,
        );
        let width = (element.ActualWidth()? * scale_x as f64).ceil() as u32;
        let height = (element.ActualHeight()? * scale_y as f64).ceil() as u32;
        if width == 0 || height == 0 {
            return Ok(false);
        }
        let logical_width = element.ActualWidth()? as f32;
        let scale = scale_x as f32;
        self.scale = scale;
        let started = Instant::now();
        self.refresh(logical_width, scale, height);
        let painted = started.elapsed();

        unsafe {
            if self.swap_chain.is_none() {
                let desc = DXGI_SWAP_CHAIN_DESC1 {
                    Width: width,
                    Height: height,
                    Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    SampleDesc: DXGI_SAMPLE_DESC {
                        Count: 1,
                        Quality: 0,
                    },
                    BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                    BufferCount: 2,
                    Scaling: DXGI_SCALING_STRETCH,
                    SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
                    AlphaMode: DXGI_ALPHA_MODE_IGNORE,
                    ..Default::default()
                };
                let swap_chain = self.gpu.factory.CreateSwapChainForComposition(
                    &self.gpu.device,
                    &desc,
                    None,
                )?;
                self.panel
                    .cast::<ISwapChainPanelNative>()?
                    .SetSwapChain(swap_chain.as_raw())
                    .ok()?;
                self.swap_chain = Some(swap_chain);
                diag::trace("swap chain attached to SwapChainPanel");
            } else if (width, height) != (self.width, self.height) {
                self.gpu.context.Flush();
                self.swap_chain
                    .as_ref()
                    .unwrap()
                    .ResizeBuffers(2, width, height, DXGI_FORMAT_B8G8R8A8_UNORM, 0)
                    .ok()?;
            }
            let swap_chain = self.swap_chain.clone().unwrap();
            // Inverse scale, so one swap chain pixel maps to one physical pixel at any DPI.
            swap_chain
                .cast::<IDXGISwapChain2>()?
                .SetMatrixTransform(&DXGI_MATRIX_3X2_F {
                    _11: 1.0 / scale_x,
                    _22: 1.0 / scale_y,
                    ..Default::default()
                })
                .ok()?;

            if (width, height) != (self.width, self.height) {
                // Every buffer of the old size is useless now, and so is the gutter strip.
                self.width = width;
                self.height = height;
                self.pages = [None, None];
                self.page_size = (0, 0);
            }
            self.layout_gutter();
            let uploading = Instant::now();
            let sent = self.upload_page()?;
            let back_buffer: ID3D11Texture2D = swap_chain.GetBuffer(0)?;
            self.copy_page(&back_buffer)?;
            self.overlay_selection(&back_buffer)?;
            self.upload_gutter(&back_buffer)?;
            let uploaded = uploading.elapsed();
            let presenting = Instant::now();
            swap_chain.Present(1, 0).ok()?;
            let presented = presenting.elapsed();
            let total = started.elapsed();
            let rows = match sent {
                Sent::Whole => self.height,
                Sent::Nothing => 0,
                Sent::Band(_, rows) => rows,
            };
            diag::trace(&format!(
                "frame {width}x{height}: paint {:.1} upload {:.1} ({} of {rows} rows) present {:.1} total {:.1} ms",
                painted.as_secs_f64() * 1000.0,
                uploaded.as_secs_f64() * 1000.0,
                match sent {
                    Sent::Whole => "whole".to_string(),
                    Sent::Nothing => "no change".to_string(),
                    Sent::Band(top, _) => format!("band at {top}"),
                },
                presented.as_secs_f64() * 1000.0,
                total.as_secs_f64() * 1000.0,
            ));
        }
        Ok(true)
    }

    fn refresh(&mut self, logical_width: f32, scale: f32, height: u32) {
        let Some(document) = &self.document else {
            self.page_layout = None;
            return;
        };
        let options = Options {
            width: ((logical_width - BAR) / self.zoom).max(1.0),
            scale: scale * self.zoom,
            theme: self.theme,
            max_height: None,
            base_dir: self.base_dir.clone(),
        };
        self.options = Some(options.clone());
        let stale = self
            .page_layout
            .as_ref()
            .is_none_or(|layout| !layout.matches(&options));
        if stale {
            let started = Instant::now();
            let layout = self.renderer.layout(document, &options);
            diag::trace(&format!(
                "height pass {logical_width:.0} dip at scale {scale:.2}: {:.1} ms, page {:.0} px",
                started.elapsed().as_secs_f64() * 1000.0,
                layout.total_height()
            ));
            self.page_layout = Some(layout);
        }
        let Some(layout) = &self.page_layout else {
            return;
        };
        let max_scroll = (layout.total_height() - height as f32).max(0.0);
        if let Some(top) = self.pending_scroll.take() {
            self.scroll = top.clamp(0.0, max_scroll);
        }
        self.scroll = self.scroll.clamp(0.0, max_scroll);
        self.renderer
            .view_frame(document, &options, layout, self.scroll, height as f32);
    }

    fn background(&self) -> u32 {
        let [r, g, b] = self.theme.bg;
        0xFF00_0000 | (r as u32) << 16 | (g as u32) << 8 | b as u32
    }

    /// Where the gutter strip starts and how wide it is: the scrollbar's own column, which is also
    /// as far as the page is copied. The page can be a few px wider than the width it was laid out
    /// for, and the bar has to cover that overhang rather than be clipped by it.
    fn layout_gutter(&mut self) {
        // One px of the page's overhang becomes the hairline separator beside the bar.
        let bar_x = self
            .bar()
            .map_or(self.width, |bar| (bar.x.max(0.0) as u32).saturating_sub(1))
            .min(self.width);
        if (bar_x, self.width - bar_x) != (self.gutter_x, self.gutter_width) {
            self.gutter.clear();
        }
        self.gutter_x = bar_x;
        self.gutter_width = self.width - bar_x;
        let needed = self.gutter_width as usize * self.height as usize;
        if self.gutter.len() != needed {
            self.gutter = vec![0; needed];
        }
        self.draw_gutter();
    }

    /// Paints the gutter strip: the page's background, a hairline separator and the thumb.
    fn draw_gutter(&mut self) {
        let height = self.height as usize;
        let width = self.gutter_width as usize;
        if width == 0 || height == 0 {
            return;
        }
        let background = self.background();
        self.gutter.fill(background);
        let Some(bar) = self.bar() else {
            return;
        };
        let [br, bg, bb] = self.theme.bg;
        let [tr, tg, tb] = self.theme.text;
        let mix = |amount: f32| -> u32 {
            let blend = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * amount) as u32;
            0xFF00_0000 | blend(br, tr) << 16 | blend(bg, tg) << 8 | blend(bb, tb)
        };
        let separator = mix(0.10);
        let thumb = mix(0.30);
        let left = self.gutter_x as usize;
        if let Some(column) = (bar.x.max(0.0) as usize).checked_sub(1)
            && column >= left
            && column - left < width
        {
            for y in 0..height {
                self.gutter[y * width + column - left] = separator;
            }
        }
        let Some((top, bottom)) = self.thumb_rows() else {
            return;
        };
        let first = (bar.x + bar.pad) as usize;
        let last = (bar.x + bar.width - bar.pad) as usize;
        let first = first.saturating_sub(left).min(width);
        let last = last.saturating_sub(left).min(width);
        for y in top as usize..bottom as usize {
            let row = y * width;
            for x in first..last {
                self.gutter[row + x] = thumb;
            }
        }
    }

    /// Rows of the panel the thumb covers, for deciding which gutter rows to re-send.
    fn thumb_rows(&self) -> Option<(u32, u32)> {
        let bar = self.bar()?;
        let height = self.height;
        let top = ((bar.thumb_y + bar.pad) as usize).min(height as usize) as u32;
        let bottom =
            ((bar.thumb_y + bar.thumb_height - bar.pad) as usize).min(height as usize) as u32;
        (bottom > top).then_some((top, bottom))
    }

    /// Brings the page texture up to date with the renderer's frame. A scroll moves the rows that
    /// stay on screen to the other texture and sends only the band that came into view.
    unsafe fn upload_page(&mut self) -> Result<Sent> {
        unsafe {
            let Some(frame) = self.renderer.frame() else {
                return Ok(Sent::Nothing);
            };
            let pixels = frame.bitmap.pixels.as_ptr().cast::<c_void>();
            let (width, height) = (frame.bitmap.width, frame.bitmap.height);
            let stride = width * 4;
            let recreate = self.page_size != (width, height) || self.pages[0].is_none();
            if recreate {
                let desc = D3D11_TEXTURE2D_DESC {
                    Width: width,
                    Height: height,
                    MipLevels: 1,
                    ArraySize: 1,
                    Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    SampleDesc: DXGI_SAMPLE_DESC {
                        Count: 1,
                        Quality: 0,
                    },
                    Usage: D3D11_USAGE_DEFAULT,
                    BindFlags: D3D11_BIND_SHADER_RESOURCE,
                    CPUAccessFlags: 0,
                    MiscFlags: 0,
                };
                for page in &mut self.pages {
                    *page = Some(self.gpu.device.create_texture_2d(&desc)?);
                }
                self.page_size = (width, height);
                self.front = 0;
            }
            let front = self.pages[self.front]
                .as_ref()
                .expect("page texture")
                .clone();
            let Some(band) = frame.band.filter(|_| !recreate && self.shift) else {
                self.gpu
                    .context
                    .UpdateSubresource(&front, 0, None, pixels, stride, 0);
                return Ok(Sent::Whole);
            };
            let (top, rows) = band;
            if rows == 0 {
                return Ok(Sent::Nothing);
            }
            let boxed = |top: u32, rows: u32| D3D11_BOX {
                left: 0,
                top,
                front: 0,
                right: width,
                bottom: top + rows,
                back: 1,
            };
            let source = pixels.add(top as usize * stride as usize);
            // Row `r` was row `r + shift`, so the rows that stay move by that much.
            let shift = frame.shift;
            if shift == 0 {
                self.gpu.context.UpdateSubresource(
                    &front,
                    0,
                    Some(&boxed(top, rows)),
                    source,
                    stride,
                    0,
                );
                return Ok(Sent::Band(top, rows));
            }
            let moved = shift.unsigned_abs().min(height);
            if moved == 0 || moved >= height {
                // Not a shift the GPU can make; send the whole frame instead.
                self.gpu
                    .context
                    .UpdateSubresource(&front, 0, None, pixels, stride, 0);
                return Ok(Sent::Whole);
            }
            let other = 1 - self.front;
            let destination = self.pages[other].as_ref().expect("page texture");
            let (source_top, source_bottom, destination_top) = if shift > 0 {
                (moved, height, 0)
            } else {
                (0, height - moved, moved)
            };
            let moved_box = D3D11_BOX {
                left: 0,
                top: source_top,
                front: 0,
                right: width,
                bottom: source_bottom,
                back: 1,
            };
            self.gpu.context.copy_subresource_region(
                destination,
                0,
                0,
                destination_top,
                &front,
                0,
                Some(&moved_box),
            );
            self.gpu.context.UpdateSubresource(
                destination,
                0,
                Some(&boxed(top, rows)),
                source,
                stride,
                0,
            );
            self.front = other;
            Ok(Sent::Band(top, rows))
        }
    }

    /// Fills the back buffer with the page, one GPU copy, or with plain background before there is
    /// a page to show.
    unsafe fn copy_page(&mut self, back_buffer: &ID3D11Texture2D) -> Result<()> {
        unsafe {
            let Some(page) = self.pages[self.front].clone() else {
                let len = self.width as usize * self.height as usize;
                self.blank.resize(len, 0);
                let background = self.background();
                self.blank.fill(background);
                self.gpu.context.UpdateSubresource(
                    back_buffer,
                    0,
                    None,
                    self.blank.as_ptr().cast(),
                    self.width * 4,
                    0,
                );
                return Ok(());
            };
            let (width, height) = self.page_size;
            let rows = height.min(self.height);
            let columns = width.min(self.gutter_x);
            let boxed = D3D11_BOX {
                left: 0,
                top: 0,
                front: 0,
                right: columns,
                bottom: rows,
                back: 1,
            };
            self.gpu
                .context
                .copy_subresource_region(back_buffer, 0, 0, 0, &page, 0, Some(&boxed));
            Ok(())
        }
    }

    /// Draws the selection over the back buffer's copy of the page. The page texture itself stays
    /// the document's pixels, so a selection never enters the frame delta or the scroll shift.
    unsafe fn overlay_selection(&mut self, back_buffer: &ID3D11Texture2D) -> Result<()> {
        unsafe {
            if self.selection.is_none() {
                return Ok(());
            }
            let rects = self.selection_rects();
            if rects.is_empty() {
                return Ok(());
            }
            let Some(frame) = self.renderer.frame() else {
                return Ok(());
            };
            let (width, rows) = (frame.bitmap.width as usize, frame.bitmap.height as usize);
            let mut spans = Vec::new();
            let (mut lo, mut hi) = (rows, 0usize);
            for rect in &rects {
                let top = ((rect.y - frame.top).floor().max(0.0) as usize).min(rows);
                let bottom = ((rect.y + rect.h - frame.top).ceil().max(0.0) as usize).min(rows);
                let x0 = (rect.x.max(0.0) as usize).min(width);
                let x1 = ((rect.x + rect.w).max(0.0) as usize).min(width);
                if bottom <= top || x1 <= x0 {
                    continue;
                }
                lo = lo.min(top);
                hi = hi.max(bottom);
                spans.push((top, bottom, x0, x1));
            }
            if lo >= hi || spans.is_empty() {
                return Ok(());
            }
            let mut strip = vec![0u8; width * (hi - lo) * 4];
            strip.copy_from_slice(&frame.bitmap.pixels[lo * width * 4..hi * width * 4]);
            let [sr, sg, sb] = self.theme.accent;
            let alpha = 72.0 / 255.0;
            let blend = |under: u8, over: u8| -> u8 {
                let value = (under as f32 * (1.0 - alpha) + over as f32 * alpha).round();
                value.clamp(0.0, 255.0) as u8
            };
            for (top, bottom, x0, x1) in spans {
                for y in top..bottom {
                    let base = (y - lo) * width * 4;
                    for x in x0..x1 {
                        let at = base + x * 4;
                        strip[at] = blend(strip[at], sb);
                        strip[at + 1] = blend(strip[at + 1], sg);
                        strip[at + 2] = blend(strip[at + 2], sr);
                    }
                }
            }
            let columns = (width as u32).min(self.gutter_x).max(1);
            let boxed = D3D11_BOX {
                left: 0,
                top: lo as u32,
                front: 0,
                right: columns,
                bottom: hi as u32,
                back: 1,
            };
            self.gpu.context.UpdateSubresource(
                back_buffer,
                0,
                Some(&boxed),
                strip.as_ptr().cast::<c_void>(),
                (width * 4) as u32,
                0,
            );
            Ok(())
        }
    }

    /// Sends the gutter strip. It is narrow enough to send whole, and it has to be: a flip-model
    /// back buffer alternates between two buffers, so any rows left unwritten would show the other
    /// buffer's contents.
    unsafe fn upload_gutter(&mut self, back_buffer: &ID3D11Texture2D) -> Result<()> {
        unsafe {
            let (width, height) = (self.gutter_width, self.height);
            if width == 0 || height == 0 {
                return Ok(());
            }
            let boxed = D3D11_BOX {
                left: self.gutter_x,
                top: 0,
                front: 0,
                right: self.gutter_x + width,
                bottom: height,
                back: 1,
            };
            self.gpu.context.UpdateSubresource(
                back_buffer,
                0,
                Some(&boxed),
                self.gutter.as_ptr().cast(),
                width * 4,
                0,
            );
            Ok(())
        }
    }
}

/// Where the scrollbar is, in panel device px.
pub struct Bar {
    pub x: f32,
    pub width: f32,
    pub pad: f32,
    pub travel: f32,
    pub thumb_y: f32,
    pub thumb_height: f32,
    pub max: f32,
}
