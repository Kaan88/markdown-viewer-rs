//! The only module allowed to contain `unsafe`. `crate::shell` forbids it.
//!
//! `H<T>` is a copyable raw pointer to a widget owned by Qt's parent tree. It is only handed out for
//! widgets that live until the process exits (the window and everything parented to it, which this
//! shell never deletes), which is what keeps every copy valid. Objects Qt deletes earlier, the
//! outline's tree items, are never exposed as handles: see `Tree`.
pub mod ffi;

use cxx::UniquePtr;
use cxx_qt::{casting::Upcast, CxxQtType};
use cxx_qt_lib::{
    KeyboardModifier, MouseButton, MouseButtons, QColor, QFlags, QImage, QImageFormat, QPoint,
    QPointF, QRectF, QString, QUrl,
};
use ffi::*;
use std::os::raw::c_char;
use std::pin::Pin;
use std::rc::Rc;

macro_rules! trivial_cxx {
    ($name:ident, $id:literal) => {
        #[repr(transparent)]
        #[derive(Clone, Copy)]
        pub struct $name(pub u32);
        unsafe impl cxx::ExternType for $name {
            type Id = cxx::type_id!($id);
            type Kind = cxx::kind::Trivial;
        }
    };
}
trivial_cxx!(DropActions, "Qt::DropActions");
trivial_cxx!(ScrollPhase, "Qt::ScrollPhase");
trivial_cxx!(EventType, "QEvent::Type");
trivial_cxx!(TimerType, "Qt::TimerType");
trivial_cxx!(Alignment, "Qt::Alignment");
trivial_cxx!(Orientation, "Qt::Orientation");
trivial_cxx!(FocusPolicy, "Qt::FocusPolicy");
trivial_cxx!(CursorShape, "Qt::CursorShape");
trivial_cxx!(ClipboardMode, "QClipboard::Mode");
/// `QClipboard::Clipboard`, the system clipboard (not the X11 selection).
const CLIPBOARD: ClipboardMode = ClipboardMode(0);

#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct QSizeType(pub i64);
unsafe impl cxx::ExternType for QSizeType {
    type Id = cxx::type_id!("qsizetype");
    type Kind = cxx::kind::Trivial;
}

pub const EV_MOUSE_PRESS: u32 = 2;
pub const EV_MOUSE_RELEASE: u32 = 3;
pub const EV_MOUSE_MOVE: u32 = 5;
pub const EV_UPDATE_REQUEST: u32 = 77;
const DROP_COPY: u32 = 1;
const SCROLL_NONE: u32 = 0; // Qt::NoScrollPhase
const QINTERNAL_IMAGE: i32 = 3;

/// cxx-qt requires signal closures to be `Send`, though direct connections on
/// objects of the GUI thread only ever run on that thread.
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
impl<T> H<T> {
    pub fn raw(&self) -> *mut T {
        self.0
    }
}

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
upcast_widget!(
    Canvas,
    QMainWindow,
    QLineEdit,
    QTabWidget,
    QDockWidget,
    QTreeWidget,
    QScrollBar,
    QToolBar
);

fn qs(s: &str) -> QString {
    QString::from(s)
}

/// Copies text to the system clipboard.
pub fn set_clipboard(text: &str) {
    let clipboard = QApplication::clipboard();
    if clipboard.is_null() {
        return;
    }
    // Owned by the application object, which outlives every caller.
    unsafe { Pin::new_unchecked(&mut *clipboard) }.clipboard_set_text(&qs(text), CLIPBOARD);
}

pub fn read_clipboard() -> String {
    let clipboard = QApplication::clipboard();
    if clipboard.is_null() {
        return String::new();
    }
    unsafe { &*clipboard }.clipboard_get_text(CLIPBOARD).to_string()
}

