//! The only module allowed to contain `unsafe`. `crate::shell` forbids it.
//! Handles are raw pointers to objects owned by Qt's parent tree. In this spike every
//! object lives until process exit, which is what keeps the handles valid.
#![allow(unused_unsafe)] // pin!() has its own unsafe block, also used inside larger ones.
pub mod ffi;

use cxx::UniquePtr;
use cxx_qt::{casting::Upcast, CxxQtType};
use cxx_qt_lib::{
    KeyboardModifier, MouseButton, MouseButtons, QAnyStringView, QColor, QFlags, QFont,
    QFontSpacingType, QImage, QImageFormat, QList, QPoint, QPointF, QSize, QString,
};
use ffi::*;
use std::os::raw::c_char;
use std::pin::Pin;
use std::rc::Rc;

macro_rules! trivial_cxx {
    ($($name:ident = $id:literal),* $(,)?) => {$(
        #[repr(transparent)]
        #[derive(Clone, Copy)]
        pub struct $name(pub u32);
        unsafe impl cxx::ExternType for $name {
            type Id = cxx::type_id!($id);
            type Kind = cxx::kind::Trivial;
        }
    )*};
}
trivial_cxx!(
    TimerType = "Qt::TimerType",
    Alignment = "Qt::Alignment",
    Orientation = "Qt::Orientation",
    FocusPolicy = "Qt::FocusPolicy",
    ScrollBarPolicy = "Qt::ScrollBarPolicy",
    WidgetAttribute = "Qt::WidgetAttribute",
    BrushStyle = "Qt::BrushStyle",
    TextElideMode = "Qt::TextElideMode",
    ShortcutContext = "Qt::ShortcutContext",
    ScrollPhase = "Qt::ScrollPhase",
    EventType = "QEvent::Type",
    ColorGroup = "QPalette::ColorGroup",
    ColorRole = "QPalette::ColorRole",
    FrameShape = "QFrame::Shape",
    SequenceFormat = "QKeySequence::SequenceFormat",
    RenderFlags = "QWidget::RenderFlags",
    ProcessEventsFlags = "QEventLoop::ProcessEventsFlags",
);

pub const EV_MOUSE_MOVE: u32 = 5;
pub const EV_UPDATE_REQUEST: u32 = 77;
pub const EV_APP_PALETTE_CHANGE: u32 = 38;
pub const EV_THEME_CHANGE: u32 = 210;

// QPalette::ColorRole values.
pub const R_WINDOW_TEXT: u32 = 0;
pub const R_BUTTON: u32 = 1;
pub const R_LIGHT: u32 = 2;
pub const R_MIDLIGHT: u32 = 3;
pub const R_DARK: u32 = 4;
pub const R_MID: u32 = 5;
pub const R_TEXT: u32 = 6;
pub const R_BRIGHT_TEXT: u32 = 7;
pub const R_BUTTON_TEXT: u32 = 8;
pub const R_BASE: u32 = 9;
pub const R_WINDOW: u32 = 10;
pub const R_SHADOW: u32 = 11;
pub const R_HIGHLIGHT: u32 = 12;
pub const R_HIGHLIGHTED_TEXT: u32 = 13;
pub const R_LINK: u32 = 14;
pub const R_LINK_VISITED: u32 = 15;
pub const R_ALT_BASE: u32 = 16;
pub const R_TOOLTIP_BASE: u32 = 18;
pub const R_TOOLTIP_TEXT: u32 = 19;
pub const R_PLACEHOLDER: u32 = 20;

const G_ACTIVE: u32 = 0;
const G_DISABLED: u32 = 1;
const G_INACTIVE: u32 = 2;

/// cxx-qt requires signal closures to be `Send`, though direct connections on objects
/// of the GUI thread only ever run on that thread.
struct GuiThreadOnly<F>(F);
unsafe impl<F> Send for GuiThreadOnly<F> {}
impl<F> GuiThreadOnly<F> {
    fn get(&mut self) -> &mut F {
        &mut self.0
    }
}

