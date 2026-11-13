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
  closed; and **Check at launch: on/off**. They are in About and not in the main menu because two
  more rows did not fit the menu in the default 800 px window.

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
| shared `mdshared-ui/src/notice.rs` | `draw_update_bar`: the bar, its button and its close button |
| `bins/mdv-app/src/updater.rs` | `APP`: mdview's name, version, manifest address, engine name and installed-apps key |
| `bins/mdv-app/src/workspace.rs` ("updates") | Holds the `Updates`, remembers the time of the last look in the session |
| `bins/mdv-app/src/page.rs` `draw_update_bar`, `overlays.rs` (About) | Where the bar and the two buttons are placed and hit |
| `bins/mdv-app/src/app/commands.rs` | The threads (`mdview-update`) and their `Wake::Update` answers |

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

1. Set the version in `[workspace.package]` of `Cargo.toml`.
2. `packaging\windows\build-installer.ps1 -Notes 'One line.'` builds the static executables and
   the installer, and fills `target\release-files\` with `mdview.exe`, `mdview-engine.exe`,
   `mdview-setup-<version>.exe` and `mdview-update.txt`, whose URLs point at tag `v<version>`.
   A test (`the_release_script_writes_a_manifest_this_build_reads`) checks that manifest and the
   two hashes when the folder exists.
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

Nothing. `Platform::LinuxX64` exists in the manifest grammar, `fetch_file` fails on the stub
transport, and a running ELF binary can be replaced by rename as well, so the same flow should
apply once there is a Linux build and a transport.