/// Events the shell can observe on the canvas. All safe Rust on the other side.
pub trait CanvasHandler {
    fn paint(&self, target: &mut PaintTarget<'_>);
    fn mouse_press(&self, x: f64, y: f64);
    fn mouse_move(&self, x: f64, y: f64);
    fn mouse_release(&self, x: f64, y: f64);
    fn wheel(&self, dx: i32, dy: i32);
    fn key(&self, code: i32);
    /// Ctrl+F: show the find bar.
    fn find(&self) {}
    /// F3 and Shift+F3: step through matches.
    fn find_step(&self, _delta: i32) {}
    /// Ctrl+C: copy the selection.
    fn copy(&self) {}
    fn drag_enter(&self, text: String) -> bool;
    fn dropped(&self, text: String);
    fn timer(&self, id: i32);
    fn update_request(&self);
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
    unsafe fn mouse_press_event(self: Pin<&mut Self>, e: *mut QMouseEvent) {
        let p = (*e).mouse_pos();
        self.handler().mouse_press(p.x(), p.y());
    }
    unsafe fn mouse_move_event(self: Pin<&mut Self>, e: *mut QMouseEvent) {
        let p = (*e).mouse_pos();
        self.handler().mouse_move(p.x(), p.y());
    }
    unsafe fn mouse_release_event(self: Pin<&mut Self>, e: *mut QMouseEvent) {
        let p = (*e).mouse_pos();
        self.handler().mouse_release(p.x(), p.y());
    }
    unsafe fn wheel_event(self: Pin<&mut Self>, e: *mut QWheelEvent) {
        let a = (*e).angleDelta();
        self.handler().wheel(a.x(), a.y());
        self.base_wheel_event(e); // proves the base class is callable from an override
    }
    unsafe fn drag_enter_event(self: Pin<&mut Self>, e: *mut QDragEnterEvent) {
        let text = (*(*e).drag_mime_data()).text().to_string();
        if self.handler().drag_enter(text) {
            Pin::new_unchecked(&mut *e).acceptProposedAction();
        }
    }
    unsafe fn key_press_event(self: Pin<&mut Self>, e: *mut QKeyEvent) {
        let modifiers = (*e).modifiers();
        let code = (*e).key_code();
        if modifiers.test_flag(KeyboardModifier::ControlModifier) && matches!(code, 0x43 | 0x63) {
            self.handler().copy();
            return;
        }
        if modifiers.test_flag(KeyboardModifier::ControlModifier) && matches!(code, 0x46 | 0x66) {
            self.handler().find();
            return;
        }
        if code == 0x0100_0032 {
            let step = if modifiers.test_flag(KeyboardModifier::ShiftModifier) {
                -1
            } else {
                1
            };
            self.handler().find_step(step);
            return;
        }
        self.handler().key(code);
    }
    unsafe fn drop_event(self: Pin<&mut Self>, e: *mut QDropEvent) {
        let text = (*(*e).mimeData()).text().to_string();
        self.handler().dropped(text);
        Pin::new_unchecked(&mut *e).drop_accept();
    }
    unsafe fn timer_event(self: Pin<&mut Self>, e: *mut QTimerEvent) {
        self.handler().timer((*e).timerId());
    }
    unsafe fn event_filter(self: Pin<&mut Self>, _w: *mut QObject, e: *mut QEvent) -> bool {
        if (*e).event_type().0 == EV_UPDATE_REQUEST {
            self.handler().update_request();
        }
        false
    }
}

extern "C" fn noop_cleanup(_: *mut cxx_qt_lib::c_void) {}

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
    /// Blits BGRX through QPainter with a net identity transform, so the raster
    /// engine's integer-translate blit path is used rather than a scaler.
    /// `exact = false` is the G2 negative control: the same draw left under Qt's
    /// device scale, so the raster engine resamples it.
    pub fn blit_painter(&mut self, buf: &[u8], w: i32, h: i32, exact: bool) {
        assert!(buf.len() >= (w * h * 4) as usize);
        let dpr = self.dpr();
        let canvas = unsafe { self.canvas.as_mut().get_unchecked_mut() as *mut Canvas };
        let mut p: UniquePtr<QPainter> = unsafe { new_painter(canvas) };
        let inv = new_transform(1.0 / dpr, 0.0, 0.0, 1.0 / dpr, 0.0, 0.0);
        // cxx-qt-lib owns QPainter's cxx identity; RawPainter names the same C++ class
        // so this crate can bind the members cxx-qt-lib lacks.
        let raw = unsafe {
            Pin::new_unchecked(
                &mut *(p.as_mut().unwrap().get_unchecked_mut() as *mut _ as *mut RawPainter),
            )
        };
        let mut raw = raw;
        if exact {
            raw.as_mut().set_world_transform(&inv, false);
        }
        let img = unsafe {
            QImage::from_raw_parts(
                buf.as_ptr(),
                w,
                h,
                QImageFormat::Format_RGB32,
                noop_cleanup,
                std::ptr::null_mut(),
            )
        };
        raw.as_mut().draw_image_at(&QPoint::new(0, 0), &img);
        raw.painter_end();
    }
    /// Fills page rectangles (x, y, w, h in device px) through QPainter with a translucent colour,
    /// on top of whatever was blitted. The scroll offset is subtracted here.
    pub fn fill_rects(&mut self, rects: &[(f64, f64, f64, f64)], scroll: f64, color: [u8; 4]) {
        if rects.is_empty() {
            return;
        }
        let dpr = self.dpr();
        let canvas = unsafe { self.canvas.as_mut().get_unchecked_mut() as *mut Canvas };
        let mut p: UniquePtr<QPainter> = unsafe { new_painter(canvas) };
        let inv = new_transform(1.0 / dpr, 0.0, 0.0, 1.0 / dpr, 0.0, 0.0);
        let raw = unsafe {
            Pin::new_unchecked(
                &mut *(p.as_mut().unwrap().get_unchecked_mut() as *mut _ as *mut RawPainter),
            )
        };
        let mut raw = raw;
        raw.as_mut().set_world_transform(&inv, false);
        let c = QColor::from_rgba_f(
            color[0] as f32 / 255.0,
            color[1] as f32 / 255.0,
            color[2] as f32 / 255.0,
            color[3] as f32 / 255.0,
        );
        for (x, y, w, h) in rects {
            let r = QRectF::new(*x, *y - scroll, *w, *h);
            raw.as_mut().fill_rect_raw(&r, &c);
        }
        raw.painter_end();
    }
    /// Copies BGRX straight into the backing store's own QImage, in device pixels.
    /// Returns false if the backing store is not a 32 bit QImage.
    pub fn blit_direct(&mut self, buf: &[u8], w: i32, h: i32) -> bool {
        let dpr = self.dpr();
        let widget = self.as_widget();
        let win = widget.window();
        let origin = unsafe { widget.mapTo(win, &QPoint::new(0, 0)) };
        let (lw, lh) = self.logical_size();
        let store = widget.backingStore();
        if store.is_null() {
            return false;
        }
        unsafe {
            let dev = Pin::new_unchecked(&mut *store).paintDevice();
            if dev.is_null() || (*dev).devType() != QINTERNAL_IMAGE {
                return false;
            }
            // QImage's only base is QPaintDevice, at offset zero.
            let img = &*(dev as *const RawImage);
            if img.raw_depth() != 32 {
                return false;
            }
            let bits = img.constBits() as *mut u8; // no detach
            let bpl = img.bytesPerLine().0 as usize;
            let (iw, ih) = (img.raw_width(), img.raw_height());
            let x0 = (origin.x() as f64 * dpr).round() as i32;
            let y0 = (origin.y() as f64 * dpr).round() as i32;
            let x1 = (((origin.x() + lw) as f64 * dpr).round() as i32).min(iw);
            let y1 = (((origin.y() + lh) as f64 * dpr).round() as i32).min(ih);
            let row_px = (x1 - x0).min(w).max(0) as usize;
            for y in y0..y1.min(y0 + h) {
                let src = buf.as_ptr().add(((y - y0) * w * 4) as usize);
                let dst = bits.add(y as usize * bpl + x0 as usize * 4);
                std::ptr::copy_nonoverlapping(src, dst, row_px * 4);
            }
        }
        true
    }
}