/// Copyable handle to a Qt object.
pub struct H<T>(*mut T);
impl<T> Clone for H<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for H<T> {}

macro_rules! pin {
    ($h:expr) => {
        unsafe { Pin::new_unchecked(&mut *$h.0) }
    };
}
macro_rules! upcast_widget {
    ($($t:ty),*) => {$(
        impl H<$t> {
            pub fn widget(&self) -> H<QWidget> {
                H(unsafe { <$t as Upcast<QWidget>>::upcast_ptr(self.0) }.cast_mut())
            }
        }
    )*};
}
upcast_widget!(Canvas, QLineEdit, QLabel, QToolButton, QTabBar, QSplitter, QScrollBar, QTreeWidget, QMenu);

fn qs(s: &str) -> QString {
    QString::from(s)
}

/// An RGB colour in plain Rust, so the shell's theme maths stays safe code.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Rgb(pub u8, pub u8, pub u8);
impl Rgb {
    fn q(self) -> QColor {
        QColor::from_rgb(self.0 as i32, self.1 as i32, self.2 as i32)
    }
    fn qa(self, a: i32) -> QColor {
        let mut c = self.q();
        c.set_alpha(a);
        c
    }
    pub fn mix(self, o: Rgb, t: f64) -> Rgb {
        let m = |a: u8, b: u8| (a as f64 + (b as f64 - a as f64) * t).round() as u8;
        Rgb(m(self.0, o.0), m(self.1, o.1), m(self.2, o.2))
    }
    pub fn luma(self) -> f64 {
        0.299 * self.0 as f64 + 0.587 * self.1 as f64 + 0.114 * self.2 as f64
    }
    pub fn css(self) -> String {
        format!("rgb({},{},{})", self.0, self.1, self.2)
    }
    pub fn css_a(self, a: f64) -> String {
        format!("rgba({},{},{},{})", self.0, self.1, self.2, (a * 255.0).round() as i32)
    }
}

/// A QPalette seeded from the application's current one.
pub struct Palette(UniquePtr<QPalette>);
impl Palette {
    pub fn from_app() -> Self {
        Palette(new_palette())
    }
    pub fn get(&self, role: u32) -> Rgb {
        let c = self.0.pal_color(ColorGroup(G_ACTIVE), ColorRole(role));
        Rgb(c.red() as u8, c.green() as u8, c.blue() as u8)
    }
    /// Sets the role for the active and inactive groups, so that a window losing focus
    /// doesn't change the look of the chrome.
    pub fn set(&mut self, role: u32, c: Rgb) -> &mut Self {
        let q = c.q();
        for g in [G_ACTIVE, G_INACTIVE] {
            self.0.pin_mut().pal_set_color(ColorGroup(g), ColorRole(role), &q);
        }
        self
    }
    pub fn set_disabled(&mut self, role: u32, c: Rgb) -> &mut Self {
        self.0.pin_mut().pal_set_color(ColorGroup(G_DISABLED), ColorRole(role), &c.q());
        self
    }
    pub fn install_on_app(&self) {
        unsafe { QApplication::app_set_palette(&self.0, std::ptr::null()) }
    }
}

pub fn style_name() -> String {
    let s = QApplication::app_style();
    if s.is_null() { "none".into() } else { unsafe { &*s }.style_name().to_string() }
}

