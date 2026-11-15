# A macOS version: plan (2026-11-15; stage 1 under way on the runners)

A plan for porting mdview and mdedit to macOS. Nothing here is built, and nothing has been
compiled for macOS. It rests on a read of all three repositories on 2026-11-15 (shared crates at
`d955a48`, mdview 0.5.0, mdedit 0.7.0). The usual rule holds: nothing counts as working on macOS
until it has run on a Mac, and the owner approves the interface by looking at it.

## Where the code stands

- `target_os = "macos"` appears nowhere. Code is split into "Windows" and "not Windows", and most
  of the "not Windows" side assumes Linux (Wayland and X11, D-Bus, XDG folders, AppImage).
- The base is portable: winit 0.30, softbuffer, parley (which finds fonts through CoreText on
  macOS), arboard, rfd, syntect, the renderer and the engine have no Linux-only code of their own.
- CI and releases are x86_64 Windows and Linux only.

## Where to start from

On 2026-11-15 the released state is `main` in all three repositories (mdview 0.5.0, mdedit 0.7.0,
shared crates `065cbdc`). Fixes from the hands-on tests of that day are on the branch
`f/control-fixes` in each repository (shared `d955a48`, with both apps pinned to it), verified in
real windows on Windows and green in CI, but not merged to `main` and not released. They touch
key handling (`event_key`), the resize band and the window procedure, which the port also
touches: check whether they have been merged, and if not, build the port on top of them.

## Progress

**Stage 1, 2026-11-15: on the runners**, on the branch `f/macos` in all three repositories,
built on `main` (mdview 0.5.1, mdedit 0.7.1; the control fixes are merged there). The shared
crates' branch is pinned by both apps. Nothing has been run on the owner's Mac, and no window
has been opened on a Mac yet.

- CI has a `macos` job on `macos-15` (Apple Silicon) and `macos-15-intel` (`macos-13`, named
  above, is retired). Build, clippy and tests in all three; the screenshot suite of both apps
  runs there and its pictures are artifacts of the run. They cannot be downloaded into a cloud
  session (the artifact host is refused by its network), so they are looked at from the run's
  page in a browser.
- What changed: see the commit messages on `f/macos`. In short: zbus and the portal only on
  Linux; `open`/`open -R`; dark mode from `AppleInterfaceStyle`; `/usr/lib/libcurl.4.dylib`;
  `~/Library/Application Support`; FSEvents; names without case (`paths::NAMES_IGNORE_CASE`);
  greyscale text; update platforms `macos-aarch64` and `macos-x86_64` read from
  `<name>-update-macos.txt`, the parser skips keys of unknown platforms, no self-update on a Mac;
  the other app found as `<name>.app` beside this bundle or in `/Applications` and
  `~/Applications` (from stage 4, as it was small); macOS font fallbacks; the editor pastes a
  picture (PNG, or TIFF made PNG) and copied files through `NSPasteboard`; the key without
  modifiers is read on macOS too.
- State of the runs: all three build and pass clippy on both Macs, and both apps' screenshot
  suites run there. The last test failures found there are fixed in the latest commits, whose
  runs were not finished when this was written: see the Actions page of each repository.
- Found on the runners: FSEvents keeps a file's "created" flag on later events for it, so a
  write into a new file woke a listed folder; on macOS a listing event now wakes only when the
  folder's own modification time moved. Two viewer tests held on one font's pixels and failed
  in Helvetica's metrics; they no longer do.
- Found in the code, for stage 2: rfd already runs its panels on the main thread when called
  from a helper thread while the app is running (`dispatch2::run_on_main`), so the dialogs may
  need no change. To be seen in a real window.