impl H<Canvas> {
    pub fn new(handler: Rc<dyn CanvasHandler>) -> Self {
        let c = new_canvas();
        unsafe { Pin::new_unchecked(&mut *c) }.rust_mut().handler = Some(handler);
        H(c)
    }
    pub fn set_cursor_shape(&self, shape: u32) {
        self.widget().set_cursor_shape(shape)
    }
    /// Sends a mouse event straight to the canvas, in canvas logical coordinates. Unlike a
    /// synthetic event through the QWindow, this reaches `mouseReleaseEvent` reliably: Qt only
    /// keeps the press's implicit grab for events it knows came from the platform.
    pub fn send_mouse(&self, kind: u32, x: f64, y: f64) -> bool {
        let p = QPointF::new(x, y);
        let (button, buttons) = if kind == EV_MOUSE_MOVE {
            (
                MouseButton::NoButton,
                MouseButtons::from(MouseButton::LeftButton),
            )
        } else if kind == EV_MOUSE_PRESS {
            (
                MouseButton::LeftButton,
                MouseButtons::from(MouseButton::LeftButton),
            )
        } else {
            (MouseButton::LeftButton, QFlags::from(MouseButton::NoButton))
        };
        let mods = QFlags::from(KeyboardModifier::NoModifier);
        let mut e = new_mouse_event(EventType(kind), &p, &p, button, buttons, mods);
        let raw = e.as_mut().unwrap();
        let raw = unsafe { raw.get_unchecked_mut() as *mut QMouseEvent as *mut QEvent };
        let receiver =
            unsafe { <QWidget as Upcast<QObject>>::upcast_ptr(self.widget().0) }.cast_mut();
        unsafe { QApplication::send_event(receiver, raw) }
    }
    pub fn start_timer(&self, ms: i32) -> i32 {
        pin!(self).startTimer(ms, TimerType(0)) // Qt::PreciseTimer
    }
    pub fn kill_timer(&self, id: i32) {
        pin!(self).killTimer(id)
    }
    /// Device origin of the canvas in the window, unrounded, for reporting.
    pub fn describe(&self) -> String {
        let w = self.widget();
        let d = unsafe { &*w.0 };
        let dpr = d.devicePixelRatioF();
        let win = d.window();
        let o = unsafe { d.mapTo(win, &QPoint::new(0, 0)) };
        let wn = unsafe { &*win };
        format!(
            "dpr={dpr} canvas_in_window={},{} canvas_logical={}x{} canvas_device_origin={:.2},{:.2} canvas_device_size={:.2}x{:.2} window_logical={}x{}",
            o.x(), o.y(), d.width(), d.height(),
            o.x() as f64 * dpr, o.y() as f64 * dpr,
            d.width() as f64 * dpr, d.height() as f64 * dpr,
            wn.width(), wn.height(),
        )
    }
    /// Filter window events through this canvas's eventFilter override.
    pub fn watch(&self, window: H<QWindow>) {
        let me = unsafe { <QWidget as Upcast<QObject>>::upcast_ptr(self.widget().0) }.cast_mut();
        unsafe { pin!(window).installEventFilter(me) };
    }
}

