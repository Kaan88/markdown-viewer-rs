# mdview AppImage

Status: scripts written; AppDir assembly tested, the `linuxdeploy` step has NOT been run.

## Producing a release

1. Needs: Rust, Qt 6 (`qmake6`), clang, `linuxdeploy` and `linuxdeploy-plugin-qt` on PATH, `zsyncmake`.
   The script never downloads tools on its own. Without them it prints the official URLs
   (`github.com/linuxdeploy/linuxdeploy`, `.../linuxdeploy-plugin-qt`, `continuous` releases) and
   exits 1. `--fetch-tools` downloads them into `tools/` (git-ignored) and checks `tools.sha256`;
   the hashes there are `TODO`, so fill them in after inspecting the files yourself.
2. `./build-appimage.sh` builds `mdview-engine` and the Qt shell as `packaging/arch/PKGBUILD` does,
   assembles `out/AppDir` (`usr/bin/mdview`, `usr/bin/mdview-engine`, desktop file, icon), runs
   `linuxdeploy --plugin qt --output appimage` with
   `LDAI_UPDATE_INFORMATION="gh-releases-zsync|Kaan88|markdown-viewer-rs|latest|mdview-*x86_64.AppImage.zsync"`
   and writes into `out/`: `mdview-<version>-x86_64.AppImage`, `.zsync`, `SHA256SUMS`.
   `--appdir-only` stops after assembling the AppDir.
3. Publish all three files as assets of one GitHub release (tag it, mark it "latest"). The update
   string looks for the `.zsync` asset of the latest release; the `.zsync` points at the AppImage
   asset by name, so keep the names unchanged.

The icon (`mdview.svg`) is a placeholder.

`LDAI_UPDATE_INFORMATION` was checked against the linuxdeploy-plugin-appimage README on GitHub
(it also documents `LDAI_OUTPUT`, `LDAI_VERSION`, and says it generates the `.zsync`).

The engine is found at runtime beside the executable (`crates/render/src/engine.rs` searches the
directory of `current_exe()`), i.e. `usr/bin`, so nothing sets `MDV_ENGINE`. `QT_QPA_PLATFORM` is not set.

## Updating (users)

    AppImageUpdate mdview-x86_64.AppImage              # delta update (zsync)
    appimageupdatetool --check-for-update mdview-*.AppImage   # exit 1 = update available

Or download the new AppImage by hand and compare with `SHA256SUMS`.

## Shell integration (not implemented yet)

The shell must read the `APPIMAGE` environment variable (set by the AppImage runtime to the absolute
path of the running file). Unset means not an AppImage: do nothing. When set, at most once per 24 h and
unless the user opted out, spawn (off the UI thread):

    packaging/appimage/update-check.sh          # check, 24 h stamp in ${XDG_STATE_HOME:-~/.local/state}/mdview/
    packaging/appimage/update-check.sh --apply  # on user consent

The helper is not shipped inside the AppImage yet; the Rust side should either bundle it into
`usr/bin` (add an `install` line to the script) or call `appimageupdatetool --check-for-update
"$APPIMAGE"` directly (exit 1 = update available, 0 = current) and keep its own stamp.
The helper prints one line: `update-available`, `up-to-date`, `skipped (checked recently)`,
`disabled`, `no-tool`, `not-appimage`, `error (<code>)`, `updated`, `update-failed (<code>)`.
Opt-out: `MDVIEW_NO_UPDATE_CHECK=1` or the file `${XDG_CONFIG_HOME:-~/.config}/mdview/no-update-check`;
the Rust side should map its settings toggle onto that file.
