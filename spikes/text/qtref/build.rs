fn main() {
    cxx_qt_build::CxxQtBuilder::new()
        .qt_module("Gui")
        .qt_module("Widgets")
        .file("src/main.rs")
        .build();
}