impl H<QWidget> {
    pub fn show(&self) {
        pin!(self).show()
    }
    pub fn set_visible(&self, visible: bool) {
        pin!(self).set_visible(visible)
    }
    pub fn is_visible(&self) -> bool {
        pin!(self).is_visible()
    }
    pub fn update(&self) {
        pin!(self).update()
    }
    pub fn repaint(&self) {
        pin!(self).repaint()
    }
    pub fn resize(&self, w: i32, h: i32) {
        pin!(self).resize(w, h)
    }
    pub fn set_title(&self, t: &str) {
        pin!(self).setWindowTitle(&qs(t))
    }
    pub fn accept_drops(&self) {
        pin!(self).setAcceptDrops(true)
    }
    pub fn track_mouse(&self) {
        pin!(self).setMouseTracking(true)
    }
    pub fn set_focus_policy(&self, p: u32) {
        pin!(self).setFocusPolicy(FocusPolicy(p))
    }
    pub fn set_focus(&self) {
        pin!(self).setFocus()
    }
    pub fn set_cursor_shape(&self, shape: u32) {
        let cursor = new_cursor(CursorShape(shape));
        pin!(self).setCursor(cursor.as_ref().expect("cursor"))
    }
    pub fn fix_width(&self, w: i32) {
        pin!(self).setMinimumWidth(w);
        pin!(self).setMaximumWidth(w);
    }
    pub fn width(&self) -> i32 {
        unsafe { &*self.0 }.width()
    }
    pub fn dpr(&self) -> f64 {
        unsafe { &*self.0 }.devicePixelRatioF()
    }
    pub fn window_handle(&self) -> Option<H<QWindow>> {
        let w = unsafe { &*self.0 }.windowHandle();
        (!w.is_null()).then_some(H(w))
    }
    pub fn center_in_window(&self) -> (f64, f64) {
        let d = unsafe { &*self.0 };
        let win = d.window();
        let o = unsafe { d.mapTo(win, &QPoint::new(d.width() / 2, d.height() / 2)) };
        (o.x() as f64, o.y() as f64)
    }
}

