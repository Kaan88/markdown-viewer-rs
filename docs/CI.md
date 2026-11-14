# CI and releases (GitHub Actions)

Two workflows live in `.github/workflows/`. Nothing here needs a secret: the repository, the
forks of winit and softbuffer and the shared crates are all public, and the release job uses the
default `GITHUB_TOKEN`.

## ci.yml: checks

Runs on a push to `main`, on every pull request, and by hand (`workflow_dispatch`). A newer run of
the same branch cancels the older one. Permissions: `contents: read`.

Two jobs, `linux` (ubuntu-latest) and `windows` (windows-latest, the MSVC toolchain, natively):

1. `cargo fmt --all --check`
2. `cargo clippy --release --workspace --all-targets -- -D warnings`
3. `cargo test --release --workspace`
4. The headless screenshot suite as a smoke test: `cargo build --release -p mdview -p mdview-engine`,
   `bash tests/shots.sh target/shots`, then `tests/shots-check.sh target/shots`, which fails when
   any shot named in `shots.sh` is missing, empty or not a PNG. There is no pixel comparison:
   the runners' fonts differ from the development machines'. The PNGs are uploaded as the
   artifacts `shots-linux` and `shots-windows` (14 days).

The Rust toolchain is pinned in the workflows (`RUST_TOOLCHAIN`, 1.99.1) with
`dtolnay/rust-toolchain`; there is no `rust-toolchain.toml`, so a local build keeps using
whatever Rust is installed. `Swatinem/rust-cache` caches the registry and `target`. The Linux job
installs the system packages the build links against (the list of `packaging/linux/Dockerfile`).

## release.yml: the release

Triggered by pushing a tag `v*`, and by hand with `workflow_dispatch` (input `publish`, default
`false`). The first job refuses a tag that is not `v` plus the version in `[workspace.package]`
of `Cargo.toml`, before anything is built.

| Job | Runs on | Does |
| --- | --- | --- |
| `windows` | windows-latest | Installs Inno Setup (`choco install innosetup`), runs `packaging\windows\build-installer.ps1` (native build of `mdview.exe` and `mdview-engine.exe` with the static C runtime, the Inno Setup installer), then `packaging\windows\build-zip.ps1` (the portable zip). |
| `linux` | ubuntu-latest | `packaging/linux/build-packages.sh`: builds the binary in the Ubuntu 20.04 image of `packaging/linux/Dockerfile` (glibc 2.31, the script refuses a binary that needs a newer one), then the AppImage, tar.gz, deb, rpm and the PKGBUILD from that one binary. Smoke test: the tar.gz is unpacked, installed into a temporary prefix, run headless, uninstalled. |
| `publish` | ubuntu-latest | Needs both. Writes `mdview-update.txt` with both platforms (`packaging/write-manifest.sh`: the Windows entry names the native exes the `windows` job built and carries the engine keys, the Linux entry the AppImage), checks every hash against its file, runs the repo's manifest test (`cargo test --release -p mdview the_release_scripts_write_a_manifest_this_build_reads`) against the assembled `target/release-files-all/`, and, only for a tag or `publish: true`, runs `gh release create v<version>` at the tagged commit with all assets, marked latest. This is the only job with `contents: write`. |

