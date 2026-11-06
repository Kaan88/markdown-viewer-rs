use cxx_qt_build::CxxQtBuilder;

fn main() {
    CxxQtBuilder::new()
        .qt_module("Gui")
        .qt_module("Widgets")
        .file("src/qt/ffi.rs")
        .build();
}