impl H<QScrollBar> {
    /// Qt::Vertical
    pub fn vertical(parent: H<QWidget>) -> Self {
        H(unsafe { new_scroll_bar(Orientation(2), parent.0) })
    }
    pub fn set_range(&self, min: i32, max: i32) {
        pin!(self).setRange(min, max)
    }
    pub fn set_value(&self, v: i32) {
        pin!(self).setValue(v)
    }
    pub fn value(&self) -> i32 {
        unsafe { &*self.0 }.value()
    }
    pub fn set_page_step(&self, step: i32) {
        pin!(self).setPageStep(step)
    }
    pub fn set_single_step(&self, step: i32) {
        pin!(self).setSingleStep(step)
    }
    pub fn on_value_changed(&self, f: impl FnMut(i32) + 'static) {
        let mut f = GuiThreadOnly(f);
        pin!(self)
            .on_value_changed(move |_, v| f.get()(v))
            .release();
    }
}

/// Opens an http(s) or mailto link in the platform's handler. False when Qt could not start one.
pub fn open_url(url: &str) -> bool {
    let parsed = QUrl::from_user_input(&qs(url), &qs(""));
    QDesktopServices::open_url(&parsed)
}

/// The system file chooser, starting in `dir`. `None` when the user cancels.
pub fn choose_file(parent: H<QWidget>, dir: &std::path::Path) -> Option<std::path::PathBuf> {
    let caption = qs("Open Markdown file");
    let start = qs(&dir.display().to_string());
    let filter = qs("Markdown (*.md *.markdown);;All files (*)");
    let dialog = H(unsafe { new_file_dialog(parent.0, &caption, &start, &filter) });
    if dialog.0.is_null() {
        return None;
    }
    if pin!(dialog).exec_dialog() == 0 {
        return None;
    }
    let files = unsafe { &*dialog.0 }.selected_files();
    let first = files.get(0)?;
    let chosen = first.to_string();
    (!chosen.is_empty()).then(|| std::path::PathBuf::from(chosen))
}

/// Gives `w` a horizontal layout with `left` taking the space and `right` beside it.
pub fn layout_row(w: H<QWidget>, left: H<QWidget>, right: H<QWidget>) {
    let l = unsafe { new_hbox(w.0) };
    let mut p = unsafe { Pin::new_unchecked(&mut *l) };
    p.as_mut().hbox_set_margins(0, 0, 0, 0);
    p.as_mut().hbox_set_spacing(0);
    unsafe { p.as_mut().hbox_add_widget(left.0, 1, Alignment(0)) };
    unsafe { p.as_mut().hbox_add_widget(right.0, 0, Alignment(0)) };
}

pub fn new_plain_widget() -> H<QWidget> {
    H(new_widget())
}