/// Events the shell observes on the canvas. All safe Rust on the other side.
pub trait CanvasHandler {
    fn paint(&self, target: &mut PaintTarget<'_>);
    fn resized(&self);
    fn wheel(&self, dy: i32);
    fn timer(&self, id: i32);
    fn update_request(&self);
    fn app_palette_changed(&self);
}

#[derive(Default)]
pub struct CanvasRust {
    handler: Option<Rc<dyn CanvasHandler>>,
}

impl cxx_qt::Initialize for Canvas {
    fn initialize(self: Pin<&mut Self>) {}
}

impl Canvas {
    fn handler(&self) -> Rc<dyn CanvasHandler> {
        self.rust().handler.clone().expect("canvas handler")
    }
    unsafe fn paint_event(self: Pin<&mut Self>, _e: *mut QPaintEvent) {
        let h = self.handler();
        let mut t = PaintTarget { canvas: self };
        h.paint(&mut t);
    }
    unsafe fn resize_event(self: Pin<&mut Self>, _e: *mut QResizeEvent) {
        self.handler().resized();
    }
    unsafe fn wheel_event(self: Pin<&mut Self>, e: *mut QWheelEvent) {
        self.handler().wheel((*e).angleDelta().y());
    }
    unsafe fn timer_event(self: Pin<&mut Self>, e: *mut QTimerEvent) {
        self.handler().timer((*e).timerId());
    }
    unsafe fn event_filter(self: Pin<&mut Self>, _w: *mut QObject, e: *mut QEvent) -> bool {
        match (*e).event_type().0 {
            EV_UPDATE_REQUEST => self.handler().update_request(),
            // Qt doesn't pass these on to QWidget::changeEvent, but windows do get them.
            EV_APP_PALETTE_CHANGE | EV_THEME_CHANGE => self.handler().app_palette_changed(),
            _ => {}
        }
        false
    }
}

extern "C" fn noop_cleanup(_: *mut cxx_qt_lib::c_void) {}

fn raw_painter(p: &mut UniquePtr<QPainter>) -> Pin<&mut RawPainter> {
    // cxx-qt-lib owns QPainter's cxx identity; RawPainter names the same C++ class so
    // this crate can bind the members cxx-qt-lib lacks.
    unsafe { Pin::new_unchecked(&mut *(p.as_mut().unwrap().get_unchecked_mut() as *mut _ as *mut RawPainter)) }
}

/// Valid only during paintEvent.
pub struct PaintTarget<'a> {
    canvas: Pin<&'a mut Canvas>,
}

impl PaintTarget<'_> {
    fn as_widget(&self) -> &QWidget {
        (*self.canvas).upcast()
    }
    pub fn dpr(&self) -> f64 {
        self.as_widget().devicePixelRatioF()
    }
    pub fn logical_size(&self) -> (i32, i32) {
        (self.as_widget().width(), self.as_widget().height())
    }
    /// Blits a 32 bit page 1:1 in device pixels at (x, y). The net identity transform
    /// keeps QPainter on the raster engine's integer blit path, so nothing is resampled.
    /// `background` first fills the area the page doesn't cover.
    pub fn blit_page(&mut self, page: &[u8], w: i32, h: i32, x: i32, y: i32, background: Option<Rgb>) {
        assert!(page.len() >= (w as usize) * (h as usize) * 4);
        let dpr = self.dpr();
        let (lw, lh) = self.logical_size();
        let canvas = unsafe { self.canvas.as_mut().get_unchecked_mut() as *mut Canvas };
        let mut p: UniquePtr<QPainter> = unsafe { new_painter(canvas) };
        let inv = new_transform(1.0 / dpr, 0.0, 0.0, 1.0 / dpr, 0.0, 0.0);
        let mut raw = raw_painter(&mut p);
        raw.as_mut().set_world_transform(&inv, false);
        if let Some(bg) = background {
            let (dw, dh) = ((lw as f64 * dpr).ceil() as i32 + 2, (lh as f64 * dpr).ceil() as i32 + 2);
            raw.as_mut().raw_fill_rect(0, 0, dw, dh, &bg.q());
        }
        let img = unsafe {
            QImage::from_raw_parts(page.as_ptr(), w, h, QImageFormat::Format_RGB32, noop_cleanup, std::ptr::null_mut())
        };
        raw.as_mut().draw_image_at(&QPoint::new(x, y), &img);
        raw.painter_end();
    }
}

