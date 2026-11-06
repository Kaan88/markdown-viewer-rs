//! Flip-model composition swap chain showing a 1 px checkerboard through UpdateSubresource.
use crate::bindings::*;
use std::ffi::c_void;
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
            crate::diag::trace(&format!("hardware D3D11 failed: {:#010x}", e.code().0));
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
        crate::diag::trace(&format!(
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

pub struct Renderer {
    gpu: Gpu,
    panel: SwapChainPanel,
    swap_chain: Option<IDXGISwapChain1>,
    width: u32,
    height: u32,
    pixels: Vec<u32>,
    pub scroll: i32,
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
            scroll: 0,
        }
    }

    /// Resizes to the panel's physical size if needed, redraws and presents. Returns false while
    /// the panel has no size yet.
    pub fn draw(&mut self) -> Result<bool> {
        let element: FrameworkElement = self.panel.cast()?;
        let (scale_x, scale_y) = (self.panel.CompositionScaleX()?, self.panel.CompositionScaleY()?);
        let width = (element.ActualWidth()? * scale_x as f64).ceil() as u32;
        let height = (element.ActualHeight()? * scale_y as f64).ceil() as u32;
        if (width, height) != (self.width, self.height) {
            crate::diag::trace(&format!("panel {width}x{height} scale {scale_x}x{scale_y}"));
        }
        if width == 0 || height == 0 {
            return Ok(false);
        }
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
                crate::diag::trace("swap chain attached to SwapChainPanel");
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

            self.width = width;
            self.height = height;
            self.fill();
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

    /// 1 px checkerboard, with a blue band that follows the wheel so input is visible.
    fn fill(&mut self) {
        let (width, height) = (self.width as usize, self.height as usize);
        self.pixels.resize(width * height, 0);
        let band = (self.scroll.rem_euclid(height as i32)) as usize;
        for (y, row) in self.pixels.chunks_exact_mut(width).enumerate() {
            let in_band = y >= band && y < band + 40;
            for (x, pixel) in row.iter_mut().enumerate() {
                *pixel = match ((x + y) & 1 == 0, in_band) {
                    (true, false) => 0xFF00_0000,
                    (false, false) => 0xFFFF_FFFF,
                    (true, true) => 0xFF00_0040,
                    (false, true) => 0xFF60_90FF,
                };
            }
        }
    }
}
