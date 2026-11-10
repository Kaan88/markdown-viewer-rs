// The whole CXX-Qt bridge for this spike. Every binding here is a declaration:
// C++ glue is generated at build time, none is authored.
#![allow(non_snake_case, clippy::too_many_arguments)]

#[cxx_qt::bridge]
pub mod ffi {
    #[namespace = "Qt"]
    #[repr(u32)]
    enum DockWidgetArea {
        LeftDockWidgetArea = 1,
        RightDockWidgetArea = 2,
    }

    #[namespace = "Qt"]
    unsafe extern "C++" {
        type DockWidgetArea;
        include!("cxx-qt-lib/qt.h");
        type MouseButton = cxx_qt_lib::MouseButton;
        type MouseButtons = cxx_qt_lib::MouseButtons;
        type KeyboardModifiers = cxx_qt_lib::KeyboardModifiers;
        type DropActions = crate::qt::DropActions;
        type ScrollPhase = crate::qt::ScrollPhase;
        type TimerType = crate::qt::TimerType;
        type Alignment = crate::qt::Alignment;
        type Orientation = crate::qt::Orientation;
        type FocusPolicy = crate::qt::FocusPolicy;
        type CursorShape = crate::qt::CursorShape;
        type ContextMenuPolicy = crate::qt::ContextMenuPolicy;
        type ColorScheme = crate::qt::ColorScheme;
    }

    #[namespace = "QClipboard"]
    unsafe extern "C++" {
        #[cxx_name = "Mode"]
        type ClipboardMode = crate::qt::ClipboardMode;
    }

    #[namespace = "QEvent"]
    unsafe extern "C++" {
        #[cxx_name = "Type"]
        type EventType = crate::qt::EventType;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib/common.h");
        include!("cxx-qt-lib/qstring.h");
        include!("cxx-qt-lib/qstringlist.h");
        include!("cxx-qt-lib/qimage.h");
        include!("cxx-qt-lib/qpoint.h");
        include!("cxx-qt-lib/qpointf.h");
        include!("cxx-qt-lib/qrectf.h");
        include!("cxx-qt-lib/qcolor.h");
        type QString = cxx_qt_lib::QString;
        type QStringList = cxx_qt_lib::QStringList;
        type qsizetype = crate::qt::QSizeType;
        type QImage = cxx_qt_lib::QImage;
        type QPoint = cxx_qt_lib::QPoint;
        type QPointF = cxx_qt_lib::QPointF;
        type QRectF = cxx_qt_lib::QRectF;
        type QColor = cxx_qt_lib::QColor;
        type QUrl = cxx_qt_lib::QUrl;
        type QCursor;

        include!(<QtGui/QPainter>);
        include!(<QtGui/QTransform>);
        include!(<QtGui/QCursor>);
        include!(<QtGui/QPaintEvent>);
        include!(<QtGui/QMouseEvent>);
        include!(<QtGui/QKeyEvent>);
        include!(<QtGui/QWheelEvent>);
        include!(<QtGui/QDragEnterEvent>);
        include!(<QtGui/QDropEvent>);
        include!(<QtGui/QBackingStore>);
        include!(<QtGui/QDesktopServices>);
        include!(<QtCore/QTimerEvent>);
        include!(<QtCore/QEvent>);
        include!(<QtWidgets/QFileDialog>);
        type QPaintEvent;
        type QMouseEvent;
        type QKeyEvent;
        type QWheelEvent;
        type QDragEnterEvent;
        type QDropEvent;
        type QTimerEvent;
        type QEvent;
        type QPainter = cxx_qt_lib::QPainter;
        #[cxx_name = "QPainter"]
        type RawPainter;
        type QTransform;
        type QBackingStore;
        type QPaintDevice;
        type QDesktopServices;
        #[cxx_name = "QImage"]
        type RawImage;
        type QTreeWidgetItem;
    }

