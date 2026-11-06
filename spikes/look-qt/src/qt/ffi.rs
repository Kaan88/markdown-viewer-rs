// The CXX-Qt bridge for the look spike. Declarations only: the C++ glue is generated
// at build time and none is authored.
#![allow(non_snake_case, clippy::too_many_arguments)]

#[cxx_qt::bridge]
pub mod ffi {
    #[namespace = "Qt"]
    unsafe extern "C++" {
        include!("cxx-qt-lib/qt.h");
        type TimerType = crate::qt::TimerType;
        type Alignment = crate::qt::Alignment;
        type Orientation = crate::qt::Orientation;
        type FocusPolicy = crate::qt::FocusPolicy;
        type ScrollBarPolicy = crate::qt::ScrollBarPolicy;
        type WidgetAttribute = crate::qt::WidgetAttribute;
        type BrushStyle = crate::qt::BrushStyle;
        type TextElideMode = crate::qt::TextElideMode;
        type ShortcutContext = crate::qt::ShortcutContext;
        type MouseButton = cxx_qt_lib::MouseButton;
        type MouseButtons = cxx_qt_lib::MouseButtons;
        type KeyboardModifiers = cxx_qt_lib::KeyboardModifiers;
        type ScrollPhase = crate::qt::ScrollPhase;
    }
    #[namespace = "QEvent"]
    unsafe extern "C++" {
        #[cxx_name = "Type"]
        type EventType = crate::qt::EventType;
    }
    #[namespace = "QPalette"]
    unsafe extern "C++" {
        type ColorGroup = crate::qt::ColorGroup;
        type ColorRole = crate::qt::ColorRole;
    }
    #[namespace = "QFrame"]
    unsafe extern "C++" {
        #[cxx_name = "Shape"]
        type FrameShape = crate::qt::FrameShape;
    }
    #[namespace = "QKeySequence"]
    unsafe extern "C++" {
        type SequenceFormat = crate::qt::SequenceFormat;
    }
    #[namespace = "QEventLoop"]
    unsafe extern "C++" {
        type ProcessEventsFlags = crate::qt::ProcessEventsFlags;
    }
    #[namespace = "QWidget"]
    unsafe extern "C++" {
        type RenderFlags = crate::qt::RenderFlags;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib/common.h");
        include!("cxx-qt-lib/qstring.h");
        include!("cxx-qt-lib/qanystringview.h");
        include!("cxx-qt-lib/qimage.h");
        include!("cxx-qt-lib/qpoint.h");
        include!("cxx-qt-lib/qpointf.h");
        include!("cxx-qt-lib/qsize.h");
        include!("cxx-qt-lib/qcolor.h");
        include!("cxx-qt-lib/qfont.h");
        include!("cxx-qt-lib/qlist.h");
        type QString = cxx_qt_lib::QString;
        type QAnyStringView<'a> = cxx_qt_lib::QAnyStringView<'a>;
        type QImage = cxx_qt_lib::QImage;
        type QPoint = cxx_qt_lib::QPoint;
        type QPointF = cxx_qt_lib::QPointF;
        type QSize = cxx_qt_lib::QSize;
        type QColor = cxx_qt_lib::QColor;
        type QFont = cxx_qt_lib::QFont;
        type QList_i32 = cxx_qt_lib::QList<i32>;

        include!(<QtGui/QPainter>);
        include!(<QtGui/QTransform>);
        include!(<QtGui/QPaintEvent>);
        include!(<QtGui/QResizeEvent>);
        include!(<QtGui/QMouseEvent>);
        include!(<QtGui/QWheelEvent>);
        include!(<QtGui/QPalette>);
        include!(<QtGui/QIcon>);
        include!(<QtGui/QBrush>);
        include!(<QtGui/QRegion>);
        include!(<QtGui/QKeySequence>);
        include!(<QtCore/QTimerEvent>);
        include!(<QtCore/QEvent>);
        type QPaintEvent;
        type QResizeEvent;
        type QMouseEvent;
        type QWheelEvent;
        type QTimerEvent;
        type QEvent;
        type QPainter = cxx_qt_lib::QPainter;
        #[cxx_name = "QPainter"]
        type RawPainter;
        type QTransform;
        type QPaintDevice;
        #[cxx_name = "QImage"]
        type RawImage;
        type QTreeWidgetItem;
        type QPalette;
        type QIcon;
        type QBrush;
        type QRegion;
        type QKeySequence;
        type QGraphicsEffect;
    }

