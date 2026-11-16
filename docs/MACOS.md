# A macOS version: plan and progress (stages 1 to 4 done on runners, 2026-11-16)

A plan for porting mdview and mdedit to macOS, and below it where the work stands
(**Progress**). The plan rests on a read of all three repositories on 2026-11-15 (shared crates at
`d955a48`, mdview 0.5.0, mdedit 0.7.0). The usual rule holds: nothing counts as working on macOS
until it has run on a Mac, and the owner approves the interface by looking at it.

## Where the code stood (before the port, 2026-11-15)

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

**Stages 1 to 4 done on GitHub's macOS runners (2026-11-16)**, built on the branch `f/macos` from
`main` (mdview 0.5.1, mdedit 0.7.1) and merged into `main` in all three repositories (shared
`b3e04ee`). **The owner's first Mac session (2026-11-16) passed**: the CI bundles of both apps
launch; trackpad scrolling, Retina sharpness, opening from Finder, the shortcuts, Turkish typing
and the look (title bar buttons, icon, dark mode, fonts) are all approved. Afterwards our winit
fork (`md-0.30.13`, `62fa727`) passes on text inserted without a key press (Emoji & Symbols,
dictation), which winit dropped; that is not yet tried on the Mac.

### How it is checked without a Mac

- CI (`ci.yml`): a `macos` job on `macos-15` (Apple Silicon) and `macos-15-intel` (`macos-13`
  is retired) in all three repositories: fmt, clippy, tests, both apps' screenshot suites, and
  the bundle zipped as an artifact (`mdview-macos-aarch64.zip`, `mdedit-macos-x86_64.zip`, ...).
- `mac-look.yml` (both apps, Apple Silicon): builds the bundles and runs them in a real window
  on the runner, started through LaunchServices as Finder starts them, driven through System
  Events (`tests/mac-*.sh`). The pictures are printed into the log as base64 as well as uploaded,
  because a cloud session cannot download artifacts: `gh run view <id> --log |
  tools/mac-look-decode.sh <dir>` writes them back.
- What those runs have shown working (pass lines in the logs, pictures looked at):
  typing; a dead key (Option+E, then E) arriving as "é" through the input method; Command+S,
  Command+Z, Command+A/X/V (the menu bar's Edit items leave them to the window); Command+Left
  and Option+Right; Command+O opening the system's open panel from the helper thread and Escape
  closing it; Command+Q with unsaved changes asking first; Command+E in the viewer starting
  `mdedit.app` from beside its own bundle on the file; `open -a` reaching the running copy; the
  native title bar with the tabs under the system's buttons; SF and Menlo; Mac shortcut labels
  in the menus.

### What was built

- Stage 1: see the commits of 2026-11-15 (Linux-only crates gated, `open`, dark mode from
  `AppleInterfaceStyle`, the system libcurl, `~/Library/Application Support`, FSEvents, names
  without case, update platforms read from `<name>-update-macos.txt`, the pasteboard).
- Stage 2: the window keeps the system's frame with a transparent full-size title bar
  (`window::identify`, `chrome::NATIVE_FRAME`); greyscale text; `system-ui` (SF) and Helvetica
  Neue for the interface and the page, SF Mono or Menlo for code; input methods on, with
  `Ime::Commit` typed and the candidate window at the editor's caret. The dialogs needed no
  change: rfd runs them on the main thread itself.
- Stage 3: Command is the primary modifier (`Mods::ctrl`; the Control key is `Mods::control`);
  `keys::for_platform` moves what the Mac keeps (Control+Tab for tabs, Option+Command+F for
  Replace, Option+Escape for completion, Command+[ and ] for Back and Forward);
  `keys::text_key` gives the Mac's text keys; Option types; labels in the Mac's symbols; no
  access-key underlines; a column selection is Command+Option+Shift+arrows; Control and a click
  is a right click.
- Stage 4: `packaging/macos/make-bundle.sh` (Info.plist with the document types, macOS 12, the
  icon from `assets/<name>-macos.png`, the ad-hoc signature); files from Finder through the
  open-documents event (`open_documents`); a menu bar whose Quit is the app's Exit
  (`menu_bar`; winit's ended the process at once); the other app as a bundle beside this one or
  in the Applications folders; side by side within the screen's visible frame; "Show in
  Finder"; the editor's memory and CPU figures.
- Mac extras the owner asked for: trackpad pinch zooms, the smart-zoom tap is actual size; the
  title row's double click follows the system setting; opened files go into the system's recent
  documents; the unsaved-changes dot in the editor's close button; Window, Enter Full Screen
  (Control+Command+F); the double-click interval, caret blink and Reduce Motion from the system.

### The owner's first Mac session (done, 2026-11-16)

Checked and approved by the owner on their Mac, with the CI bundles: both apps launch (the
first-open step for an unsigned download included); trackpad scrolling; sharp text on Retina;
opening from Finder; the shortcuts; Turkish typing; the look (the system's three buttons beside
the 40 px tab row, the icon, dark mode, the fonts).

- Known gaps: text inserted without a key press (Edit, Emoji & Symbols; dictation) did not
  arrive, because winit only passed inserted text on when it ended a composition; our fork now
  sends it as `Ime::Commit` (`view.rs`, `insert_text`, `62fa727`), to try on the Mac. No
  preedit (the underlined text while composing) is drawn. The update bar does not yet open the
  download page on a Mac. Input methods are on for macOS only; Windows and Linux would gain from
  them too.
- The bundles to try: the `mdview-macos-*.zip` and `mdedit-macos-*.zip` artifacts of the latest
  CI run on `main` (open from the Actions page; right-click, Open, the first time).

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
