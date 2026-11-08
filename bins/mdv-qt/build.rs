use cxx_qt_build::CxxQtBuilder;

fn main() {
    unsafe {
        CxxQtBuilder::new()
            .qt_module("Gui")
            .qt_module("Widgets")
            .file("src/qt/ffi.rs")
            .build();
    }
}
