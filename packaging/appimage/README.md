# mdview AppImage

Status: built and run. The supported way is `build-in-container.sh TOOLS_DIR OUT_DIR`, which builds in
`ubuntu:24.04` so the image needs only glibc 2.39 (building on Arch ties it to Arch's newer glibc).
`TOOLS_DIR` holds the three linuxdeploy files named in `tools.sha256`; check them with
`grep -v '^#' tools.sha256 | (cd TOOLS_DIR && sha256sum -c -)`. Keep `OUT_DIR` outside any temp folder.
Not done yet: a published GitHub release, so the update path has never run against a real release.

## Producing a release

1. Needs: Rust, Qt 6 (`qmake6`), clang, `linuxdeploy` and `linuxdeploy-plugin-qt` on PATH, `zsyncmake`.
   The script never downloads tools on its own. Without them it prints the official URLs
   (`github.com/linuxdeploy/linuxdeploy`, `.../linuxdeploy-plugin-qt`, `continuous` releases) and
   exits 1. `--fetch-tools` downloads them into `tools/` (git-ignored) and checks `tools.sha256`;
   the hashes there are the files the 2026-11-10 builds used; upstream rebuilds `continuous`, so a
   new download can differ and is refused until its hash is reviewed and recorded.
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

## Shell integration

`bins/mdv-qt/src/appimage_update.rs` calls `appimageupdatetool` directly when `APPIMAGE` is set and the
tool is on `PATH`; `update-check.sh` is not used by the shell. Help > Update mdview now applies a
delta. Opt-out: `MDVIEW_NO_UPDATE_CHECK=1` or `${XDG_CONFIG_HOME:-~/.config}/mdview/no-update-check`.

**Decided 2026-11-10: update checks are manual only.** The shell still checks once a day at launch;
that automatic check is to be removed, leaving a Help menu item that checks and then offers to apply
(see `docs/HANDOFF.md` section 12).