    // Existing Qt classes.
    unsafe extern "C++Qt" {
        include!(<QtWidgets/QApplication>);
        include!(<QtWidgets/QMainWindow>);
        include!(<QtWidgets/QMenuBar>);
        include!(<QtWidgets/QMenu>);
        include!(<QtWidgets/QToolBar>);
        include!(<QtWidgets/QLineEdit>);
        include!(<QtWidgets/QTabWidget>);
        include!(<QtWidgets/QTabBar>);
        include!(<QtWidgets/QDockWidget>);
        include!(<QtWidgets/QTreeWidget>);
        include!(<QtWidgets/QStatusBar>);
        include!(<QtWidgets/QVBoxLayout>);
        include!(<QtWidgets/QHBoxLayout>);
        include!(<QtWidgets/QScrollBar>);
        include!(<QtGui/QAction>);
        include!(<QtGui/QClipboard>);
        include!(<QtGui/QWindow>);
        include!(<QtCore/QMimeData>);

        #[qobject]
        type QApplication;
        #[qobject]
        type QWindow;
        #[qobject]
        type QMimeData;
        #[qobject]
        type QWidget;
        #[qobject]
        #[base = QWidget]
        type QMainWindow;
        #[qobject]
        #[base = QWidget]
        type QMenuBar;
        #[qobject]
        #[base = QWidget]
        type QMenu;
        #[qobject]
        #[base = QWidget]
        type QToolBar;
        #[qobject]
        #[base = QWidget]
        type QLineEdit;
        #[qobject]
        #[base = QWidget]
        type QTabWidget;
        #[qobject]
        #[base = QWidget]
        type QTabBar;
        #[qobject]
        #[base = QWidget]
        type QDockWidget;
        #[qobject]
        #[base = QWidget]
        type QTreeWidget;
        #[qobject]
        #[base = QWidget]
        type QStatusBar;
        #[qobject]
        type QVBoxLayout;
        #[qobject]
        type QHBoxLayout;
        #[qobject]
        #[base = QWidget]
        type QScrollBar;
        #[qobject]
        type QFileDialog;
        #[qobject]
        type QAction;

        #[qsignal]
        fn triggered(self: Pin<&mut QAction>, checked: bool);
        #[qsignal]
        #[rust_name = "value_changed"]
        #[cxx_name = "valueChanged"]
        fn valueChanged(self: Pin<&mut QScrollBar>, value: i32);
        #[qsignal]
        #[cxx_name = "textChanged"]
        fn text_changed(self: Pin<&mut QLineEdit>, text: &QString);
        #[qsignal]
        #[cxx_name = "returnPressed"]
        fn return_pressed(self: Pin<&mut QLineEdit>);
        #[qsignal]
        #[cxx_name = "itemClicked"]
        unsafe fn item_clicked(
            self: Pin<&mut QTreeWidget>,
            item: *mut QTreeWidgetItem,
            column: i32,
        );

        #[qsignal]
        #[cxx_name = "currentChanged"]
        fn current_changed(self: Pin<&mut QTabBar>, index: i32);
        #[qsignal]
        #[cxx_name = "tabCloseRequested"]
        fn tab_close_requested(self: Pin<&mut QTabBar>, index: i32);
        #[qsignal]
        #[cxx_name = "tabMoved"]
        fn tab_moved(self: Pin<&mut QTabBar>, from: i32, to: i32);
        #[qsignal]
        #[cxx_name = "customContextMenuRequested"]
        fn custom_context_menu_requested(self: Pin<&mut QWidget>, pos: &QPoint);
        #[qsignal]
        #[cxx_name = "aboutToShow"]
        fn about_to_show(self: Pin<&mut QMenu>);

        // QTabBar
        #[rust_name = "bar_add_tab"]
        #[cxx_name = "addTab"]
        fn addTabBar(self: Pin<&mut QTabBar>, label: &QString) -> i32;
        #[rust_name = "bar_remove_tab"]
        #[cxx_name = "removeTab"]
        fn removeTabBar(self: Pin<&mut QTabBar>, index: i32);
        #[rust_name = "bar_set_current"]
        #[cxx_name = "setCurrentIndex"]
        fn setCurrentIndexBar(self: Pin<&mut QTabBar>, index: i32);
        #[rust_name = "bar_current"]
        #[cxx_name = "currentIndex"]
        fn currentIndexBar(self: &QTabBar) -> i32;
        #[rust_name = "bar_count"]
        #[cxx_name = "count"]
        fn countBar(self: &QTabBar) -> i32;
        #[rust_name = "bar_set_text"]
        #[cxx_name = "setTabText"]
        fn setTabTextBar(self: Pin<&mut QTabBar>, index: i32, text: &QString);
        #[rust_name = "bar_set_tool_tip"]
        #[cxx_name = "setTabToolTip"]
        fn setTabToolTipBar(self: Pin<&mut QTabBar>, index: i32, text: &QString);
        #[rust_name = "bar_tab_text"]
        #[cxx_name = "tabText"]
        fn tabTextBar(self: &QTabBar, index: i32) -> QString;
        #[rust_name = "bar_set_closable"]
        #[cxx_name = "setTabsClosable"]
        fn setTabsClosableBar(self: Pin<&mut QTabBar>, on: bool);
        #[rust_name = "bar_set_movable"]
        #[cxx_name = "setMovable"]
        fn setMovableBar(self: Pin<&mut QTabBar>, on: bool);
        #[rust_name = "bar_set_document_mode"]
        #[cxx_name = "setDocumentMode"]
        fn setDocumentModeBar(self: Pin<&mut QTabBar>, on: bool);
        #[rust_name = "bar_set_auto_hide"]
        #[cxx_name = "setAutoHide"]
        fn setAutoHideBar(self: Pin<&mut QTabBar>, on: bool);
        #[rust_name = "bar_set_expanding"]
        #[cxx_name = "setExpanding"]
        fn setExpandingBar(self: Pin<&mut QTabBar>, on: bool);

        // QMenu submenus, rebuilt as they open
        #[rust_name = "menu_add_menu"]
        #[cxx_name = "addMenu"]
        fn addMenuMenu(self: Pin<&mut QMenu>, title: &QString) -> *mut QMenu;
        #[rust_name = "menu_clear"]
        #[cxx_name = "clear"]
        fn clearMenu(self: Pin<&mut QMenu>);
        #[rust_name = "menu_add_separator"]
        #[cxx_name = "addSeparator"]
        fn addSeparatorMenu(self: Pin<&mut QMenu>) -> *mut QAction;
        #[rust_name = "menu_set_tool_tips_visible"]
        #[cxx_name = "setToolTipsVisible"]
        fn setToolTipsVisibleMenu(self: Pin<&mut QMenu>, on: bool);
        #[rust_name = "action_set_enabled"]
        #[cxx_name = "setEnabled"]
        fn setEnabledAction(self: Pin<&mut QAction>, on: bool);
        #[rust_name = "action_set_tool_tip"]
        #[cxx_name = "setToolTip"]
        fn setToolTipAction(self: Pin<&mut QAction>, tip: &QString);

        #[rust_name = "clear_tree"]
        #[cxx_name = "clear"]
        fn clear(self: Pin<&mut QTreeWidget>);
        #[rust_name = "expand_tree"]
        #[cxx_name = "expandAll"]
        fn expandAll(self: Pin<&mut QTreeWidget>);

        // The find bar needs these; cxx-qt's generated header does not expose them by default.
        #[rust_name = "set_visible"]
        #[cxx_name = "setVisible"]
        fn setVisibleWidget(self: Pin<&mut QWidget>, visible: bool);
        #[rust_name = "is_visible"]
        #[cxx_name = "isVisible"]
        fn isVisibleWidget(self: &QWidget) -> bool;
        #[rust_name = "set_placeholder"]
        #[cxx_name = "setPlaceholderText"]
        fn setPlaceholderText(self: Pin<&mut QLineEdit>, text: &QString);
        #[rust_name = "select_all"]
        #[cxx_name = "selectAll"]
        fn selectAll(self: Pin<&mut QLineEdit>);
        #[rust_name = "add_toolbar_action"]
        #[cxx_name = "addAction"]
        fn addActionToolbar(self: Pin<&mut QToolBar>, text: &QString) -> *mut QAction;
        // The clipboard, bound directly: `clipboard` is QGuiApplication's static accessor, and the
        // explicit `Mode` argument picks `text(Mode)` out of its overloads, because cxx names the
        // member through a pointer of exactly this signature.
        // The OS colour scheme: 0 unknown, 1 light, 2 dark (Qt::ColorScheme).
        include!(<QtGui/QStyleHints>);
        include!(<QtGui/QGuiApplication>);
        #[qobject]
        type QStyleHints;
        #[Self = "QApplication"]
        fn styleHints() -> *mut QStyleHints;
        fn colorScheme(self: &QStyleHints) -> ColorScheme;
        #[qobject]
        type QClipboard;
        #[Self = "QApplication"]
        fn clipboard() -> *mut QClipboard;
        #[rust_name = "clipboard_get_text"]
        #[cxx_name = "text"]
        fn clipboardGetText(self: &QClipboard, mode: ClipboardMode) -> QString;
        #[rust_name = "clipboard_set_text"]
        #[cxx_name = "setText"]
        fn clipboardSetText(self: Pin<&mut QClipboard>, text: &QString, mode: ClipboardMode);
    }

