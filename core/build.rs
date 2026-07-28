fn main() {
    cxx_qt_build::CxxQtBuilder::new()
        // QWidgets lives on the C++ side; the engine only needs Core + Gui
        // (QImage, QColor) to hand composited pixels across the bridge.
        .qt_module("Gui")
        .file("src/bridge.rs")
        .build();
}
