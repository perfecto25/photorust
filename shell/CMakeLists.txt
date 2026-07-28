# The C++/Qt QWidgets shell.
#
# Everything here is UI: window, docks, panels, canvas presentation and input.
# All pixel work lives in the Rust core and is reached through the CXX-Qt
# bridge (CLAUDE.md §2).

set(SHELL_SOURCES
    src/main.cpp
    src/MainWindow.cpp
    src/MainWindow.h

    src/canvas/CanvasView.cpp
    src/canvas/CanvasView.h

    src/panels/ColorPanel.cpp
    src/panels/ColorPanel.h
    src/panels/HistoryPanel.cpp
    src/panels/HistoryPanel.h
    src/panels/LayersPanel.cpp
    src/panels/LayersPanel.h

    src/shortcuts/CommandRegistry.cpp
    src/shortcuts/CommandRegistry.h

    src/tools/ToolId.h
    src/tools/ToolStrip.cpp
    src/tools/ToolStrip.h
)

add_executable(photorust ${SHELL_SOURCES})

target_include_directories(photorust PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/src)

# Link order matters: `photorust_core` carries the CXX-Qt generated headers and
# the initializer objects as usage requirements.
target_link_libraries(photorust PRIVATE
    photorust_core
    Qt::Core
    Qt::Gui
    Qt::Widgets
)

# Lets main.cpp fall back to the in-tree resources when running straight from
# the build directory, before anything has been installed.
target_compile_definitions(photorust PRIVATE
    PHOTORUST_SOURCE_RESOURCES="${CMAKE_CURRENT_SOURCE_DIR}/resources"
)

target_compile_options(photorust PRIVATE
    $<$<CXX_COMPILER_ID:GNU,Clang,AppleClang>:-Wall -Wextra -Wpedantic>
)

# Copy the theme and keymap next to the executable so a fresh build runs
# correctly without an install step.
add_custom_command(TARGET photorust POST_BUILD
    COMMAND ${CMAKE_COMMAND} -E copy_directory
        ${CMAKE_CURRENT_SOURCE_DIR}/resources
        $<TARGET_FILE_DIR:photorust>/resources
    COMMENT "Staging shell resources (theme.qss, shortcuts.json)"
)

# ---------------------------------------------------------------- install ---

if(APPLE)
    set_target_properties(photorust PROPERTIES
        MACOSX_BUNDLE TRUE
        MACOSX_BUNDLE_BUNDLE_NAME "PhotoRust"
        MACOSX_BUNDLE_GUI_IDENTIFIER "org.photorust.PhotoRust"
        MACOSX_BUNDLE_BUNDLE_VERSION "${PROJECT_VERSION}"
        MACOSX_BUNDLE_SHORT_VERSION_STRING "${PROJECT_VERSION}"
    )
    # Inside a bundle, resources belong in Contents/Resources.
    set_source_files_properties(resources/theme.qss resources/shortcuts.json
        PROPERTIES MACOSX_PACKAGE_LOCATION Resources)
endif()

install(TARGETS photorust
    BUNDLE  DESTINATION .
    RUNTIME DESTINATION bin
)

if(NOT APPLE)
    install(DIRECTORY resources/ DESTINATION bin/resources)
endif()