    // The custom canvas: a QWidget subclass implemented in Rust.
    extern "RustQt" {
        #[qobject]
        #[base = QWidget]
        type Canvas = super::CanvasRust;

        #[cxx_override]
        #[cxx_name = "paintEvent"]
        unsafe fn paint_event(self: Pin<&mut Self>, e: *mut QPaintEvent);
        #[cxx_override]
        #[cxx_name = "mousePressEvent"]
        unsafe fn mouse_press_event(self: Pin<&mut Self>, e: *mut QMouseEvent);
        #[cxx_override]
        #[cxx_name = "mouseMoveEvent"]
        unsafe fn mouse_move_event(self: Pin<&mut Self>, e: *mut QMouseEvent);
        #[cxx_override]
        #[cxx_name = "mouseReleaseEvent"]
        unsafe fn mouse_release_event(self: Pin<&mut Self>, e: *mut QMouseEvent);
        #[cxx_override]
        #[cxx_name = "wheelEvent"]
        unsafe fn wheel_event(self: Pin<&mut Self>, e: *mut QWheelEvent);
        #[cxx_override]
        #[cxx_name = "dragEnterEvent"]
        unsafe fn drag_enter_event(self: Pin<&mut Self>, e: *mut QDragEnterEvent);
        #[cxx_override]
        #[cxx_name = "dropEvent"]
        unsafe fn drop_event(self: Pin<&mut Self>, e: *mut QDropEvent);
        #[cxx_override]
        #[cxx_name = "keyPressEvent"]
        unsafe fn key_press_event(self: Pin<&mut Self>, e: *mut QKeyEvent);
        #[cxx_override]
        #[cxx_name = "timerEvent"]
        unsafe fn timer_event(self: Pin<&mut Self>, e: *mut QTimerEvent);
        #[cxx_override]
        #[cxx_name = "eventFilter"]
        unsafe fn event_filter(self: Pin<&mut Self>, watched: *mut QObject, e: *mut QEvent)
            -> bool;

        // Calls the real QWidget::wheelEvent from the override.
        #[inherit]
        #[cxx_name = "wheelEvent"]
        unsafe fn base_wheel_event(self: Pin<&mut Self>, e: *mut QWheelEvent);
    }
    impl cxx_qt::Constructor<()> for Canvas {}