/// Read-only file mapping: untouched pages cost nothing.
pub struct Mapped {
    ptr: *mut libc::c_void,
    len: usize,
}
impl Mapped {
    pub fn open(path: &str) -> Option<Self> {
        use std::os::fd::AsRawFd;
        let f = std::fs::File::open(path).ok()?;
        let len = f.metadata().ok()?.len() as usize;
        let ptr = unsafe {
            libc::mmap(std::ptr::null_mut(), len, libc::PROT_READ, libc::MAP_PRIVATE, f.as_raw_fd(), 0)
        };
        (ptr != libc::MAP_FAILED).then_some(Mapped { ptr, len })
    }
    pub fn bytes(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.ptr as *const u8, self.len) }
    }
}
impl Drop for Mapped {
    fn drop(&mut self) {
        unsafe { libc::munmap(self.ptr, self.len) };
    }
}

impl H<Canvas> {
    pub fn new(handler: Rc<dyn CanvasHandler>) -> Self {
        let c = new_canvas();
        unsafe { Pin::new_unchecked(&mut *c) }.rust_mut().handler = Some(handler);
        H(c)
    }
    pub fn start_timer(&self, ms: i32) -> i32 {
        pin!(self).startTimer(ms, TimerType(0)) // Qt::PreciseTimer
    }
    pub fn kill_timer(&self, id: i32) {
        pin!(self).killTimer(id)
    }
    pub fn watch(&self, window: H<QWindow>) {
        let me = unsafe { <QWidget as Upcast<QObject>>::upcast_ptr(self.widget().0) }.cast_mut();
        unsafe { pin!(window).installEventFilter(me) };
    }
}