    unsafe extern "C++Qt" {
        include!(<QtWidgets/QApplication>);
        include!(<QtWidgets/QStyle>);
        include!(<QtWidgets/QMenu>);
        include!(<QtWidgets/QLineEdit>);
        include!(<QtWidgets/QLabel>);
        include!(<QtWidgets/QToolButton>);
        include!(<QtWidgets/QTabBar>);
        include!(<QtWidgets/QSplitter>);
        include!(<QtWidgets/QScrollBar>);
        include!(<QtWidgets/QTreeWidget>);
        include!(<QtWidgets/QVBoxLayout>);
        include!(<QtWidgets/QHBoxLayout>);
        include!(<QtWidgets/QGraphicsDropShadowEffect>);
        include!(<QtGui/QAction>);
        include!(<QtGui/QWindow>);

        #[qobject]
        type QApplication;
        #[qobject]
        type QStyle;
        #[qobject]
        type QWindow;
        #[qobject]
        type QWidget;
        #[qobject]
        #[base = QWidget]
        type QMenu;
        #[qobject]
        #[base = QWidget]
        type QLineEdit;
        #[qobject]
        #[base = QWidget]
        type QLabel;
        #[qobject]
        #[base = QWidget]
        type QToolButton;
        #[qobject]
        #[base = QWidget]
        type QTabBar;
        #[qobject]
        #[base = QWidget]
        type QSplitter;
        #[qobject]
        #[base = QWidget]
        type QScrollBar;
        #[qobject]
        #[base = QWidget]
        type QTreeWidget;
        #[qobject]
        type QVBoxLayout;
        #[qobject]
        type QHBoxLayout;
        #[qobject]
        type QAction;
        #[qobject]
        type QGraphicsDropShadowEffect;

        #[qsignal]
        fn triggered(self: Pin<&mut QAction>, checked: bool);
        #[qsignal]
        fn clicked(self: Pin<&mut QToolButton>, checked: bool);
        #[qsignal]
        #[cxx_name = "currentChanged"]
        fn current_changed(self: Pin<&mut QTabBar>, index: i32);
        #[qsignal]
        #[cxx_name = "tabCloseRequested"]
        fn tab_close_requested(self: Pin<&mut QTabBar>, index: i32);
        #[qsignal]
        #[cxx_name = "valueChanged"]
        fn value_changed(self: Pin<&mut QScrollBar>, value: i32);
    }

    extern "RustQt" {
        #[qobject]
        #[base = QWidget]
        type Canvas = super::CanvasRust;

        #[cxx_override]
        #[cxx_name = "paintEvent"]
        unsafe fn paint_event(self: Pin<&mut Self>, e: *mut QPaintEvent);
        #[cxx_override]
        #[cxx_name = "resizeEvent"]
        unsafe fn resize_event(self: Pin<&mut Self>, e: *mut QResizeEvent);
        #[cxx_override]
        #[cxx_name = "wheelEvent"]
        unsafe fn wheel_event(self: Pin<&mut Self>, e: *mut QWheelEvent);
        #[cxx_override]
        #[cxx_name = "timerEvent"]
        unsafe fn timer_event(self: Pin<&mut Self>, e: *mut QTimerEvent);
        #[cxx_override]
        #[cxx_name = "eventFilter"]
        unsafe fn event_filter(self: Pin<&mut Self>, watched: *mut QObject, e: *mut QEvent)
            -> bool;
    }
    impl cxx_qt::Constructor<()> for Canvas {}