    // Constructors: the bridge binds cxx-qt-lib's templates, deduced per signature.
    #[namespace = "rust::cxxqtlib1"]
    unsafe extern "C++" {
        #[rust_name = "new_app"]
        unsafe fn make_unique(argc: &mut i32, argv: *mut *mut c_char) -> UniquePtr<QApplication>;
        #[rust_name = "new_canvas"]
        fn new_ptr() -> *mut Canvas;
        #[rust_name = "new_main_window"]
        fn new_ptr() -> *mut QMainWindow;
        #[rust_name = "new_widget"]
        fn new_ptr() -> *mut QWidget;
        #[rust_name = "new_tabs"]
        fn new_ptr() -> *mut QTabWidget;
        #[rust_name = "new_tab_bar"]
        fn new_ptr() -> *mut QTabBar;
        #[rust_name = "new_menu"]
        fn new_ptr() -> *mut QMenu;
        #[rust_name = "new_line_edit"]
        fn new_ptr() -> *mut QLineEdit;
        #[rust_name = "new_tree"]
        fn new_ptr() -> *mut QTreeWidget;
        #[rust_name = "new_vbox"]
        unsafe fn new_ptr(parent: *mut QWidget) -> *mut QVBoxLayout;
        #[rust_name = "new_hbox"]
        unsafe fn new_ptr(parent: *mut QWidget) -> *mut QHBoxLayout;
        #[rust_name = "new_scroll_bar"]
        unsafe fn new_ptr(orientation: Orientation, parent: *mut QWidget) -> *mut QScrollBar;
        #[rust_name = "new_file_dialog"]
        unsafe fn new_ptr(
            parent: *mut QWidget,
            caption: &QString,
            dir: &QString,
            filter: &QString,
        ) -> *mut QFileDialog;
        #[rust_name = "new_dock"]
        unsafe fn new_ptr(title: &QString, parent: *mut QWidget) -> *mut QDockWidget;
        #[rust_name = "new_tree_item"]
        unsafe fn new_ptr(tree: *mut QTreeWidget) -> *mut QTreeWidgetItem;
        #[rust_name = "new_child_item"]
        unsafe fn new_ptr(parent: *mut QTreeWidgetItem) -> *mut QTreeWidgetItem;
        #[rust_name = "new_mime"]
        fn new_ptr() -> *mut QMimeData;
        #[rust_name = "new_painter"]
        unsafe fn make_unique(w: *mut Canvas) -> UniquePtr<QPainter>;
        #[rust_name = "new_cursor"]
        fn make_unique(shape: CursorShape) -> UniquePtr<QCursor>;
        #[rust_name = "new_transform"]
        fn make_unique(
            m11: f64,
            m12: f64,
            m21: f64,
            m22: f64,
            dx: f64,
            dy: f64,
        ) -> UniquePtr<QTransform>;
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
        #[rust_name = "new_drag_enter_event"]
        unsafe fn make_unique(
            pos: QPoint,
            actions: DropActions,
            data: *const QMimeData,
            buttons: MouseButtons,
            mods: KeyboardModifiers,
        ) -> UniquePtr<QDragEnterEvent>;
        #[rust_name = "new_drop_event"]
        unsafe fn make_unique(
            pos: QPointF,
            actions: DropActions,
            data: *const QMimeData,
            buttons: MouseButtons,
            mods: KeyboardModifiers,
        ) -> UniquePtr<QDropEvent>;
    }