impl H<QWidget> {
    pub fn new() -> Self {
        H(new_widget())
    }
    pub fn show(&self) { pin!(self).show() }
    pub fn update(&self) { pin!(self).update() }
    pub fn resize(&self, w: i32, h: i32) { pin!(self).resize(w, h) }
    pub fn width(&self) -> i32 { unsafe { &*self.0 }.width() }
    pub fn height(&self) -> i32 { unsafe { &*self.0 }.height() }
    pub fn set_title(&self, t: &str) { pin!(self).setWindowTitle(&qs(t)) }
    pub fn set_min_width(&self, w: i32) { pin!(self).setMinimumWidth(w) }
    pub fn set_fixed_height(&self, h: i32) { pin!(self).setFixedHeight(h) }
    pub fn set_fixed_width(&self, w: i32) { pin!(self).setFixedWidth(w) }
    pub fn set_fixed_size(&self, w: i32, h: i32) { pin!(self).set_fixed_size(w, h) }
    pub fn set_visible(&self, on: bool) { pin!(self).setVisible(on) }
    pub fn is_visible(&self) -> bool { unsafe { &*self.0 }.isVisible() }
    pub fn set_palette(&self, p: &Palette) { pin!(self).setPalette(&p.0) }
    pub fn set_autofill(&self, on: bool) { pin!(self).setAutoFillBackground(on) }
    pub fn set_style_sheet(&self, s: &str) { pin!(self).setStyleSheet(&qs(s)) }
    pub fn set_object_name(&self, n: &str) { pin!(self).setObjectName(QAnyStringView::from(n)) }
    pub fn set_styled_background(&self) { pin!(self).setAttribute(WidgetAttribute(93), true) } // WA_StyledBackground
    pub fn set_opaque_paint(&self) { pin!(self).setAttribute(WidgetAttribute(4), true) } // WA_OpaquePaintEvent
    pub fn set_no_focus(&self) { pin!(self).setFocusPolicy(FocusPolicy(0)) }
    pub fn set_margins(&self, l: i32, t: i32, r: i32, b: i32) { pin!(self).widget_set_margins(l, t, r, b) }
    pub fn set_window_icon_file(&self, path: &str) {
        let i = new_icon(&qs(path));
        pin!(self).setWindowIcon(&i);
    }
    pub fn set_tool_tip(&self, t: &str) { pin!(self).setToolTip(&qs(t)) }
    pub fn set_parent(&self, p: H<QWidget>) { unsafe { pin!(self).setParent(p.0) } }
    pub fn set_geometry(&self, x: i32, y: i32, w: i32, h: i32) { pin!(self).set_geometry(x, y, w, h) }
    pub fn raise(&self) { pin!(self).raise() }
    pub fn add_action(&self, a: H<QAction>) { unsafe { pin!(self).addAction(a.0) } }
    pub fn dpr(&self) -> f64 { unsafe { &*self.0 }.devicePixelRatioF() }
    pub fn window_handle(&self) -> Option<H<QWindow>> {
        let w = unsafe { &*self.0 }.windowHandle();
        (!w.is_null()).then_some(H(w))
    }
    /// Top-left in the top-level window's logical coordinates.
    pub fn origin_in_window(&self) -> (i32, i32) {
        let d = unsafe { &*self.0 };
        let p = unsafe { d.mapTo(d.window(), &QPoint::new(0, 0)) };
        (p.x(), p.y())
    }
    pub fn map_to_global(&self, x: i32, y: i32) -> (i32, i32) {
        let p = unsafe { &*self.0 }.mapToGlobal(&QPoint::new(x, y));
        (p.x(), p.y())
    }
    /// Scales the widget's own font relative to its point size, in logical pixels,
    /// because cxx-qt-lib's QFont doesn't bind setPointSize.
    pub fn tune_font(&self, factor: f64, bold: bool, letter_spacing: f64) {
        let mut f: QFont = unsafe { &*self.0 }.font().clone();
        let pt = f.point_size().max(1) as f64;
        f.set_pixel_size((pt * 96.0 / 72.0 * factor).round() as i32);
        f.set_bold(bold);
        if letter_spacing != 0.0 {
            f.set_letter_spacing(QFontSpacingType::AbsoluteSpacing, letter_spacing);
        }
        pin!(self).setFont(&f);
    }
    pub fn set_shadow(&self, blur: f64, dy: f64, c: Rgb, alpha: i32) {
        let e = new_shadow();
        unsafe {
            let mut p = Pin::new_unchecked(&mut *e);
            p.as_mut().setBlurRadius(blur);
            p.as_mut().setOffset(0.0, dy);
            p.as_mut().setColor(&c.qa(alpha));
            // QGraphicsDropShadowEffect derives from QGraphicsEffect by single inheritance.
            pin!(self).setGraphicsEffect(e as *mut QGraphicsEffect);
        }
    }
    /// QWidget::render into a QImage, then QImage::save.
    pub fn save_png(&self, path: &str) -> bool {
        let dpr = self.dpr();
        let (w, h) = ((self.width() as f64 * dpr).round() as i32, (self.height() as f64 * dpr).round() as i32);
        let mut img = QImage::from_width_height_and_format(w, h, QImageFormat::Format_RGB32);
        // QImage's only base is QPaintDevice, at offset zero.
        let raw = &mut img as *mut QImage as *mut RawImage;
        unsafe {
            Pin::new_unchecked(&mut *raw).image_set_dpr(dpr);
            let mut p = new_painter_on(raw as *mut QPaintDevice);
            let region = new_region();
            let rp = raw_painter(&mut p).get_unchecked_mut() as *mut RawPainter;
            pin!(self).render(rp, &QPoint::new(0, 0), &region, RenderFlags(3));
            raw_painter(&mut p).painter_end();
            (&*raw).image_save(&qs(path), std::ptr::null(), -1)
        }
    }
}

