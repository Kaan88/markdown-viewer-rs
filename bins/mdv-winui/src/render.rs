//! The window's pixels: lays the document out with `mdview-render`, then copies the visible slice
//! into a composition swap chain 1:1 with physical pixels.
use crate::bindings::*;
use crate::diag;
use mdview_core::Document;
use mdview_render::{Bitmap, DocumentRenderer, Options, PageLayout, Theme};
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

/// The visible slice of the page, ready to blit.
struct Page {
    bitmap: Bitmap,
}

pub struct Renderer {
    gpu: Gpu,
    panel: SwapChainPanel,
    swap_chain: Option<IDXGISwapChain1>,
    width: u32,
    height: u32,
    pixels: Vec<u32>,
    page: Option<Page>,
    page_layout: Option<PageLayout>,
    renderer: DocumentRenderer,
    document: Option<Document>,
    base_dir: Option<std::path::PathBuf>,
    theme: Theme,
    zoom: f32,
    scroll: f32,
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
            pixels: Vec::new(),
            page: None,
            page_layout: None,
            renderer: DocumentRenderer::new(),
            document: None,
            base_dir: None,
            theme: mdview_render::LIGHT,
            zoom: 1.0,
            scroll: 0.0,
            scale: 1.0,
        }
    }

    pub fn set_document(&mut self, document: Document, base_dir: Option<std::path::PathBuf>) {
        self.document = Some(document);
        self.base_dir = base_dir;
        self.page = None;
        self.page_layout = None;
        self.scroll = 0.0;
    }

    pub fn set_zoom(&mut self, zoom: f32) {
        self.zoom = zoom.clamp(0.5, 3.0);
        self.page = None;
        self.page_layout = None;
    }

    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
        self.page = None;
        self.page_layout = None;
    }

    pub fn scroll_by(&mut self, delta: f32) {
        self.scroll_to(self.scroll + delta);
    }

    pub fn viewport_height(&self) -> u32 {
        self.height
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
        self.refresh(logical_width, scale, height);

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

            let resized = (width, height) != (self.width, self.height);
            self.width = width;
            self.height = height;
            if resized {
                self.fill_background();
            }
            self.blit();
            let back_buffer: ID3D11Texture2D = swap_chain.GetBuffer(0)?;
            self.gpu.context.UpdateSubresource(
                &back_buffer,
                0,
                None,
                self.pixels.as_ptr().cast(),
                width * 4,
                0,
            );
            swap_chain.Present(1, 0).ok()?;
        }
        Ok(true)
    }

    fn refresh(&mut self, logical_width: f32, scale: f32, height: u32) {
        let Some(document) = &self.document else {
            self.page = None;
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
        self.scroll = self
            .scroll
            .clamp(0.0, (layout.total_height() - height as f32).max(0.0));
        let slice = self
            .renderer
            .view(document, &options, layout, self.scroll, height as f32);
        self.page = Some(Page {
            bitmap: slice.bitmap,
        });
    }

    fn background(&self) -> u32 {
        let [r, g, b] = self.theme.bg;
        0xFF00_0000 | (r as u32) << 16 | (g as u32) << 8 | b as u32
    }

    fn fill_background(&mut self) {
        let background = self.background();
        self.pixels
            .resize(self.width as usize * self.height as usize, 0);
        self.pixels.fill(background);
    }

    /// Copies the rows of the page that are in view. The page is centred if it is narrower than
    /// the panel, which only happens when the two round differently.
    fn blit(&mut self) {
        if self.pixels.len() != self.width as usize * self.height as usize {
            self.fill_background();
        }
        let Some(page) = &self.page else {
            return;
        };
        let page_width = page.bitmap.width as usize;
        let page_height = page.bitmap.height as usize;
        let width = self.width as usize;
        let height = self.height as usize;
        let copy_width = width.min(page_width);
        let left = 0;
        // The bitmap is BGRA bytes, which is what little-endian u32 0xAARRGGBB holds too.
        let source: &[u32] = unsafe {
            std::slice::from_raw_parts(page.bitmap.pixels.as_ptr().cast(), page_width * page_height)
        };
        let top = 0i64;
        for y in 0..height.min(page_height) {
            let source_y = top + y as i64;
            if source_y < 0 || source_y >= page_height as i64 {
                continue;
            }
            let start = source_y as usize * page_width;
            let destination = y * width + left;
            self.pixels[destination..destination + copy_width]
                .copy_from_slice(&source[start..start + copy_width]);
        }
        self.draw_bar();
    }

    /// Paints the scrollbar gutter over the right edge: a hairline separator and a thumb.
    fn draw_bar(&mut self) {
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
        let width = self.width as usize;
        let height = self.height as usize;
        let x0 = bar.x.max(0.0) as usize;
        let separator_x = x0.saturating_sub(1).min(width.saturating_sub(1));
        for y in 0..height {
            self.pixels[y * width + separator_x] = separator;
        }
        let left = (bar.x + bar.pad) as usize;
        let right = ((bar.x + bar.width - bar.pad) as usize).min(width);
        let top = (bar.thumb_y + bar.pad) as usize;
        let bottom = ((bar.thumb_y + bar.thumb_height - bar.pad) as usize).min(height);
        for y in top..bottom {
            let row = y * width;
            for x in left..right {
                self.pixels[row + x] = thumb;
            }
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
