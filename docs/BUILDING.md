# Building PhotoRust

Targets Linux and macOS. Windows is out of scope — the top-level
`CMakeLists.txt` fails the configure step there rather than half-working.

---

## Prerequisites

- **Qt 6** (6.4+) with QWidgets development headers
- A **stable Rust** toolchain (1.75+)
- **CMake** 3.24+
- A C++17 compiler

Corrosion and the CXX-Qt CMake module are fetched automatically at configure
time, so the first configure needs network access.

```bash
# Fedora
sudo dnf install qt6-qtbase-devel qt6-qtsvg-devel cmake gcc-c++ mold

# Debian / Ubuntu
sudo apt install qt6-base-dev libqt6svg6-dev cmake g++ mold

# macOS
brew install qt cmake rust
```

`mold` (or `lld`) is not strictly required, but the Rust build warns without
one and linking is much slower with GNU `ld.bfd`.

## Build and run

```bash
cmake -S . -B build -DCMAKE_BUILD_TYPE=Debug
cmake --build build -j$(nproc)

./build/photorust
```

CMake drives both halves: Corrosion invokes Cargo for `core/`, and the
resulting static library is linked into the shell. The theme, keymap and icon
are staged next to the executable, so a fresh build runs without installing.

To get a launcher entry and a desktop icon, install it:

```bash
cmake --install build --prefix ~/.local
```

That puts the binary in `~/.local/bin`, the icon into the hicolor theme and
`org.photorust.PhotoRust.desktop` into `~/.local/share/applications`. The
running window already shows the icon without installing; the desktop entry is
what gives it a launcher, a name and a file association for `.psd`.

### Tests

The engine's tests live beside the code they cover:

```bash
cd core && cargo test        # 623 tests
```

The GPU tests compare each accelerated operation against the CPU reference. On
a machine with no GPU they still pass — there is simply nothing to compare —
so the suite is not a check that acceleration works, only that it is correct
where present. To verify the CPU fallback explicitly:

```bash
PHOTORUST_BACKEND=cpu cargo test    # force the CPU path
WGPU_BACKEND=dx12 cargo test        # no adapter on Linux: exercises fallback
```

or through CTest, which runs them as part of the project:

```bash
ctest --test-dir build --output-on-failure
```