    #[namespace = "rust::cxxqtlib1"]
    unsafe extern "C++" {
        #[rust_name = "new_app"]
        unsafe fn make_unique(argc: &mut i32, argv: *mut *mut c_char) -> UniquePtr<QApplication>;
        #[rust_name = "new_canvas"]
        fn new_ptr() -> *mut Canvas;
        #[rust_name = "new_widget"]
        fn new_ptr() -> *mut QWidget;
        #[rust_name = "new_tabbar"]
        fn new_ptr() -> *mut QTabBar;
        #[rust_name = "new_line_edit"]
        fn new_ptr() -> *mut QLineEdit;
        #[rust_name = "new_label"]
        fn new_ptr() -> *mut QLabel;
        #[rust_name = "new_tool_button"]
        fn new_ptr() -> *mut QToolButton;
        #[rust_name = "new_tree"]
        fn new_ptr() -> *mut QTreeWidget;
        #[rust_name = "new_menu"]
        unsafe fn new_ptr(parent: *mut QWidget) -> *mut QMenu;
        #[rust_name = "new_splitter"]
        fn new_ptr(o: Orientation) -> *mut QSplitter;
        #[rust_name = "new_scrollbar"]
        fn new_ptr(o: Orientation) -> *mut QScrollBar;
        #[rust_name = "new_shadow"]
        fn new_ptr() -> *mut QGraphicsDropShadowEffect;
        #[rust_name = "new_vbox"]
        unsafe fn new_ptr(parent: *mut QWidget) -> *mut QVBoxLayout;
        #[rust_name = "new_hbox"]
        unsafe fn new_ptr(parent: *mut QWidget) -> *mut QHBoxLayout;
        #[rust_name = "new_tree_item"]
        unsafe fn new_ptr(tree: *mut QTreeWidget) -> *mut QTreeWidgetItem;
        #[rust_name = "new_child_item"]
        unsafe fn new_ptr(parent: *mut QTreeWidgetItem) -> *mut QTreeWidgetItem;
        #[rust_name = "new_palette"]
        fn make_unique() -> UniquePtr<QPalette>;
        #[rust_name = "new_icon"]
        fn make_unique(path: &QString) -> UniquePtr<QIcon>;
        #[rust_name = "new_brush"]
        fn make_unique(c: &QColor, style: BrushStyle) -> UniquePtr<QBrush>;
        #[rust_name = "new_region"]
        fn make_unique() -> UniquePtr<QRegion>;
        #[rust_name = "new_key_sequence"]
        fn make_unique(s: &QString, f: SequenceFormat) -> UniquePtr<QKeySequence>;
        #[rust_name = "new_painter"]
        unsafe fn make_unique(w: *mut Canvas) -> UniquePtr<QPainter>;
        #[rust_name = "new_painter_on"]
        unsafe fn make_unique(d: *mut QPaintDevice) -> UniquePtr<QPainter>;
        #[rust_name = "new_transform"]
        fn make_unique(m11: f64, m12: f64, m21: f64, m22: f64, dx: f64, dy: f64)
            -> UniquePtr<QTransform>;
        #[rust_name = "new_mouse_event"]
        fn make_unique(
            t: EventType,
            local: &QPointF,
            global: &QPointF,
            button: MouseButton,
            buttons: MouseButtons,
            mods: KeyboardModifiers,
        ) -> UniquePtr<QMouseEvent>;
        #[rust_name = "new_wheel_event"]
        fn make_unique(
            pos: QPointF,
            global: QPointF,
            pixel: QPoint,
            angle: QPoint,
            buttons: MouseButtons,
            mods: KeyboardModifiers,
            phase: ScrollPhase,
            inverted: bool,
        ) -> UniquePtr<QWheelEvent>;
    }