impl H<QWindow> {
    pub fn request_update(&self) { pin!(self).requestUpdate() }
    fn receiver(&self) -> *mut QObject {
        unsafe { <QWindow as Upcast<QObject>>::upcast_ptr(self.0) }.cast_mut()
    }
    /// Synthetic events go through QWidgetWindow's real hit testing and delivery.
    fn send<T: cxx::memory::UniquePtrTarget>(&self, e: &mut UniquePtr<T>) -> bool {
        let raw = e.as_mut().unwrap();
        let raw = unsafe { raw.get_unchecked_mut() as *mut T as *mut QEvent };
        unsafe { QApplication::send_event(self.receiver(), raw) }
    }
    pub fn send_hover_move(&self, x: f64, y: f64) -> bool {
        let p = QPointF::new(x, y);
        let none: MouseButtons = QFlags::from(MouseButton::NoButton);
        let mods = QFlags::from(KeyboardModifier::NoModifier);
        let mut e = new_mouse_event(EventType(EV_MOUSE_MOVE), &p, &p, MouseButton::NoButton, none, mods);
        self.send(&mut e)
    }
    pub fn send_wheel(&self, x: f64, y: f64, angle_y: i32) -> bool {
        let p = QPointF::new(x, y);
        let mut e = new_wheel_event(
            p.clone(),
            p,
            QPoint::new(0, 0),
            QPoint::new(0, angle_y),
            QFlags::from(MouseButton::NoButton),
            QFlags::from(KeyboardModifier::NoModifier),
            ScrollPhase(0),
            false,
        );
        self.send(&mut e)
    }
}

