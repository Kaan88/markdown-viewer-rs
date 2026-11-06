//! Qt reference: a real QWidget subclass (Rust paintEvent) draws the specimen with
//! QPainter, and QWidget::render() captures it into a QImage. Going through a widget
//! matters: Qt's raster engine only uses LCD glyphs when painter.device() is a widget;
//! a plain QPainter on QImage is forced to grayscale.
//! Rust only; cxx/cxx-qt generate the C++ glue.
#![allow(non_snake_case)]
use core::pin::Pin;
use cxx_qt::Initialize;
use cxx_qt_lib::{QColor, QFont, QImage, QImageFormat, QPainter, QPen, QPoint, QPointF, QRectF, QRegion, QString};
use std::cell::Cell;
use std::os::raw::c_char;

include!("../../src/specimen_data.rs");

#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct RenderFlags(i32);
unsafe impl cxx::ExternType for RenderFlags {
    type Id = cxx::type_id!("QWidget::RenderFlags");
    type Kind = cxx::kind::Trivial;
}

#[cxx_qt::bridge]
pub mod ffi {
    #[namespace = "QWidget"]
    unsafe extern "C++" {
        type RenderFlags = crate::RenderFlags;
    }
    unsafe extern "C++" {
        include!("cxx-qt-lib/common.h");
        include!("cxx-qt-lib/qstring.h");
        include!("cxx-qt-lib/qimage.h");
        include!("cxx-qt-lib/qpoint.h");
        include!("cxx-qt-lib/qpointf.h");
        include!("cxx-qt-lib/qregion.h");
        include!("cxx-qt-lib/qpainter.h");
        include!(<QtGui/QPaintEvent>);
        include!(<QtWidgets/QApplication>);
        include!(<QtWidgets/QWidget>);
        type QString = cxx_qt_lib::QString;
        type QPoint = cxx_qt_lib::QPoint;
        type QPointF = cxx_qt_lib::QPointF;
        type QRegion = cxx_qt_lib::QRegion;
        type QPainter = cxx_qt_lib::QPainter;
        type QPaintEvent;
        type QPaintDevice;
        type QApplication;
        #[cxx_name = "QPainter"]
        type RawPainter;
        #[cxx_name = "QImage"]
        type RawImage;
        #[cxx_name = "drawText"]
        fn draw_text_f(self: Pin<&mut RawPainter>, p: &QPointF, s: &QString);
        #[cxx_name = "setDevicePixelRatio"]
        fn set_dpr(self: Pin<&mut RawImage>, r: f64);
        unsafe fn save(self: &RawImage, file: &QString, fmt: *const c_char, q: i32) -> bool;
    }
    unsafe extern "C++Qt" {
        #[qobject]
        type QWidget;
    }
    extern "RustQt" {
        #[qobject]
        #[base = QWidget]
        type Canvas = super::CanvasRust;
        #[cxx_override]
        #[cxx_name = "paintEvent"]
        unsafe fn paint_event(self: Pin<&mut Self>, e: *mut QPaintEvent);
    }
    unsafe extern "RustQt" {
        #[inherit]
        fn resize(self: Pin<&mut Canvas>, w: i32, h: i32);
        #[inherit]
        unsafe fn render(
            self: Pin<&mut Canvas>,
            target: *mut QPaintDevice,
            off: &QPoint,
            region: &QRegion,
            flags: RenderFlags,
        );
    }
    impl cxx_qt::Constructor<()> for Canvas {}
    #[namespace = "rust::cxxqtlib1"]
    unsafe extern "C++" {
        #[rust_name = "new_app"]
        unsafe fn make_unique(argc: &mut i32, argv: *mut *mut c_char) -> UniquePtr<QApplication>;
        #[rust_name = "new_canvas"]
        fn new_ptr() -> *mut Canvas;
        #[rust_name = "new_painter"]
        unsafe fn make_unique(w: *mut Canvas) -> UniquePtr<QPainter>;
    }
}

#[derive(Default)]
pub struct CanvasRust {}
impl Initialize for ffi::Canvas {
    fn initialize(self: Pin<&mut Self>) {}
}

thread_local! { static THEME: Cell<([u8; 3], [u8; 3])> = const { Cell::new(([0; 3], [0; 3])) }; }

fn col(c: [u8; 3]) -> QColor {
    QColor::from_rgb(c[0] as i32, c[1] as i32, c[2] as i32)
}

impl ffi::Canvas {
    unsafe fn paint_event(self: Pin<&mut Self>, _e: *mut ffi::QPaintEvent) {
        let (bg, fg) = THEME.with(|t| t.get());
        let me = self.get_unchecked_mut() as *mut ffi::Canvas;
        let mut up = ffi::new_painter(me);
        let mut p: Pin<&mut QPainter> = up.pin_mut();
        let raw = p.as_mut().get_unchecked_mut() as *mut QPainter as *mut ffi::RawPainter;
        p.as_mut().fill_rect(&QRectF::new(0.0, 0.0, W as f64, H as f64), &col(bg));
        p.as_mut().set_pen(&QPen::from(&col(fg)));
        p.as_mut().set_layout_direction(cxx_qt_lib::LayoutDirection::LayoutDirectionAuto);
        for &(mono, px, y, t) in LINES {
            let mut f = QFont::default();
            f.set_family(&QString::from(if mono { MONO } else { BODY }));
            f.set_pixel_size(px as i32);
            p.as_mut().set_font(&f);
            Pin::new_unchecked(&mut *raw).draw_text_f(&QPointF::new(MARGIN as f64, y as f64), &QString::from(t));
        }
    }
}

fn main() {
    let dir = std::env::args().nth(1).expect("out dir");
    let argc = Box::leak(Box::new(1i32));
    let arg0 = Box::leak(Box::new(*b"qtref\0"));
    let argv = Box::leak(Box::new([arg0.as_mut_ptr() as *mut c_char, std::ptr::null_mut()]));
    let _app = unsafe { ffi::new_app(argc, argv.as_mut_ptr()) };
    let cv = ffi::new_canvas();
    let mut cv = unsafe { Pin::new_unchecked(&mut *cv) };
    cv.as_mut().resize(W as i32, H as i32);
    for &(pct, s) in SCALES {
        for &(name, bg, fg) in THEMES {
            THEME.with(|t| t.set((bg, fg)));
            let (pw, ph) = ((W * s).ceil() as i32, (H * s).ceil() as i32);
            let mut img = QImage::from_width_height_and_format(pw, ph, QImageFormat::Format_RGB32);
            img.fill(&col(bg));
            let iptr = &mut img as *mut QImage;
            // SAFETY: same C++ object; QImage's first (only) base is QPaintDevice.
            unsafe {
                Pin::new_unchecked(&mut *(iptr as *mut ffi::RawImage)).set_dpr(s as f64);
                cv.as_mut().render(iptr as *mut ffi::QPaintDevice, &QPoint::new(0, 0), &QRegion::default(), RenderFlags(0));
            }
            let out = format!("{dir}/qt_{name}_{pct}.png");
            let ok = unsafe {
                (&*(iptr as *const ffi::RawImage)).save(&QString::from(&out), std::ptr::null::<c_char>(), -1)
            };
            assert!(ok, "save {out}");
        }
    }
    // Exits without an event loop; no window is ever shown.
}
