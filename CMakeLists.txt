# PhotoRust — a Photoshop CS6 clone.
#
# Build layout (CLAUDE.md §2): a C++/Qt QWidgets shell over a Rust image
# engine, joined by CXX-Qt. CMake drives both halves — Corrosion (pulled in by
# cxx-qt-cmake) invokes Cargo for `core/`, and the resulting static library is
# linked into the shell executable.

cmake_minimum_required(VERSION 3.24)

project(PhotoRust
    VERSION 0.1.0
    DESCRIPTION "A Photoshop CS6 clone: Qt Widgets shell over a Rust image engine"
    LANGUAGES CXX)

# Windows is explicitly out of scope (CLAUDE.md §1).
if(WIN32)
    message(FATAL_ERROR
        "PhotoRust targets Linux and macOS only. Windows is not supported.")
endif()

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

# CXX-Qt generates QObject subclasses in Rust; AUTOMOC has to see the headers
# they produce, and our own widgets need it regardless.
set(CMAKE_AUTOMOC ON)
set(CMAKE_AUTOUIC ON)
set(CMAKE_AUTORCC ON)

if(NOT CMAKE_BUILD_TYPE AND NOT CMAKE_CONFIGURATION_TYPES)
    set(CMAKE_BUILD_TYPE Debug CACHE STRING "Build type" FORCE)
endif()

# Put the executable somewhere predictable for the run instructions in the
# README, rather than nested under shell/.
set(CMAKE_RUNTIME_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR})

# --------------------------------------------------------------------- Qt ---

find_package(Qt6 6.4 REQUIRED COMPONENTS Core Gui Widgets)

# ---------------------------------------------------------------- CXX-Qt ---

# cxx-qt-cmake wraps Corrosion and handles the parts that are easy to get
# wrong by hand: the per-crate initializer objects, the generated include
# directories, and passing QT_MODULES through to the Rust build script.
find_package(CxxQt QUIET)
if(NOT CxxQt_FOUND)
    include(FetchContent)
    FetchContent_Declare(
        CxxQt
        GIT_REPOSITORY https://github.com/KDAB/cxx-qt-cmake.git
        GIT_TAG main
    )
    FetchContent_MakeAvailable(CxxQt)
endif()

# Creates a CMake target named after the crate: `photorust_core`.
#
# QT_MODULES must list every Qt module the Rust side links against. It has to
# match what `core/build.rs` asks for, or cxx-qt-build aborts the Cargo build
# with a mismatch error.
cxx_qt_import_crate(
    MANIFEST_PATH core/Cargo.toml
    CRATES photorust_core
    QT_MODULES Qt::Core Qt::Gui
)

# ------------------------------------------------------------------ shell ---

add_subdirectory(shell)

# ------------------------------------------------------------------ tests ---

include(CTest)
if(BUILD_TESTING)
    add_subdirectory(tests)
endif()

# ---------------------------------------------------------------- summary ---

message(STATUS "")
message(STATUS "PhotoRust ${PROJECT_VERSION}")
message(STATUS "  Build type : ${CMAKE_BUILD_TYPE}")
message(STATUS "  Qt         : ${Qt6_VERSION}")
message(STATUS "  Compiler   : ${CMAKE_CXX_COMPILER_ID} ${CMAKE_CXX_COMPILER_VERSION}")
message(STATUS "")