impl H<QMenu> {
    pub fn new(parent: H<QWidget>) -> Self { H(unsafe { new_menu(parent.0) }) }
    pub fn add_menu(&self, t: &str) -> H<QMenu> { H(pin!(self).menu_add_menu(&qs(t))) }
    pub fn add_action(&self, t: &str) -> H<QAction> { H(pin!(self).menu_add_action(&qs(t))) }
    pub fn add_separator(&self) { pin!(self).menu_add_separator(); }
    pub fn popup_at(&self, x: i32, y: i32) {
        unsafe { pin!(self).popup(&QPoint::new(x, y), std::ptr::null_mut()) }
    }
}
impl H<QAction> {
    pub fn on_triggered(&self, f: impl FnMut() + 'static) {
        let mut f = GuiThreadOnly(f);
        pin!(self).on_triggered(move |_, _| f.get()()).release();
    }
    pub fn trigger(&self) { pin!(self).trigger() }
    pub fn set_shortcut(&self, k: &str) {
        let seq = new_key_sequence(&qs(k), SequenceFormat(1)); // PortableText
        pin!(self).action_set_shortcut(&seq);
    }
}
impl H<QLineEdit> {
    pub fn new() -> Self { H(new_line_edit()) }
    pub fn set_text(&self, t: &str) { pin!(self).edit_set_text(&qs(t)) }
    pub fn set_placeholder(&self, t: &str) { pin!(self).setPlaceholderText(&qs(t)) }
    pub fn set_framed(&self, on: bool) { pin!(self).edit_set_frame(on) }
}
impl H<QLabel> {
    pub fn new(t: &str) -> Self {
        let l = H(new_label());
        l.set_text(t);
        l
    }
    pub fn set_text(&self, t: &str) { pin!(self).label_set_text(&qs(t)) }
}
impl H<QToolButton> {
    pub fn new() -> Self { H(new_tool_button()) }
    /// `path` is an icon file. QIcon::fromTheme can't be bound: it returns QIcon by
    /// value, which cxx only allows for relocatable types, and QIcon isn't declared so.
    pub fn set_icon_file(&self, path: &str, size: i32) {
        let i = new_icon(&qs(path));
        pin!(self).button_set_icon(&i);
        pin!(self).button_set_icon_size(&QSize::new(size, size));
    }
    pub fn set_auto_raise(&self, on: bool) { pin!(self).setAutoRaise(on) }
    pub fn set_checkable(&self, on: bool) { pin!(self).setCheckable(on) }
    pub fn click(&self) { pin!(self).click() }
    pub fn on_clicked(&self, f: impl FnMut(bool) + 'static) {
        let mut f = GuiThreadOnly(f);
        pin!(self).on_clicked(move |_, c| f.get()(c)).release();
    }
}
impl H<QTabBar> {
    pub fn new() -> Self {
        let t = H(new_tabbar());
        pin!(t).setDocumentMode(true);
        pin!(t).setTabsClosable(true);
        pin!(t).setDrawBase(false);
        pin!(t).setExpanding(false);
        pin!(t).setMovable(true);
        pin!(t).setElideMode(TextElideMode(1)); // ElideRight
        t
    }
    pub fn add_tab(&self, t: &str) -> i32 { pin!(self).addTab(&qs(t)) }
    pub fn remove_tab(&self, i: i32) { pin!(self).removeTab(i) }
    pub fn set_current(&self, i: i32) { pin!(self).setCurrentIndex(i) }
    pub fn current(&self) -> i32 { unsafe { &*self.0 }.currentIndex() }
    pub fn count(&self) -> i32 { unsafe { &*self.0 }.count() }
    pub fn text(&self, i: i32) -> String { unsafe { &*self.0 }.tabText(i).to_string() }
    pub fn on_current_changed(&self, f: impl FnMut(i32) + 'static) {
        let mut f = GuiThreadOnly(f);
        pin!(self).on_current_changed(move |_, i| f.get()(i)).release();
    }
    pub fn on_close_requested(&self, f: impl FnMut(i32) + 'static) {
        let mut f = GuiThreadOnly(f);
        pin!(self).on_tab_close_requested(move |_, i| f.get()(i)).release();
    }
}
impl H<QSplitter> {
    pub fn new_horizontal() -> Self {
        let s = H(new_splitter(Orientation(1)));
        pin!(s).setChildrenCollapsible(false);
        s
    }
    pub fn add(&self, w: H<QWidget>) { unsafe { pin!(self).splitter_add_widget(w.0) } }
    pub fn set_handle_width(&self, w: i32) { pin!(self).setHandleWidth(w) }
    pub fn set_stretch(&self, i: i32, s: i32) { pin!(self).setStretchFactor(i, s) }
    pub fn set_sizes(&self, v: &[i32]) {
        let mut l = QList::<i32>::default();
        for x in v {
            l.append(*x);
        }
        pin!(self).setSizes(&l);
    }
}
impl H<QScrollBar> {
    pub fn new_vertical() -> Self { H(new_scrollbar(Orientation(2))) }
    pub fn set_range(&self, max: i32, page: i32, single: i32) {
        pin!(self).setRange(0, max);
        pin!(self).setPageStep(page);
        pin!(self).setSingleStep(single);
    }
    pub fn set_value(&self, v: i32) { pin!(self).setValue(v) }
    pub fn value(&self) -> i32 { unsafe { &*self.0 }.value() }
    pub fn on_value_changed(&self, f: impl FnMut(i32) + 'static) {
        let mut f = GuiThreadOnly(f);
        pin!(self).on_value_changed(move |_, v| f.get()(v)).release();
    }
}
impl H<QTreeWidget> {
    /// A flat, frameless tree with no header.
    pub fn new_flat(indent: i32, expandable: bool) -> Self {
        let t = H(new_tree());
        pin!(t).setHeaderHidden(true);
        pin!(t).setFrameShape(FrameShape(0)); // NoFrame
        pin!(t).setIndentation(indent);
        pin!(t).setRootIsDecorated(expandable);
        pin!(t).setItemsExpandable(expandable);
        pin!(t).setUniformRowHeights(true);
        pin!(t).setHorizontalScrollBarPolicy(ScrollBarPolicy(1)); // AlwaysOff
        t
    }
    pub fn add_top(&self, text: &str, row_h: i32) -> H<QTreeWidgetItem> {
        let i = H(unsafe { new_tree_item(self.0) });
        i.init(text, row_h);
        i
    }
    pub fn expand_all(&self) { pin!(self).expandAll() }
    pub fn set_current(&self, i: H<QTreeWidgetItem>) { unsafe { pin!(self).setCurrentItem(i.0) } }
    pub fn viewport(&self) -> H<QWidget> { H(unsafe { &*self.0 }.tree_viewport()) }
}
impl H<QTreeWidgetItem> {
    fn init(&self, t: &str, row_h: i32) {
        unsafe {
            pin!(self).item_set_text(0, &qs(t));
            pin!(self).item_set_size_hint(0, &QSize::new(0, row_h));
        }
    }
    pub fn add_child(&self, text: &str, row_h: i32) -> H<QTreeWidgetItem> {
        let i = H(unsafe { new_child_item(self.0) });
        i.init(text, row_h);
        i
    }
    pub fn set_foreground(&self, c: Rgb) {
        let b = new_brush(&c.q(), BrushStyle(1)); // SolidPattern
        unsafe { pin!(self).item_set_foreground(0, &b) }
    }
    pub fn set_font_from(&self, w: H<QWidget>, bold: bool) {
        let mut f: QFont = unsafe { &*w.0 }.font().clone();
        f.set_bold(bold);
        unsafe { pin!(self).item_set_font(0, &f) }
    }
}

