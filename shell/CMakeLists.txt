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

    src/dialogs/ColorPickerDialog.cpp
    src/dialogs/ColorPickerDialog.h

    src/panels/BrushPresetPicker.cpp
    src/panels/BrushPresetPicker.h
    src/panels/ColorPanel.cpp
    src/panels/ColorPanel.h
    src/panels/HistoryPanel.cpp
    src/panels/HistoryPanel.h
    src/panels/InfoPanel.cpp
    src/panels/InfoPanel.h
    src/panels/LayersPanel.cpp
    src/panels/LayersPanel.h
    src/panels/PanelHeader.cpp
    src/panels/PanelHeader.h

    src/shortcuts/CommandRegistry.cpp
    src/shortcuts/CommandRegistry.h

    src/tools/ToolIcons.cpp
    src/tools/ToolIcons.h
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
    Qt::Svg
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

# The application icon lives at the top of the repository, so it can be edited
# in place without hunting through the source tree. It is staged and installed
# under the name the .desktop file's `Icon=` key refers to.
set(PHOTORUST_ICON "${PROJECT_SOURCE_DIR}/icons/photorust.svg")
if(NOT EXISTS "${PHOTORUST_ICON}")
    message(FATAL_ERROR "Application icon not found at ${PHOTORUST_ICON}")
endif()
# Re-run the staging step when the icon is edited.
set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${PHOTORUST_ICON}")

# Copy the theme and keymap next to the executable so a fresh build runs
# correctly without an install step.
add_custom_command(TARGET photorust POST_BUILD
    COMMAND ${CMAKE_COMMAND} -E copy_directory
        ${CMAKE_CURRENT_SOURCE_DIR}/resources
        $<TARGET_FILE_DIR:photorust>/resources
    # After the directory copy, so it lands in the staged tree rather than being
    # overwritten by it.
    COMMAND ${CMAKE_COMMAND} -E copy_if_different
        "${PHOTORUST_ICON}"
        $<TARGET_FILE_DIR:photorust>/resources/icons/photorust.svg
    COMMENT "Staging shell resources (theme.qss, shortcuts.json, icon)"
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
    # The icon is copied rather than declared as a source, since it lives outside
    # this directory.
    add_custom_command(TARGET photorust POST_BUILD
        COMMAND ${CMAKE_COMMAND} -E make_directory
            $<TARGET_BUNDLE_CONTENT_DIR:photorust>/Resources/icons
        COMMAND ${CMAKE_COMMAND} -E copy_if_different
            "${PHOTORUST_ICON}"
            $<TARGET_BUNDLE_CONTENT_DIR:photorust>/Resources/icons/photorust.svg
        COMMENT "Staging the application icon into the bundle"
    )
    # That covers the window icon, which Qt loads from the SVG. The Finder
    # icon is a separate thing: it needs an .icns, which has to be generated
    # from the SVG with iconutil and pointed at by MACOSX_BUNDLE_ICON_FILE.
endif()

install(TARGETS photorust
    BUNDLE  DESTINATION .
    RUNTIME DESTINATION bin
)

if(NOT APPLE)
    install(DIRECTORY resources/ DESTINATION bin/resources)

    # Desktop integration. The icon goes into the hicolor theme under the name
    # the .desktop file's `Icon=` key refers to, which is how launchers, the
    # task switcher and the window list all find it.
    include(GNUInstallDirs)
    install(FILES "${PHOTORUST_ICON}"
        DESTINATION ${CMAKE_INSTALL_DATAROOTDIR}/icons/hicolor/scalable/apps
        RENAME photorust.svg)
    install(FILES resources/org.photorust.PhotoRust.desktop
        DESTINATION ${CMAKE_INSTALL_DATAROOTDIR}/applications)
endif()
