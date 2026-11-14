# Updates

mdview updates itself in place: it looks for a newer version, asks, downloads the two executables,
checks their hashes and swaps them in. Built 2026-11-13 after the user chose "at launch, then ask"
and "both exes, swapped in place". v0.1.0, the first release, was published the same day, so a look finds
its manifest. v0.2.0 followed on 2026-11-13, and **the user marked the update as working** the
same day. See "What has run" at the end.

## What the reader sees

- **At launch**, every launch (0.2.2; 0.2.1 and older looked at most once a day), an installed mdview fetches a small manifest from GitHub on a
  background thread. A failure is silent. Nothing else is sent or fetched.
- If the manifest names a newer version, a bar appears over the page: "mdview 0.2.0 is available."
  with the release's one line of notes, an **Update** button and a close button. Nothing downloads
  until Update is pressed.
- While it downloads, the bar says so and has no buttons. Then: "mdview 0.2.0 is installed. It
  starts the next time you open mdview." The running window stays on the old version.
- **About mdview** holds the controls: **Check for updates** (looks now and always answers: the bar, "mdview x is the newest version." or "Could not look for a
  newer version."), which reads **Update to 0.2.0** when a version was found and its bar was
  closed; and **Check at launch: on/off**. They are in About, and the same two are in **Settings**
  (**Check for updates at launch** and **Check now**); they are not in the main menu.

Settings (session file): `check_updates=0|1` (default 1) and `update_checked=<seconds since 1970>`.

The automatic look does not run for a launch with `--no-session` or `--shot`, for a copy that has
no `mdview-engine` beside it, or for a cargo build (an exe in a folder named `release`, `debug` or
`deps`). Such a copy can still look when asked, but pressing Update says "Only an installed mdview
can update itself."

## Code

| Where | What |
|---|---|
The mechanism lives in `Kaan88/markdown-shared-rs`, so the editor updates itself with the same
code; mdview supplies its name, manifest address and file names.

| Where | What |
|---|---|
| shared `mdshared-app/src/update.rs` | Version ordering, the manifest parser, SHA-256. Pure: no I/O beyond hashing a file it is handed. `mdview_core::update` re-exports it |
| shared `mdshared-app/src/selfupdate.rs` | The swap: `<name>.new` in, the running file aside to `<name>.old`, all files or none, and the clean-up of what an earlier update left. `mdview_core::selfupdate` re-exports it |
| shared `mdshared-app/src/updater.rs` | `App` (what names an application), `check`, `install`, `installed_files`, `record_version`, `check_due`, and `Updates`: the offer, installing, installed, bar closed, and every message |
| shared `mdshared-fetch` `fetch_file` | One blocking https GET with the caller's size cap and deadline, on the image fetcher's transport |
| shared `mdshared-app/src/relaunch.rs` | Restart after an update: `target` (what to start: the installed file; none for a copy that cannot update), `spawn` (with `--restarted`), `wait_for_predecessor` (the new process waits until the old instance's port and lock are gone, before the single-instance hand-over) |
| shared `mdshared-ui/src/notice.rs` | `draw_update_bar`: the bar, its button (Restart once installed) and its close button |
| `bins/mdv-app/src/updater.rs` | `APP`: mdview's name, version, manifest address, engine name and installed-apps key |
| `bins/mdv-app/src/workspace.rs` ("updates") | Holds the `Updates`, remembers the time of the last look in the session |
| `bins/mdv-app/src/page.rs` `draw_update_bar`, `overlays.rs` (About) | Where the bar and the two buttons are placed and hit |
| `bins/mdv-app/src/app/commands.rs` | The threads (`mdview-update`) and their `Wake::Update` answers; `restart_into_update` sets `App::restart` and exit, and `run()` starts the new file after the window is dropped |

## Manifest

A release asset named `mdview-update.txt` on the latest GitHub release of
`Kaan88/markdown-viewer-rs`, fetched from
`https://github.com/Kaan88/markdown-viewer-rs/releases/latest/download/mdview-update.txt`.
The grammar and limits are in the module docs of `mdshared-app/src/update.rs`; in short:

```text
manifest: 1
version: 0.2.0
notes: One line of release notes.
windows-x86_64.asset: mdview.exe
windows-x86_64.url: https://github.com/Kaan88/markdown-viewer-rs/releases/download/v0.2.0/mdview.exe
windows-x86_64.sha256: <64 hex digits>
windows-x86_64.engine.url: https://github.com/Kaan88/markdown-viewer-rs/releases/download/v0.2.0/mdview-engine.exe
windows-x86_64.engine.sha256: <64 hex digits>
```

Limits: 8 KiB, 64 lines, https URLs only (at most 512 bytes, no userinfo), 64-hex SHA-256, asset
names limited to `[A-Za-z0-9._-]`, unknown or duplicate keys reject the whole manifest. The
`engine.*` pair is both or neither; a platform entry without it is not offered, because an
in-place update needs both files. A pre-release version is offered only to a build that is older
by SemVer precedence.

**Integrity.** The binaries are not code-signed and will not be (the user's decision), so the
SHA-256 in the manifest is the only integrity check, and the manifest's own authenticity rests on
TLS to GitHub. A download that does not match its hash is discarded and nothing is replaced.

## Download and swap

1. Each file is fetched whole into memory (cap 128 MiB, 10 minutes for the pair), with the fixed
   `User-Agent: mdview`, `Accept: */*`, no cookies or credentials, at most 5 redirects followed by
   hand and never from https down to http. GitHub redirects a release asset to its storage host.
2. Its SHA-256 is compared with the manifest's. On a mismatch, or any failed download, the files
   already written are removed and the bar goes back to offering the update.
3. A matching file is written beside its target as `mdview.exe.new` / `mdview-engine.exe.new`.
4. When both are there: `mdview.exe` is renamed to `mdview.exe.old` and the new file renamed into
   its place, then the same for the engine. Windows allows a running executable to be renamed,
   though not overwritten or deleted. If the second file fails, the first is put back.
5. The version in Windows' installed-apps entry (the installer's `AppId` key under `HKCU`) is
   rewritten through `reg.exe`, if the entry exists.
6. The next launch deletes the `.old` files, on its updater thread.

A window of the old version that is still open when a second update arrives keeps `mdview.exe.old`
locked; that update fails cleanly and can be tried again once the old window is closed.

## Publishing a release

**The way to publish is the tag-driven workflow** (`.github/workflows/release.yml`): set the
version, push, wait for the `ci` run, push the tag `v<version>`. What it builds, the asset names
and the dry run are in `docs/CI.md`. The local scripts below (the Windows procedure here, and
"Releasing from Linux") are the dry path: for trying a release and for testing, not what users
download. The published Windows files are built natively by the workflow.

The Windows procedure with the local script:

1. Set the version in `[workspace.package]` of `Cargo.toml`.
2. `packaging\windows\build-installer.ps1 -Notes 'One line.'` builds the static executables and
   the installer, and fills `target\release-files\` with `mdview.exe`, `mdview-engine.exe`,
   `mdview-setup-<version>.exe` and `mdview-update.txt`, whose URLs point at tag `v<version>`.
   A test (`the_release_scripts_write_a_manifest_this_build_reads`) checks that manifest and the
   file hashes when the folder exists (also `target/dist-cross` and `target/release-files-all`;
   it runs on Windows and Linux).
3. Create the GitHub release with tag `v<version>` and upload those four files. It must be the
   latest release, not a pre-release, for `releases/latest/download/` to serve its manifest.

To try a release before it is the latest, point a build at its manifest:
`MDVIEW_UPDATE_MANIFEST=https://…/mdview-update.txt`. `mdview --check-update` prints what a look
finds and exits, with no window.

## What has run (2026-11-13, the Windows PC)

- Unit tests: the manifest's engine keys; the swap, its rollback, and swapping a running
  executable; `install` with a stand-in transport (a damaged file, a failed download, a good
  pair); the workspace's stages and saved settings; the bar and the About buttons in a headless
  `App`.
- `mdview --check-update` against GitHub. Before the release: `Status(404)` for the manifest, a
  redirected download of another file from the repository that then failed to parse as a
  manifest, and an `http` URL refused. After v0.1.0 was published: the manifest fetched through
  GitHub's redirect and read, answer "nothing newer". The two published executables, downloaded
  with curl, match the hashes in the published manifest.
- `--shot` renders of the bar in its three stages and of the About dialog, looked at by Claude.
- After v0.2.0 was published: the published manifest says 0.2.0; the two published executables
  match its hashes; the published 0.1.0 `mdview.exe`, downloaded and run with `--check-update`,
  answers "mdview 0.2.0 is available" with both addresses; the 0.2.0 build answers "nothing
  newer".
- **The update itself: working, on the user's word (2026-11-13).** Claude did not watch it; the
  user said to mark it as working after 0.2.0 was out. Which of the steps the user saw (the bar,
  the download, the swap, the version in Windows' list of installed apps, the clean-up of
  `.old` on the next launch) is not recorded.
- After v0.2.1 was published (2026-11-13): the published manifest, read through
  `releases/latest/download/`, says 0.2.1 with both URLs at tag v0.2.1; the `mdview.exe` and
  `mdview-engine.exe` downloaded from those URLs match the two hashes in it; the four assets are
  `mdview.exe` 8363520 bytes, `mdview-engine.exe` 26289152, `mdview-setup-0.2.1.exe` 11592884 and
  `mdview-update.txt` 524. `mdview --check-update` run from the 0.2.1 build answers "nothing
  newer" with exit code 0. An install of 0.2.0 being offered and installing 0.2.1 was not tried.

- After v0.2.2 was published (2026-11-13): the live manifest says 0.2.2; the two downloaded
  executables match its hashes; `mdview --check-update` from the 0.2.2 build answers "nothing
  newer" and from the published 0.2.1 build "mdview 0.2.2 is available" with both addresses. A
  launch of the 0.2.2 build (copied out of `target`, own settings file through `MDVIEW_SETTINGS`)
  with `update_checked` one minute old looked at once: the value moved to now. The same launch
  of the published 0.2.1 build left it unchanged, as it should: copies of 0.2.1 and older still
  look once a day, so they find 0.2.2 at launch only when their day is up, or by the manual
  check. An install of 0.2.1 updating itself to 0.2.2 was not tried.

## Linux

Updating in place works only inside an AppImage (2026-11-14, shared `84de734`). `installed_files`
returns the canonical `$APPIMAGE` when it is a file in a writable folder, with no engine: the
engine is inside the AppImage (`usr/bin/mdview-engine` beside `usr/bin/mdview`, which
`engine_path()` finds from `current_exe()`), so a Linux release lists no `engine.*` keys and the
update replaces the one AppImage (`<name>.new` staged with mode 0755, the old one aside as
`.old`). A plain binary or a cargo build updates nothing. The transport is the system libcurl,
loaded at run time; `mdview --check-update` reached GitHub from the 0.2.3 build on Linux ("nothing
newer"). The Linux manifest entry (`linux-x86_64.asset/url/sha256`) is written by
`packaging/write-manifest.sh` (see below); an update of a real AppImage from a published release
has not run. The release test (`the_release_scripts_write_a_manifest_this_build_reads`) runs on
both systems.

## Releasing from Linux

Added 2026-11-14; checked against the scripts on the same day for 0.3.0. Both platforms' files can
be built on a Linux PC with Docker, `appimagetool` (with `mksquashfs`), `curl`, `wine`, the Inno
Setup folder from a Windows install, and `python3` with `pefile` for the installer's icon step
below. Publishing needs `gh`. `build-installer.ps1` stays the way to build on Windows; the two make
the same Windows file names.

### Commands

```
packaging/linux/build-appimage.sh                        # target/dist-linux/mdview-<ver>-linux-x86_64.AppImage
packaging/windows-cross/build.sh --notes 'One line.'     # target/dist-cross/: both exes, the setup, the manifest
packaging/write-manifest.sh --version V [--notes N] --out DIR \
    --windows-exe DIR/mdview.exe --windows-engine DIR/mdview-engine.exe --linux DIR/<AppImage>
packaging/release-all.sh --notes 'One line.'             # everything into target/release-files-all/
packaging/release-all.sh --notes '...' --notes-file FILE --publish    # also: gh release create v<ver> ... --latest
packaging/wine-compare.sh NATIVE_DIR CROSS_DIR           # tests/shots.sh under Wine with both builds, cmp the PNGs
```

`release-all.sh` reads the version from `[workspace.package]` of `Cargo.toml`, refuses a dirty tree
(changes under `docs/` excepted) and a HEAD no remote branch contains (it runs `git fetch` first).
It runs the AppImage script, then `packaging/windows-cross/build.sh`, copies the four files into
`target/release-files-all/` (the intermediate outputs stay in `target/dist-linux/` and
`target/dist-cross/`), and writes the one manifest there with `packaging/write-manifest.sh`, which
re-reads it and checks every hash against the file it names. `--notes` is the manifest's line
(optional, as in the PowerShell script; at most 1024 bytes, one line), `--notes-file` the GitHub
release text. `--publish --dry-run` prints the `gh` command instead of
running it. The release has five assets: the AppImage, `mdview.exe`, `mdview-engine.exe`, the
setup and the manifest. The manifest carries the Windows entry with its engine keys and the Linux
entry without any: the engine is inside the AppImage.

### What is built how

- **AppImage.** `packaging/linux/Dockerfile` (Ubuntu 20.04, glibc 2.31, rustup stable) builds
  `cargo build --release --locked -p mdview -p mdview-engine` from the repository mounted
  read-only. The script checks that `objdump -T` needs no glibc symbol newer than 2.31 (0.2.3:
  2.30), assembles `AppDir` (`usr/bin/mdview` and `usr/bin/mdview-engine`, `mdview.desktop`,
  `mdview.png`, `.DirIcon`, `AppRun` a symlink to `usr/bin/mdview`) and runs the host's
  `appimagetool` with a pinned runtime (`--runtime-file`; the script fetches that runtime once with
  `curl` and checks its SHA-256, so `appimagetool` never downloads one). Squashfs compression is
  the default (the measurements are in `docs/HANDOFF.md`, section 12).
  `SOURCE_DATE_EPOCH` is the commit time: two runs on one commit give the same AppImage (checked).
  The viewer finds the engine as a sibling of its own executable, which inside the AppImage is
  the file under the mount point, so `MDV_ENGINE` is not needed (checked: `--shot` of
  `fixtures/diagrams.md` from the AppImage draws the diagrams; with `MDV_ENGINE` pointing at
  nothing it shows "Could not be drawn" placeholders).
  Nothing is bundled; the AppImage needs from the host libc, fontconfig, freetype, libX11,
  libX11-xcb and libxcb (a bare `ubuntu:20.04` or `debian:11` container lacks the X11 ones, and
  works once `libx11-6 libx11-xcb1 libxcb1 libfontconfig1 libfreetype6` are installed), plus
  libcurl, wayland and the like loaded at run time if they are there.
- **Windows exes.** `packaging/windows-cross/Dockerfile` (Ubuntu 24.04, rustup stable with the
  `x86_64-pc-windows-msvc` target, `cargo-xwin` 0.23.1, clang, lld, llvm) runs
  `cargo xwin build --release --locked --target x86_64-pc-windows-msvc -p mdview -p mdview-engine`
  with `RUSTFLAGS='-C target-feature=+crt-static'`, as the PowerShell script does. `build.rs` uses
  `rc.exe` when it finds one and otherwise `llvm-rc` (`MD_LLVM_RC`, else PATH), so the icon is
  embedded (the script sets `MD_LLVM_RC=/usr/bin/llvm-rc`).
- **Installer.** `ISCC.exe` from the copied Inno Setup folder runs under Wine on the same
  `packaging\windows\mdview.iss`, in a scratch tree (`target/cross-installer`) laid out the way
  the script expects its inputs. Then `packaging/windows-cross/fix-setup-icons.py` (needs
  `python3` with `pefile`) rewrites the setup's resource tables to drop the stock Inno Setup icons
  that Wine's `UpdateResource` leaves beside ours, so the setup carries only our icon, as one built
  on Windows does. Without `pefile` the script prints a warning and leaves the setup as built.
- **Manifest.** `packaging/write-manifest.sh` writes `windows-x86_64.*` keys (with the two
  `engine.*` keys) and `linux-x86_64.*` keys (no engine keys) for the files it is given; the URLs
  point at tag `v<version>`. `windows-cross/build.sh` alone writes a Windows-only manifest into
  `target/dist-cross/`.

### Licence

`cargo-xwin` downloads the MSVC C runtime and the Windows SDK from Microsoft at its first run.
Using them means accepting Microsoft's licence (https://go.microsoft.com/fwlink/?LinkId=2086102).
cargo-xwin 0.23.1 has no accept flag or variable: it accepts for you when it downloads, so a run
of `packaging/windows-cross/build.sh` is that acceptance. Whoever runs it accepts the licence for
themselves.

### Caches

`$MD_BUILD_CACHE`, default `~/.cache/md-build`: `mdview/cargo` (registry and git checkouts),
`mdview/target-linux` and `mdview/target-win` (cargo target directories), `xwin` (the SDK and CRT,
about 1.2 GB, shared with mdedit's tooling), `innosetup` (a copy of
`/mnt/C/Program Files/Inno Setup 7`; override the source with `INNO_SETUP_DIR`), `wine-prefix` (the
`WINEPREFIX` for ISCC and `wine-compare.sh`, with the Windows fonts copied into
`drive_c/windows/Fonts`) and `runtime-x86_64-20251108` (the AppImage runtime, SHA-256 pinned in
`build-appimage.sh`). The Docker images are `mdview-linux-build` and `mdview-windows-cross`. All of
it can be deleted; the next run rebuilds it.

### What the cross-built Windows files have and have not been checked for

Checked (0.2.3, commit `abcaf87`'s code, on Linux): both exes import the same DLLs as the natively
built ones in `target\release-files` (`mdview.exe` 18 DLLs, `mdview-engine.exe` 4: `+crt-static`
holds, no `vcruntime140.dll`), `mdview.exe` has the same icon resources, and the sizes are within
0.5% (8,404,992 against 8,377,856 for `mdview.exe`, 26,343,936 against 26,289,152 for the engine).
Under Wine 11 with the same prefix and fonts, `tests/shots.sh` run with the native pair and with
the cross pair gave 57 PNGs each, all byte-identical (diagrams and math included, so the engine
works); `--check-update` of both answered "nothing newer".

Not checked: any run on Windows. The cross exes are not byte-identical to the native ones (a
different linker and resource compiler). The cross-built setup was about 11.75 MB against 11.60 MB
for the native one because Wine's `UpdateResource` leaves Inno Setup's stock icons in `Setup.exe`
when ISCC swaps in ours; `fix-setup-icons.py` now removes them (commit `30cb862`; its effect on a
setup run under Windows was not checked). The setup was not clicked through, and every run makes a
different file (Inno stamps the build time), unlike the exes and the AppImage.

The shared pin (branch `feat/linux`, `7cd6461` for 0.3.0, to become `main`) has the Linux transport
and the engine rule, so the two-platform manifest test applies on Linux too.

## v0.3.0 (published 2026-11-14 by the release workflow, from the tag at commit 5e043ba)

Assets: `mdview.exe` (8,630,272 bytes), `mdview-engine.exe`, `mdview-setup-0.3.0.exe`,
`mdview-0.3.0-windows-x86_64.zip`, `mdview-0.3.0-linux-x86_64.AppImage` (15,346,168 bytes),
`mdview-0.3.0-linux-x86_64.tar.gz`, `mdview_0.3.0_amd64.deb`, `mdview-0.3.0-1.x86_64.rpm`,
`PKGBUILD` and `mdview-update.txt`. The Windows files were built natively on a Windows runner.

Verified against the live release: the manifest at the latest-download address names 0.3.0 with a
Windows entry (exe and engine) and a Linux entry without engine keys; the downloaded `mdview.exe`,
`mdview-engine.exe` and AppImage have the manifest's three SHA-256 values; the published
AppImage's `--check-update` answered "nothing newer", exit 0; the published 0.2.2 `mdview.exe`
with its engine, run under Wine with `--check-update`, was offered 0.3.0 with both addresses.

Before publishing: an AppImage built from the same code offered, installed and became a newer
test build end to end on KDE Plasma 6.

Not verified: the Windows files run by hand on a Windows desktop; an installed 0.2.2 updating
itself to 0.3.0 through the bar; the deb, rpm, tar.gz and PKGBUILD of this exact build installed.