    unsafe extern "C++" {
        #[Self = "QApplication"]
        fn exec() -> i32;
        #[Self = "QApplication"]
        #[rust_name = "send_event"]
        #[cxx_name = "sendEvent"]
        unsafe fn sendEvent(receiver: *mut QObject, e: *mut QEvent) -> bool;
        #[Self = "QApplication"]
        #[rust_name = "app_set_palette"]
        #[cxx_name = "setPalette"]
        unsafe fn appSetPalette(p: &QPalette, class_name: *const c_char);
        #[Self = "QApplication"]
        #[rust_name = "process_events"]
        #[cxx_name = "processEvents"]
        fn processEvents(flags: ProcessEventsFlags);
        #[Self = "QApplication"]
        #[rust_name = "app_style"]
        #[cxx_name = "style"]
        fn appStyle() -> *mut QStyle;
        #[rust_name = "app_set_style_sheet"]
        #[cxx_name = "setStyleSheet"]
        fn appSetStyleSheet(self: Pin<&mut QApplication>, s: &QString);
        #[rust_name = "style_name"]
        #[cxx_name = "objectName"]
        fn styleName(self: &QStyle) -> QString;

        // QWidget
        fn show(self: Pin<&mut QWidget>);
        fn update(self: Pin<&mut QWidget>);
        fn resize(self: Pin<&mut QWidget>, w: i32, h: i32);
        fn width(self: &QWidget) -> i32;
        fn height(self: &QWidget) -> i32;
        fn setWindowTitle(self: Pin<&mut QWidget>, t: &QString);
        fn setMinimumWidth(self: Pin<&mut QWidget>, w: i32);
        fn setFixedHeight(self: Pin<&mut QWidget>, h: i32);
        fn setFixedWidth(self: Pin<&mut QWidget>, w: i32);
        #[rust_name = "set_fixed_size"]
        fn setFixedSize(self: Pin<&mut QWidget>, w: i32, h: i32);
        fn setVisible(self: Pin<&mut QWidget>, on: bool);
        fn isVisible(self: &QWidget) -> bool;
        fn setPalette(self: Pin<&mut QWidget>, p: &QPalette);
        fn font(self: &QWidget) -> &QFont;
        fn setFont(self: Pin<&mut QWidget>, f: &QFont);
        fn setAutoFillBackground(self: Pin<&mut QWidget>, on: bool);
        fn setStyleSheet(self: Pin<&mut QWidget>, s: &QString);
        fn setObjectName(self: Pin<&mut QWidget>, n: QAnyStringView);
        fn setAttribute(self: Pin<&mut QWidget>, a: WidgetAttribute, on: bool);
        fn setFocusPolicy(self: Pin<&mut QWidget>, p: FocusPolicy);
        fn setToolTip(self: Pin<&mut QWidget>, t: &QString);
        fn setWindowIcon(self: Pin<&mut QWidget>, i: &QIcon);
        #[rust_name = "widget_set_margins"]
        #[cxx_name = "setContentsMargins"]
        fn setContentsMargins(self: Pin<&mut QWidget>, l: i32, t: i32, r: i32, b: i32);
        unsafe fn setParent(self: Pin<&mut QWidget>, p: *mut QWidget);
        #[rust_name = "set_geometry"]
        fn setGeometry(self: Pin<&mut QWidget>, x: i32, y: i32, w: i32, h: i32);
        fn raise(self: Pin<&mut QWidget>);
        unsafe fn addAction(self: Pin<&mut QWidget>, a: *mut QAction);
        unsafe fn setGraphicsEffect(self: Pin<&mut QWidget>, e: *mut QGraphicsEffect);
        fn windowHandle(self: &QWidget) -> *mut QWindow;
        fn devicePixelRatioF(self: &QWidget) -> f64;
        fn window(self: &QWidget) -> *mut QWidget;
        unsafe fn mapTo(self: &QWidget, parent: *const QWidget, p: &QPoint) -> QPoint;
        fn mapToGlobal(self: &QWidget, p: &QPoint) -> QPoint;
        unsafe fn render(
            self: Pin<&mut QWidget>,
            p: *mut RawPainter,
            off: &QPoint,
            r: &QRegion,
            f: RenderFlags,
        );

        // Canvas
        fn startTimer(self: Pin<&mut Canvas>, ms: i32, kind: TimerType) -> i32;
        fn killTimer(self: Pin<&mut Canvas>, id: i32);

        // QWindow
        fn requestUpdate(self: Pin<&mut QWindow>);
        unsafe fn installEventFilter(self: Pin<&mut QWindow>, f: *mut QObject);

        // Menus and actions
        #[rust_name = "menu_add_menu"]
        #[cxx_name = "addMenu"]
        fn addMenu(self: Pin<&mut QMenu>, title: &QString) -> *mut QMenu;
        #[rust_name = "menu_add_action"]
        #[cxx_name = "addAction"]
        fn addActionText(self: Pin<&mut QMenu>, text: &QString) -> *mut QAction;
        #[rust_name = "menu_add_separator"]
        #[cxx_name = "addSeparator"]
        fn addSeparator(self: Pin<&mut QMenu>) -> *mut QAction;
        unsafe fn popup(self: Pin<&mut QMenu>, p: &QPoint, at: *mut QAction);
        fn trigger(self: Pin<&mut QAction>);
        #[rust_name = "action_set_shortcut"]
        #[cxx_name = "setShortcut"]
        fn setShortcut(self: Pin<&mut QAction>, k: &QKeySequence);
        #[rust_name = "action_set_shortcut_context"]
        #[cxx_name = "setShortcutContext"]
        fn setShortcutContext(self: Pin<&mut QAction>, c: ShortcutContext);

        // QPalette
        #[rust_name = "pal_color"]
        #[cxx_name = "color"]
        fn color(self: &QPalette, g: ColorGroup, r: ColorRole) -> &QColor;
        #[rust_name = "pal_set_color"]
        #[cxx_name = "setColor"]
        fn setColor(self: Pin<&mut QPalette>, g: ColorGroup, r: ColorRole, c: &QColor);

        // QLineEdit
        #[rust_name = "edit_set_text"]
        #[cxx_name = "setText"]
        fn setText(self: Pin<&mut QLineEdit>, t: &QString);
        fn setPlaceholderText(self: Pin<&mut QLineEdit>, t: &QString);
        #[rust_name = "edit_set_frame"]
        #[cxx_name = "setFrame"]
        fn setFrame(self: Pin<&mut QLineEdit>, on: bool);

        // QLabel
        #[rust_name = "label_set_text"]
        #[cxx_name = "setText"]
        fn setText(self: Pin<&mut QLabel>, t: &QString);

        // QToolButton
        #[rust_name = "button_set_icon"]
        #[cxx_name = "setIcon"]
        fn setIcon(self: Pin<&mut QToolButton>, i: &QIcon);
        #[rust_name = "button_set_icon_size"]
        #[cxx_name = "setIconSize"]
        fn setIconSize(self: Pin<&mut QToolButton>, s: &QSize);
        fn setAutoRaise(self: Pin<&mut QToolButton>, on: bool);
        fn setCheckable(self: Pin<&mut QToolButton>, on: bool);
        fn setChecked(self: Pin<&mut QToolButton>, on: bool);
        fn isChecked(self: &QToolButton) -> bool;
        fn click(self: Pin<&mut QToolButton>);

        // QTabBar
        fn setDocumentMode(self: Pin<&mut QTabBar>, on: bool);
        fn setTabsClosable(self: Pin<&mut QTabBar>, on: bool);
        fn setDrawBase(self: Pin<&mut QTabBar>, on: bool);
        fn setExpanding(self: Pin<&mut QTabBar>, on: bool);
        fn setMovable(self: Pin<&mut QTabBar>, on: bool);
        fn setElideMode(self: Pin<&mut QTabBar>, m: TextElideMode);
        fn addTab(self: Pin<&mut QTabBar>, t: &QString) -> i32;
        fn removeTab(self: Pin<&mut QTabBar>, i: i32);
        fn setCurrentIndex(self: Pin<&mut QTabBar>, i: i32);
        fn currentIndex(self: &QTabBar) -> i32;
        fn count(self: &QTabBar) -> i32;
        fn tabText(self: &QTabBar, i: i32) -> QString;

        // QSplitter
        #[rust_name = "splitter_add_widget"]
        #[cxx_name = "addWidget"]
        unsafe fn addWidget(self: Pin<&mut QSplitter>, w: *mut QWidget);
        fn setHandleWidth(self: Pin<&mut QSplitter>, w: i32);
        fn setChildrenCollapsible(self: Pin<&mut QSplitter>, on: bool);
        fn setStretchFactor(self: Pin<&mut QSplitter>, i: i32, s: i32);
        fn setSizes(self: Pin<&mut QSplitter>, l: &QList_i32);

        // QScrollBar
        fn setRange(self: Pin<&mut QScrollBar>, min: i32, max: i32);
        fn setPageStep(self: Pin<&mut QScrollBar>, s: i32);
        fn setSingleStep(self: Pin<&mut QScrollBar>, s: i32);
        fn setValue(self: Pin<&mut QScrollBar>, v: i32);
        fn value(self: &QScrollBar) -> i32;

        // Shadow
        fn setBlurRadius(self: Pin<&mut QGraphicsDropShadowEffect>, r: f64);
        fn setOffset(self: Pin<&mut QGraphicsDropShadowEffect>, dx: f64, dy: f64);
        fn setColor(self: Pin<&mut QGraphicsDropShadowEffect>, c: &QColor);

        // QTreeWidget, QTreeWidgetItem
        fn setHeaderHidden(self: Pin<&mut QTreeWidget>, hide: bool);
        fn setFrameShape(self: Pin<&mut QTreeWidget>, s: FrameShape);
        fn setIndentation(self: Pin<&mut QTreeWidget>, i: i32);
        fn setRootIsDecorated(self: Pin<&mut QTreeWidget>, on: bool);
        fn setItemsExpandable(self: Pin<&mut QTreeWidget>, on: bool);
        fn setUniformRowHeights(self: Pin<&mut QTreeWidget>, on: bool);
        fn setHorizontalScrollBarPolicy(self: Pin<&mut QTreeWidget>, p: ScrollBarPolicy);
        fn expandAll(self: Pin<&mut QTreeWidget>);
        unsafe fn setCurrentItem(self: Pin<&mut QTreeWidget>, i: *mut QTreeWidgetItem);
        #[rust_name = "tree_viewport"]
        #[cxx_name = "viewport"]
        fn viewport(self: &QTreeWidget) -> *mut QWidget;
        #[rust_name = "item_set_text"]
        unsafe fn setText(self: Pin<&mut QTreeWidgetItem>, col: i32, t: &QString);
        #[rust_name = "item_set_size_hint"]
        unsafe fn setSizeHint(self: Pin<&mut QTreeWidgetItem>, col: i32, s: &QSize);
        #[rust_name = "item_set_foreground"]
        unsafe fn setForeground(self: Pin<&mut QTreeWidgetItem>, col: i32, b: &QBrush);
        #[rust_name = "item_set_font"]
        unsafe fn setFont(self: Pin<&mut QTreeWidgetItem>, col: i32, f: &QFont);

        // Layouts
        #[rust_name = "vbox_margins"]
        #[cxx_name = "setContentsMargins"]
        fn setContentsMargins(self: Pin<&mut QVBoxLayout>, l: i32, t: i32, r: i32, b: i32);
        #[rust_name = "vbox_spacing"]
        #[cxx_name = "setSpacing"]
        fn setSpacing(self: Pin<&mut QVBoxLayout>, s: i32);
        #[rust_name = "vbox_add"]
        #[cxx_name = "addWidget"]
        unsafe fn addWidget(self: Pin<&mut QVBoxLayout>, w: *mut QWidget, stretch: i32, a: Alignment);
        #[rust_name = "hbox_margins"]
        #[cxx_name = "setContentsMargins"]
        fn setContentsMargins(self: Pin<&mut QHBoxLayout>, l: i32, t: i32, r: i32, b: i32);
        #[rust_name = "hbox_spacing"]
        #[cxx_name = "setSpacing"]
        fn setSpacing(self: Pin<&mut QHBoxLayout>, s: i32);
        #[rust_name = "hbox_add"]
        #[cxx_name = "addWidget"]
        unsafe fn addWidget(self: Pin<&mut QHBoxLayout>, w: *mut QWidget, stretch: i32, a: Alignment);
        #[rust_name = "hbox_stretch"]
        #[cxx_name = "addStretch"]
        fn addStretch(self: Pin<&mut QHBoxLayout>, s: i32);

        // Events
        #[cxx_name = "type"]
        fn event_type(self: &QEvent) -> EventType;
        fn angleDelta(self: &QWheelEvent) -> QPoint;
        fn timerId(self: &QTimerEvent) -> i32;

        // QPainter (same C++ class as cxx-qt-lib's, but bindable here)
        #[rust_name = "set_world_transform"]
        fn setWorldTransform(self: Pin<&mut RawPainter>, t: &QTransform, combine: bool);
        #[rust_name = "draw_image_at"]
        fn drawImage(self: Pin<&mut RawPainter>, p: &QPoint, img: &QImage);
        #[rust_name = "raw_fill_rect"]
        fn fillRect(self: Pin<&mut RawPainter>, x: i32, y: i32, w: i32, h: i32, c: &QColor);
        #[rust_name = "painter_end"]
        fn end(self: Pin<&mut RawPainter>) -> bool;

        // QImage
        #[rust_name = "image_save"]
        unsafe fn save(self: &RawImage, path: &QString, fmt: *const c_char, quality: i32) -> bool;
        #[rust_name = "image_set_dpr"]
        fn setDevicePixelRatio(self: Pin<&mut RawImage>, d: f64);
    }
}

pub use ffi::*;
pub use super::CanvasRust;
