//! The window's pixels: lays the document out with `mdview-render`, then copies the visible slice
//! into a composition swap chain 1:1 with physical pixels.
use crate::bindings::*;
use crate::diag;
use mdview_core::Document;
use mdview_render::{Bitmap, DocumentRenderer, Options, PageLayout, Theme};
use std::ffi::c_void;
use std::time::Instant;
use windows_core::*;

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

/// One laid-out page. Replaced when the width, scale or theme changes.
struct Page {
    bitmap: Bitmap,
    /// Logical width the page was laid out for, compared to the panel's `ActualWidth`.
    logical_width: f32,
    scale: f32,
    theme: &'static str,
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
        let page_height = self
            .page_layout
            .as_ref()
            .map_or(0.0, |layout| layout.total_height());
        let max = (page_height - self.height as f32).max(0.0);
        self.scroll = (self.scroll + delta).clamp(0.0, max);
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
            width: logical_width / self.zoom,
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
            logical_width,
            scale,
            theme: self.theme.name,
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
        let left = (width - copy_width) / 2;
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
    }
}