The release text is `docs/release-notes/<version>.md` when that file exists (its first non-empty
line is also the manifest's one-line note, which the update bar shows), else a default line.

### Cutting a release

1. Set the version in `[workspace.package]` of `Cargo.toml`; optionally write
   `docs/release-notes/<version>.md`. Commit, push, and wait for the `ci` run on `main`.
2. `git tag v<version> && git push origin v<version>`.
3. The `release` run builds everything and creates the release `v<version>`: the assets below.

Dry run: Actions, `release`, Run workflow, leave `publish` unchecked (or
`gh workflow run release.yml --ref <branch>`). Everything is built and checked, and uploaded as
one artifact `release-files-all`; no release is created.

### Assets

| File | For |
| --- | --- |
| `mdview.exe`, `mdview-engine.exe` | The executable and its engine (the second process, for diagrams and math); the two files the updater installs. |
| `mdview-setup-<version>.exe` | The Inno Setup installer (per user, file associations). |
| `mdview-<version>-windows-x86_64.zip` | Portable: `mdview.exe`, `mdview-engine.exe`, `LICENSE`, `README.md` in one folder. |
| `mdview-<version>-linux-x86_64.AppImage` | Any distribution with glibc 2.31 or newer. Updates itself. |
| `mdview-<version>-linux-x86_64.tar.gz` | `install.sh [PREFIX]` (default `~/.local`) and `uninstall.sh`. |
| `mdview_<version>_amd64.deb` | Debian 11+, Ubuntu 20.04+ and derivatives. |
| `mdview-<version>-1.x86_64.rpm` | Fedora, openSUSE, RHEL and derivatives. |
| `PKGBUILD` | Arch: `makepkg -si` in a folder holding it; installs the tar.gz. |
| `mdview-update.txt` | The update manifest (Windows and Linux entries). |

## Who should use which

- Windows: the setup for a normal install; the zip for a portable copy (keep the two exes together).
- Linux with a package manager: the deb, the rpm or the PKGBUILD, so the system keeps it updated.
- Linux, any distribution, no root: the AppImage (self-updating) or the tar.gz into `~/.local`.

## Which copies update themselves

On Windows every copy that is not a `cargo` build folder: the installed one, the portable exe, the
zip's (the folder must be writable). On Linux only the AppImage (`$APPIMAGE` names a file in a
writable folder, `mdshared_app::updater::installed_files`). The deb, rpm, tar.gz and Arch
packages do not look for updates at launch, and the Update button answers with the line
"Only the AppImage can update itself." (the shared crate's text off Windows; also what
Settings > Updates and the About dialog's check lead to). The window writes its own desktop
entry only when `$APPIMAGE` is set, so the packages, which install a desktop file with
`Exec=mdview %F` and `StartupWMClass=mdview`, are not affected.

## Local builds

`packaging/release-all.sh` (the local, cross-built Windows files from Wine and `cargo-xwin`) keeps
working and now also builds the Linux packages; it is for testing. Files for users come from
this workflow, with the Windows exe built natively. `packaging/linux/build-packages.sh` builds
the Linux packages locally with Docker (`target/dist-linux/`), the same script the workflow runs.

## Known limits

- x86_64 only: no ARM builds, no macOS.
- The Windows files are not code-signed.
- The deb and the rpm are not signed and no repository serves them.
- The Arch package is `mdview-bin`: it installs the release tar.gz. The PKGBUILD in the repository
  keeps `sha256sums=('SKIP')`, because the sum of a release that does not exist yet cannot be
  known; the release's own `PKGBUILD` asset has the real one.

## Findings from the first runs

Found by the first CI runs on a clean Linux runner and on real Windows (the first time the whole
viewer test suite ran there in CI). All are fixed; no test is skipped and no step may fail
(`ci.yml` has no `--skip` and no `continue-on-error`). A few tests are `#[ignore]`d in the source
(engine idle and watchdog waits of 5 to 10 s, dumps and a live network test); they are run by hand
with `-- --ignored`.

- Windows drew U+2714 and U+2764 as empty boxes on the "Without the selector" line of
  `fixtures/emoji.md`: `shared_fonts()` applied the shared font fallbacks off Windows only, and the
  editor added them itself on Windows. `shared_fonts()` now applies them on every system (shared
  crates), so the page text and the chrome draw both as text glyphs on Windows (looked at in the CI shot).
- `an_aligned_image_stands_beside_its_text` (crates/render/tests/render.rs) found the image's
  columns by pixel colour, and the text's subpixel fringes made the same colour in the runner's
  Noto Sans. It now puts the image in a link and reads where the image is from the layout (the
  renderer's hit test for a floated image, the selection rectangle of an inline one): no glyph
  can affect it.
- `pathological_inputs_stay_linear` (crates/core/tests/fuzz.rs) failed on the runner for the case
  "unmatched fences among dollars" (10 ms, then 93 ms on 4x the input, bound 8x). Measured on a
  development machine (release, 256 KiB to 8 MiB, minima of five runs) the parser is linear for
  every family of the test: the slowest case, "unmatched fences among dollars", grows as n^1.13
  (4x the input costs 5.5x; 32x costs 51x, where quadratic code would cost 1024x), and the time per
  byte is flat from 1 MiB on; the rise is the output outgrowing the CPU caches, and a 6 ms
  baseline is noise on a shared runner. The test now compares 1 MiB with 4 MiB (20 to 90 ms
  baselines), warms both up, takes minima of several rounds, retries a failure alone, and bounds
  the ratio at 10x; quadratic code gives 16x. `scaling_holds_at_the_file_limit` still compares
  256 KiB with 4 MiB.
- Fonts: the runner has Noto Sans (installed by `ci.yml`) where the development machines have
  Segoe UI, so the screenshots differ from theirs and the smoke test compares nothing.

Windows (windows-latest): all tests, clippy with `-D warnings` and `cargo fmt` are clean, and all
70 screenshots that `tests/shots.sh` always takes were produced (it takes 3 more where `fc-list`
lists DejaVu Serif, which Windows does not).