impl H<QWindow> {
    pub fn request_update(&self) {
        pin!(self).requestUpdate()
    }
    fn receiver(&self) -> *mut QObject {
        unsafe { <QWindow as Upcast<QObject>>::upcast_ptr(self.0) }.cast_mut()
    }
    /// Sends a synthetic event to the QWindow, so it goes through QWidgetWindow's
    /// real hit testing and delivery rather than straight to the widget.
    fn send<T: cxx::memory::UniquePtrTarget>(&self, e: &mut UniquePtr<T>) -> bool {
        // Every Qt event class here derives from QEvent by single inheritance.
        let raw = e.as_mut().unwrap();
        let raw = unsafe { raw.get_unchecked_mut() as *mut T as *mut QEvent };
        unsafe { QApplication::send_event(self.receiver(), raw) }
    }
    pub fn send_mouse(&self, kind: u32, x: f64, y: f64) -> bool {
        let p = QPointF::new(x, y);
        let (button, buttons) = if kind == EV_MOUSE_MOVE {
            (
                MouseButton::NoButton,
                MouseButtons::from(MouseButton::LeftButton),
            )
        } else if kind == EV_MOUSE_PRESS {
            (
                MouseButton::LeftButton,
                MouseButtons::from(MouseButton::LeftButton),
            )
        } else {
            (MouseButton::LeftButton, QFlags::from(MouseButton::NoButton))
        };
        let mods = QFlags::from(KeyboardModifier::NoModifier);
        let mut e = new_mouse_event(EventType(kind), &p, &p, button, buttons, mods);
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
            ScrollPhase(SCROLL_NONE),
            false,
        );
        self.send(&mut e)
    }
    pub fn send_drag_and_drop(&self, x: f64, y: f64, text: &str) -> (bool, bool) {
        let mime = new_mime();
        unsafe {
            Pin::new_unchecked(&mut *mime).mime_set_text(&qs(text));
            let nobutton = || QFlags::from(MouseButton::NoButton);
            let nomods = || QFlags::from(KeyboardModifier::NoModifier);
            let mut enter = new_drag_enter_event(
                QPoint::new(x as i32, y as i32),
                DropActions(DROP_COPY),
                mime,
                nobutton(),
                nomods(),
            );
            let a = self.send(&mut enter);
            let mut drop = new_drop_event(
                QPointF::new(x, y),
                DropActions(DROP_COPY),
                mime,
                nobutton(),
                nomods(),
            );
            let b = self.send(&mut drop);
            (a, b)
        }
    }
}