pub struct VBox(*mut QVBoxLayout);
impl VBox {
    pub fn on(w: H<QWidget>, m: (i32, i32, i32, i32), spacing: i32) -> Self {
        let l = unsafe { new_vbox(w.0) };
        let mut p = unsafe { Pin::new_unchecked(&mut *l) };
        p.as_mut().vbox_margins(m.0, m.1, m.2, m.3);
        p.as_mut().vbox_spacing(spacing);
        VBox(l)
    }
    pub fn add(&self, w: H<QWidget>, stretch: i32) {
        unsafe { Pin::new_unchecked(&mut *self.0).vbox_add(w.0, stretch, Alignment(0)) }
    }
}
pub struct HBox(*mut QHBoxLayout);
impl HBox {
    pub fn on(w: H<QWidget>, m: (i32, i32, i32, i32), spacing: i32) -> Self {
        let l = unsafe { new_hbox(w.0) };
        let mut p = unsafe { Pin::new_unchecked(&mut *l) };
        p.as_mut().hbox_margins(m.0, m.1, m.2, m.3);
        p.as_mut().hbox_spacing(spacing);
        HBox(l)
    }
    pub fn add(&self, w: H<QWidget>, stretch: i32) {
        unsafe { Pin::new_unchecked(&mut *self.0).hbox_add(w.0, stretch, Alignment(0)) }
    }
    pub fn stretch(&self) {
        unsafe { Pin::new_unchecked(&mut *self.0).hbox_stretch(1) }
    }
}

/// Creates QApplication and runs the event loop. Args are leaked since Qt keeps them.
pub fn run_app(setup: impl FnOnce()) -> i32 {
    let argc = Box::leak(Box::new(1i32));
    let arg0 = Box::leak(Box::new(*b"mdview\0"));
    let argv = Box::leak(Box::new([arg0.as_mut_ptr() as *mut c_char, std::ptr::null_mut()]));
    let mut app = unsafe { new_app(argc, argv.as_mut_ptr()) };
    let app_raw = unsafe { app.as_mut().unwrap().get_unchecked_mut() as *mut QApplication };
    APP.store(app_raw, std::sync::atomic::Ordering::Relaxed);
    setup();
    QApplication::exec()
}

/// A stylesheet on the whole application, for the launch cost comparison.
pub fn set_app_style_sheet(s: &str) {
    let app = APP.load(std::sync::atomic::Ordering::Relaxed);
    if !app.is_null() {
        unsafe { Pin::new_unchecked(&mut *app) }.app_set_style_sheet(&qs(s));
    }
}
static APP: std::sync::atomic::AtomicPtr<QApplication> = std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());

pub fn process_events() {
    QApplication::process_events(ProcessEventsFlags(0));
}

pub fn monotonic_ns() -> u64 {
    let mut t = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut t) };
    t.tv_sec as u64 * 1_000_000_000 + t.tv_nsec as u64
}

pub fn boottime_ns() -> u64 {
    let mut t = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    unsafe { libc::clock_gettime(libc::CLOCK_BOOTTIME, &mut t) };
    t.tv_sec as u64 * 1_000_000_000 + t.tv_nsec as u64
}

/// Exit at once, skipping destructors. stdout must already be flushed.
pub fn fast_exit(code: i32) -> ! {
    unsafe { libc::_exit(code) }
}