    unsafe extern "C++" {
        // QApplication / QCoreApplication
        #[Self = "QApplication"]
        fn exec() -> i32;
        #[Self = "QApplication"]
        #[rust_name = "send_event"]
        #[cxx_name = "sendEvent"]
        unsafe fn sendEvent(receiver: *mut QObject, e: *mut QEvent) -> bool;

        // QWidget
        fn show(self: Pin<&mut QWidget>);
        fn update(self: Pin<&mut QWidget>);
        fn repaint(self: Pin<&mut QWidget>);
        fn resize(self: Pin<&mut QWidget>, w: i32, h: i32);
        fn width(self: &QWidget) -> i32;
        fn height(self: &QWidget) -> i32;
        fn setWindowTitle(self: Pin<&mut QWidget>, t: &QString);
        fn setAcceptDrops(self: Pin<&mut QWidget>, on: bool);
        fn setMouseTracking(self: Pin<&mut QWidget>, on: bool);
        fn setMinimumWidth(self: Pin<&mut QWidget>, w: i32);
        fn setMaximumWidth(self: Pin<&mut QWidget>, w: i32);
        fn setFocusPolicy(self: Pin<&mut QWidget>, p: FocusPolicy);
        fn setFocus(self: Pin<&mut QWidget>);
        fn setContextMenuPolicy(self: Pin<&mut QWidget>, p: ContextMenuPolicy);
        fn mapToGlobal(self: &QWidget, p: &QPoint) -> QPoint;
        fn tabAt(self: &QTabBar, p: &QPoint) -> i32;
        unsafe fn popup(self: Pin<&mut QMenu>, p: &QPoint, at: *mut QAction);
        fn setCursor(self: Pin<&mut QWidget>, cursor: &QCursor);
        fn windowHandle(self: &QWidget) -> *mut QWindow;
        fn devicePixelRatioF(self: &QWidget) -> f64;
        fn window(self: &QWidget) -> *mut QWidget;
        fn backingStore(self: &QWidget) -> *mut QBackingStore;
        unsafe fn mapTo(self: &QWidget, parent: *const QWidget, p: &QPoint) -> QPoint;

        // Canvas (inherited QObject / QWidget members it needs directly)
        fn startTimer(self: Pin<&mut Canvas>, ms: i32, kind: TimerType) -> i32;
        fn killTimer(self: Pin<&mut Canvas>, id: i32);

        // QWindow
        fn requestUpdate(self: Pin<&mut QWindow>);
        unsafe fn installEventFilter(self: Pin<&mut QWindow>, f: *mut QObject);

        // QMainWindow
        unsafe fn setCentralWidget(self: Pin<&mut QMainWindow>, w: *mut QWidget);
        fn menuBar(self: &QMainWindow) -> *mut QMenuBar;
        fn statusBar(self: &QMainWindow) -> *mut QStatusBar;
        fn addToolBar(self: Pin<&mut QMainWindow>, title: &QString) -> *mut QToolBar;
        unsafe fn addDockWidget(
            self: Pin<&mut QMainWindow>,
            a: DockWidgetArea,
            d: *mut QDockWidget,
        );

        // Menus, toolbar, status bar
        fn addMenu(self: Pin<&mut QMenuBar>, title: &QString) -> *mut QMenu;
        fn addAction(self: Pin<&mut QMenu>, text: &QString) -> *mut QAction;
        unsafe fn addWidget(self: Pin<&mut QToolBar>, w: *mut QWidget) -> *mut QAction;
        fn trigger(self: Pin<&mut QAction>);
        fn showMessage(self: Pin<&mut QStatusBar>, text: &QString, ms: i32);

        // QLineEdit
        fn setText(self: Pin<&mut QLineEdit>, t: &QString);

        // QFileDialog
        #[rust_name = "exec_dialog"]
        fn exec(self: Pin<&mut QFileDialog>) -> i32;
        #[rust_name = "selected_files"]
        fn selectedFiles(self: &QFileDialog) -> QStringList;

        // QScrollBar
        fn setRange(self: Pin<&mut QScrollBar>, min: i32, max: i32);
        fn setValue(self: Pin<&mut QScrollBar>, v: i32);
        fn value(self: &QScrollBar) -> i32;
        fn setPageStep(self: Pin<&mut QScrollBar>, step: i32);
        fn setSingleStep(self: Pin<&mut QScrollBar>, step: i32);

        // Keys
        #[rust_name = "key_code"]
        fn key(self: &QKeyEvent) -> i32;
        fn modifiers(self: &QKeyEvent) -> KeyboardModifiers;

        // QTabWidget
        unsafe fn addTab(self: Pin<&mut QTabWidget>, w: *mut QWidget, label: &QString) -> i32;

        // QDockWidget
        unsafe fn setWidget(self: Pin<&mut QDockWidget>, w: *mut QWidget);

        // QTreeWidget, QTreeWidgetItem
        fn setHeaderHidden(self: Pin<&mut QTreeWidget>, hide: bool);
        #[rust_name = "item_set_text"]
        unsafe fn setText(self: Pin<&mut QTreeWidgetItem>, col: i32, t: &QString);

        // QVBoxLayout
        fn setContentsMargins(self: Pin<&mut QVBoxLayout>, l: i32, t: i32, r: i32, b: i32);
        fn setSpacing(self: Pin<&mut QVBoxLayout>, s: i32);
        #[rust_name = "layout_add_widget"]
        unsafe fn addWidget(
            self: Pin<&mut QVBoxLayout>,
            w: *mut QWidget,
            stretch: i32,
            a: Alignment,
        );

        // QHBoxLayout
        #[rust_name = "hbox_set_margins"]
        fn setContentsMargins(self: Pin<&mut QHBoxLayout>, l: i32, t: i32, r: i32, b: i32);
        #[rust_name = "hbox_set_spacing"]
        fn setSpacing(self: Pin<&mut QHBoxLayout>, s: i32);
        #[rust_name = "hbox_add_widget"]
        unsafe fn addWidget(
            self: Pin<&mut QHBoxLayout>,
            w: *mut QWidget,
            stretch: i32,
            a: Alignment,
        );

        // Events
        #[cxx_name = "type"]
        fn event_type(self: &QEvent) -> EventType;
        #[rust_name = "mouse_pos"]
        fn position(self: &QMouseEvent) -> QPointF;
        fn angleDelta(self: &QWheelEvent) -> QPoint;
        fn acceptProposedAction(self: Pin<&mut QDragEnterEvent>);
        fn mimeData(self: &QDropEvent) -> *const QMimeData;
        #[rust_name = "drop_accept"]
        fn acceptProposedAction(self: Pin<&mut QDropEvent>);
        #[rust_name = "drag_mime_data"]
        #[cxx_name = "mimeData"]
        fn mimeData(self: &QDragEnterEvent) -> *const QMimeData;

        // QMimeData
        fn text(self: &QMimeData) -> QString;
        #[rust_name = "mime_set_text"]
        fn setText(self: Pin<&mut QMimeData>, t: &QString);

        // QPainter
        #[rust_name = "set_world_transform"]
        fn setWorldTransform(self: Pin<&mut RawPainter>, t: &QTransform, combine: bool);
        #[rust_name = "draw_image_at"]
        fn drawImage(self: Pin<&mut RawPainter>, p: &QPoint, img: &QImage);
        #[rust_name = "fill_rect_raw"]
        fn fillRect(self: Pin<&mut RawPainter>, r: &QRectF, c: &QColor);
        #[rust_name = "painter_end"]
        fn end(self: Pin<&mut RawPainter>) -> bool;

        // Opening a web or mail link in the platform's handler.
        #[Self = "QDesktopServices"]
        #[rust_name = "open_url"]
        fn openUrl(url: &QUrl) -> bool;

        // Backing store
        fn paintDevice(self: Pin<&mut QBackingStore>) -> *mut QPaintDevice;
        fn devType(self: &QPaintDevice) -> i32;
        fn constBits(self: &RawImage) -> *const u8;
        fn bytesPerLine(self: &RawImage) -> qsizetype;
        #[rust_name = "raw_depth"]
        fn depth(self: &RawImage) -> i32;
        #[rust_name = "raw_width"]
        fn width(self: &RawImage) -> i32;
        #[rust_name = "raw_height"]
        fn height(self: &RawImage) -> i32;
        fn timerId(self: &QTimerEvent) -> i32;
    }
}

pub use super::CanvasRust;
pub use ffi::*;