- Not done in stage 1: the page and interface font stacks still name no Mac family (they reach
  the system's sans-serif and monospace); the editor's memory and CPU read-outs; the update bar
  opening the download page; everything of stages 2 to 6.

## Decisions (the owner, 2026-11-15)

1. **A Mac exists** for the hands-on stages, but as much as possible is to be done on GitHub's
   macOS runners first, so the Mac is needed late and seldom.
2. **No signing.** No Developer account, no notarisation. Apple Silicon builds get the ad-hoc
   signature the system insists on (free, made at build time). The download page says how to
   open an unsigned app the first time.
3. **Native title bar**: transparent, with the system's close, minimise and zoom buttons on the
   left and our tab row drawn beneath them.
4. **Two builds**, Apple Silicon and Intel, not one universal binary. macOS 12 or later.
5. **Updates**: the bar says a new version exists and opens the download page. No self-update.

## What the runners can do, and how the work leans on them

GitHub's macOS runners are real Macs with a logged-in desktop session, so they can do far more
than compile. Set up in this order, each step widening what is checked without the owner's Mac:

1. **Build, clippy, unit tests** on both architectures (stage 1).
2. **The screenshot suite** (`tests/shots.sh`): it draws off screen and needs no window, so it
   shows fonts, text smoothing and layout on macOS. The pictures are uploaded as artifacts of the
   run and looked at from here.
3. **The app in a real window on the runner**: a script starts the bundle, waits for its window,
   takes pictures with `screencapture`, and drives it with synthetic keys and clicks (`osascript`
   or a small CGEvent helper; the runner's session permits it, and if a permission prompt blocks
   it, the app's own `--shot` and trace flags still cover what was drawn). This is where the
   native title bar, the dialogs, typing accents, Cmd shortcuts, opening a file through
   `open -a`, and the two apps talking to each other are first seen working.
4. **Bundles as artifacts** of every run on the port's branch: `mdview.app` and `mdedit.app` for
   both architectures, zipped. When the owner does sit at the Mac, there is always a current
   build to download, with nothing to compile there.

What the runners cannot show: how it feels (trackpad scrolling and pinch, pointer latency, a
Retina screen, since the runner's display is not one), the first-open warning for an unsigned
download as a user meets it, and anything about the owner's own setup. Those are the Mac
sessions, and the plan keeps them to two: one after stage 4, one before the release.

## Stage 1: it compiles (runners only)

A macOS job in each CI workflow, on both runner kinds (`macos-14` or later for Apple Silicon,
`macos-13` for Intel): build, clippy and unit tests.

- Gate `zbus` to Linux. It is the one Linux-only crate pulled in for every non-Windows target.
- Split the `not(windows)` halves into Linux and macOS where they assume Linux:
  - opening links and files: `open` in place of `xdg-open`; reveal in Finder with `open -R`;
  - dark mode: winit's theme and `ThemeChanged` in place of the D-Bus portal;
  - downloads: load `libcurl.4.dylib` (it ships with macOS) in place of the `.so` names. As
    written, every download would fail on a Mac;
  - file watching: turn on notify's FSEvents backend;
  - data folder: `~/Library/Application Support/<app>` in place of `~/.local/share`;
  - file-name comparison: case-insensitive, as APFS is by default;
  - editor only: pasting a picture and pasting copied files go through `wl-paste` and `xclip`
    today; they need the macOS pasteboard (arboard's image support, or `NSPasteboard`);
  - the desktop-entry, AppImage, KWin, shadow, blur and snap code stays Linux or Windows only.
- The update code learns the platform names `macos-aarch64` and `macos-x86_64`.

## Stage 2: first run (on a runner, in a real window)

- **Window**: the chrome of decision 3; the system gives the shadow, the rounded corners,
  resizing and full screen. Check softbuffer's speed and sharpness on a Retina screen (scale 2).
- **Text**: greyscale smoothing, not the RGB subpixel smoothing used off Windows today. Font
  stacks get the system fonts: SF Pro for the interface and the page, Menlo or SF Mono for code,
  Apple Color Emoji, PingFang for Han. The engine's fallbacks and the "Segoe UI" it writes into
  Mermaid pictures need the same.
- **Dialogs**: on macOS the open and save panels must run on the main thread; the non-Windows
  code runs them on a helper thread today.
- **Typing accents**: there is no IME handling on any platform. On a Mac this is not optional:
  Option+E then E, and press-and-hold for accents, arrive through the IME path. The shared text
  field and the editor's text area need `Ime::Commit` at least (preedit display can follow).
  This also helps Windows and Linux users who type Chinese, Japanese or Korean.

## Stage 3: keyboard

The largest single change. Ctrl is hard-coded: the modifier type has no Cmd, about 30 handlers
test `mods.ctrl` directly, and 64 shortcut labels are literal "Ctrl+..." strings.

- A "primary" modifier in the shared key code: Ctrl on Windows and Linux, Cmd on macOS. Shortcut
  tables are written once against it; labels are printed per platform (the Mac symbols).
- Mac conventions: Cmd+Q, Cmd+W, Cmd+comma for Settings, Cmd+arrows for line and document ends,
  Option+arrows by word, Option+Backspace to delete a word. Shortcuts that clash with the system
  (Cmd+H, Cmd+M, Cmd+Space, Ctrl+arrows) get other keys.
- Access keys (the underlined letters) are not a Mac idea: off there.

## Stage 4: fitting into macOS

- **App bundle**: `mdview.app` and `mdedit.app` with an `Info.plist` that declares the document
  types (Markdown; for mdedit also plain text and source files), an `.icns` icon, and for the
  viewer the engine inside the bundle.
- **Opening files from Finder**: macOS hands files to a running app as an open-document event,
  not as arguments. winit 0.30 has no API for it, so this needs a small addition to our winit
  fork or a delegate hook of our own. Until it exists, double-clicking a file opens the app
  without the file. It also replaces most of the single-instance port: macOS already runs one
  copy of a bundle.
- **Menu bar**: a Mac app is expected to have one. At least the application menu (About,
  Settings, Quit) and standard Edit and Window menus, which also make Cmd+C and friends work in
  system dialogs. Our own "..." menu can stay inside the window.
- **The two apps together**: the hand-off over a local port is portable. The rules for finding
  the other app need a Mac rule (the other bundle in `/Applications` or beside this one, or by
  bundle identifier). Side-by-side placement uses the plain winit path; the usable screen area
  must come from the system so the menu bar and the Dock are left free.
- **Small things**: "Show in Finder" as the label, the default line ending, the editor's memory
  and CPU read-outs (they return nothing on a Mac today).

## Stage 5: packaging and release

- A build script per app and architecture: bundle, ad-hoc signature, a `.dmg` or zip. The file
  names carry `macos-aarch64` and `macos-x86_64`.
- A macOS job in `release.yml` on a GitHub macOS runner.
- **Update manifest, a trap**: every released build rejects a manifest that has a key it does
  not know. Adding `macos-...` keys to `mdview-update.txt` or `mdedit-update.txt` would stop
  update checks in all Windows and Linux installs up to 0.5.0 and 0.7.0. The Mac builds
  therefore read their own manifest file (for example `mdview-update-macos.txt`), and the parser
  is made tolerant of unknown keys for the future.
- Later, optional: a Homebrew cask.

## Stage 6: hands-on test and the owner's look

The same pass as the Windows one of 2026-11-15: every feature with real mouse and keyboard, on
the Mac, then the owner looks at it. Trackpad gestures (pinch to zoom, smooth scrolling with
momentum) are part of this pass; they do not exist on the other platforms.

## Order and what blocks what

Stage 1 can start at any time. Stages 2 to 4 are where the work is, done against the runners
as described above, with the owner's Mac at the end of stage 4; within them the keyboard change (stage 3) and typing accents (stage 2) touch
the shared crates most and should land first, since Windows and Linux builds run through the
same code and must be re-tested. No stage changes what
Windows and Linux users see, except the IME work, which is a gain there too.

## Not planned in a first Mac version

Touch Bar, Services, Quick Look previews, iCloud documents, the Mac App Store (its sandbox would
need changes to how the two apps find each other and to self-update), native printing.
