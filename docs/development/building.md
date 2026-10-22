# Building

## Requirements

* **Rust 1.88 or newer** (edition 2024), installed with
  [rustup](https://rustup.rs).
* A Python environment with CadQuery, to *run* the editor and the worker
  tests (see [Installation](../getting-started/installation.md)). Python is
  not needed to compile.

```sh
cargo build             # debug build: target/debug/cq-editor
cargo build --release   # release build: target/release/cq-editor
cargo run -- examples/01_hello_box.py
```

### Debug and release builds

| | Debug | Release |
| --- | --- | --- |
| Worker sources | Loaded from `worker/` in the source tree. Python edits apply on the next worker restart (Run > Restart Python) | Embedded in the executable and extracted to the data folder on first use |
| Console window on Windows | Yes (logs are visible) | No (`windows_subsystem = "windows"`). `--help` and command-line errors are written to the console of the shell that started it, or shown in a message box |
| Optimization | Dependencies are optimized (`opt-level = 2`), the crate itself is not, and only line tables are kept for debugging | Thin LTO, stripped |
| `.venv` discovery | Also looks in the source tree | Next to the executable and in the working directory |

`CQ_EDITOR_WORKER_DIR=/path/to/dir` makes any build load the `cq_worker`
package from `dir` instead.

`build.rs` embeds every `.py` file of `worker/cq_worker/` and `examples/`.
Adding a worker module or an example therefore needs no code change: it is
picked up on the next build.

## Linux

Install the development packages for keyboard handling and the GTK file
dialogs (the same ones CI uses on Ubuntu 22.04):

```sh
sudo apt-get install -y libxkbcommon-dev libgtk-3-dev
```

## macOS

Install the Xcode command line tools (`xcode-select --install`). No other
dependency is needed.

## Windows

Two Rust toolchains work.

### MSVC (recommended for most people)

Install the *Desktop development with C++* workload of Visual Studio (or the
Build Tools), then:

```powershell
rustup default stable-x86_64-pc-windows-msvc
cargo build --release
```

Nothing else is needed.

### GNU (no Visual Studio)

The `x86_64-pc-windows-gnu` toolchain avoids Visual Studio, but its minimal
self-contained MinGW environment lacks **`as` and `dlltool`** from MinGW-w64
binutils, used by the `windows` crates to build import libraries. Install
binutils, for example from [WinLibs](https://winlibs.com) or MSYS2 (`pacman
-S mingw-w64-x86_64-binutils`), and put its `bin` folder on `PATH`.
Otherwise the build fails with *dlltool could not create import library*.

Then build from a shell where binutils is on `PATH`:

```sh
export PATH="$HOME/.cargo/bin:/path/to/mingw/bin:$PATH"
cargo build --release
```

The `.exe` gets its icon and version information (shown by Explorer) from a
resource compiled by `build.rs` with `windres`, which also needs `gcc` as its
preprocessor: both come with a full MinGW-w64 installation (MSYS2: `pacman
-S mingw-w64-x86_64-gcc`). Without them the build still succeeds, with a
warning and an executable without that information. The MSVC toolchain uses
`rc.exe` from the Windows SDK instead. Release builds in CI set
`CQ_EDITOR_REQUIRE_RESOURCES=1`, which makes a missing resource compiler an
error.

Links are opened with the `open` crate rather than eframe's `links` feature,
so the GNU toolchain no longer needs a hand-made `libshlwapi.a`.

Setting `CARGO_INCREMENTAL=0` reduces the size of `target/` considerably,
which helps on small disks.

## Release builds in CI

`.github/workflows/release.yml` runs when a GitHub release is published, or
manually. It builds `cargo build --release` on Ubuntu 22.04, Windows and
macOS, and attaches:

* `CQ-editor-linux-x86_64`
* `CQ-editor-windows-x86_64.exe`
* `CQ-editor-macos-arm64`

Each file is the complete application. Users still need their own Python
environment with CadQuery.