impl H<QMainWindow> {
    pub fn new() -> Self {
        H(new_main_window())
    }
    pub fn set_central(&self, w: H<QWidget>) {
        unsafe { pin!(self).setCentralWidget(w.0) }
    }
    pub fn menu_bar(&self) -> H<QMenuBar> {
        H(unsafe { &*self.0 }.menuBar())
    }
    pub fn status_bar(&self) -> H<QStatusBar> {
        H(unsafe { &*self.0 }.statusBar())
    }
    pub fn add_tool_bar(&self, t: &str) -> H<QToolBar> {
        H(pin!(self).addToolBar(&qs(t)))
    }
    pub fn add_dock(&self, left: bool, d: H<QDockWidget>) {
        let a = if left {
            DockWidgetArea::LeftDockWidgetArea
        } else {
            DockWidgetArea::RightDockWidgetArea
        };
        unsafe { pin!(self).addDockWidget(a, d.0) }
    }
}
impl H<QMenuBar> {
    pub fn add_menu(&self, t: &str) -> H<QMenu> {
        H(pin!(self).addMenu(&qs(t)))
    }
}
impl H<QMenu> {
    pub fn add_action(&self, t: &str) -> H<QAction> {
        H(pin!(self).addAction(&qs(t)))
    }
}
impl H<QAction> {
    pub fn on_triggered(&self, f: impl FnMut(bool) + 'static) {
        let mut f = GuiThreadOnly(f);
        pin!(self).on_triggered(move |_, c| f.get()(c)).release();
    }
    pub fn trigger(&self) {
        pin!(self).trigger()
    }
}
impl H<QToolBar> {
    pub fn add_widget(&self, w: H<QWidget>) {
        unsafe { pin!(self).addWidget(w.0) };
    }
    pub fn add_action(&self, t: &str) -> H<QAction> {
        H(pin!(self).add_toolbar_action(&qs(t)))
    }
}
impl H<QStatusBar> {
    pub fn message(&self, t: &str) {
        pin!(self).showMessage(&qs(t), 0)
    }
}
impl H<QLineEdit> {
    pub fn new() -> Self {
        H(new_line_edit())
    }
    pub fn set_text(&self, t: &str) {
        pin!(self).setText(&qs(t))
    }
    pub fn set_placeholder(&self, t: &str) {
        pin!(self).set_placeholder(&qs(t))
    }
    pub fn select_all(&self) {
        pin!(self).select_all()
    }
    pub fn on_return_pressed(&self, f: impl FnMut() + 'static) {
        let mut f = GuiThreadOnly(f);
        pin!(self).on_return_pressed(move |_| f.get()()).release();
    }
    pub fn on_text_changed(&self, f: impl FnMut(String) + 'static) {
        let mut f = GuiThreadOnly(f);
        pin!(self)
            .on_text_changed(move |_, t| f.get()(t.to_string()))
            .release();
    }
}
impl H<QTabWidget> {
    pub fn new() -> Self {
        H(new_tabs())
    }
    pub fn add_tab(&self, w: H<QWidget>, label: &str) {
        unsafe { pin!(self).addTab(w.0, &qs(label)) };
    }
}
impl H<QDockWidget> {
    pub fn new(title: &str, parent: H<QMainWindow>) -> Self {
        let p = parent.widget().0;
        H(unsafe { new_dock(&qs(title), p) })
    }
    pub fn set_widget(&self, w: H<QWidget>) {
        unsafe { pin!(self).setWidget(w.0) }
    }
}
/// The outline tree. Its items are deleted by `QTreeWidget::clear`, so no handle to an item ever
/// leaves this module: the items are rebuilt whole from (level, title) pairs, and a click is
/// reported as the item's index in that list. The pointers below are only compared, never followed,
/// after a `clear`, because `set_items` replaces the list in the same call.
#[derive(Clone)]
pub struct Tree {
    handle: H<QTreeWidget>,
    items: Rc<std::cell::RefCell<Vec<*mut QTreeWidgetItem>>>,
}
impl Tree {
    pub fn new() -> Self {
        let handle = H(new_tree());
        pin!(handle).setHeaderHidden(true);
        Tree {
            handle,
            items: Rc::default(),
        }
    }
    pub fn widget(&self) -> H<QWidget> {
        self.handle.widget()
    }
    /// Replaces every item. An entry nests under the closest earlier entry with a lower level,
    /// the way headings nest; the tree is then fully expanded.
    pub fn set_items(&self, entries: &[(u8, &str)]) {
        let mut items = self.items.borrow_mut();
        pin!(self.handle).clear_tree();
        items.clear();
        let mut stack: Vec<(u8, *mut QTreeWidgetItem)> = Vec::new();
        for &(level, title) in entries {
            while stack.last().is_some_and(|&(parent, _)| parent >= level) {
                stack.pop();
            }
            // Both constructors parent the new item, so the tree owns it until the next clear.
            let item = match stack.last() {
                Some(&(_, parent)) => unsafe { new_child_item(parent) },
                None => unsafe { new_tree_item(self.handle.0) },
            };
            unsafe { Pin::new_unchecked(&mut *item).item_set_text(0, &qs(title)) };
            stack.push((level, item));
            items.push(item);
        }
        pin!(self.handle).expand_tree();
    }
    pub fn len(&self) -> usize {
        self.items.borrow().len()
    }
    /// `f` gets the clicked item's index in the last `set_items` list.
    pub fn on_item_clicked(&self, f: impl FnMut(usize) + 'static) {
        let mut state = GuiThreadOnly((Rc::downgrade(&self.items), f));
        pin!(self.handle)
            .on_item_clicked(move |_, item, _| {
                let (items, f) = state.get();
                let index = items
                    .upgrade()
                    .and_then(|items| items.borrow().iter().position(|&known| known == item));
                if let Some(index) = index {
                    f(index);
                }
            })
            .release();
    }
}

/// Gives `w` a layout placing `child` at an exact logical offset.
pub fn layout_with_margins(w: H<QWidget>, child: H<QWidget>, left: i32, top: i32) {
    let l = unsafe { new_vbox(w.0) };
    let mut p = unsafe { Pin::new_unchecked(&mut *l) };
    p.as_mut().setContentsMargins(left, top, 0, 0);
    p.as_mut().setSpacing(0);
    unsafe { p.layout_add_widget(child.0, 1, Alignment(0)) };
}

/// Creates QApplication and runs the event loop. Args are leaked since Qt keeps them.
pub fn run_app(setup: impl FnOnce()) -> i32 {
    let argc = Box::leak(Box::new(1i32));
    let arg0 = Box::leak(Box::new(*b"mdview\0"));
    let argv = Box::leak(Box::new([
        arg0.as_mut_ptr() as *mut c_char,
        std::ptr::null_mut(),
    ]));
    let _app = unsafe { new_app(argc, argv.as_mut_ptr()) };
    setup();
    QApplication::exec()
}
