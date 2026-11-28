# mdview

A Rust remake of Kaan88/markdown-viewer-cpp, a Markdown viewer with a custom renderer. The shell is winit + softbuffer with every control hand-drawn (`bins/mdv-app`, binary `mdview`). The earlier shells are retired and archived: Qt Widgets on the `qt-archive` branch, WinUI 3 on `winui-archive`.

Before doing anything, read `docs/HANDOFF.md`: current state, decisions already made, and what to do next. The full plan is `docs/PLAN.md`.

- Commit and push only as phases complete or when asked. There is no CI, so run the tests yourself.
- `spikes/` is throwaway code, kept for reference.
- The user approves the interface by looking at it; screenshots and tests do not stand in for that.
- Report results plainly, including what was not run. Nothing counts as working on Windows until it has run on Windows.

## Using the shared toolkit (`Kaan88/markdown-shared-rs`)

This app is built on the owner's toolkit, pinned by commit in `Cargo.toml` and `bins/mdv-app/Cargo.toml`. Its widgets are pieces
that have to be wired together, and an audit of every app on it (2026-11-27) found each one had
wired some of them halfway: a text field with a caret but no drag, no triple click or no
right-click menu; a menu in English in a Turkish app; drags that stayed held after Alt+Tab.

1. **Look before writing.** Before writing input, menu, caret blink, timer, drag, scroll or
   dialog code, read the toolkit's `docs/CATALOGUE.md` at the pinned commit (its "I want to ..."
   index first). Use what is there. If it almost fits, change it in the toolkit, additively as
   its `AGENTS.md` says, rather than copy it here.
2. **Wire a text field whole.** Use `textfield::FieldInput`, or at a pin older than it
   `textfield::Pointer` with `TextField::context_menu`: every press, move and release, the right
   press, the keys, its deadline, and its menu drawn last. Never `TextField::press` alone: it
   places a caret and nothing else (the toolkit deprecates it from `FieldInput` on). The
   toolkit's settings forms, and at a pin that has them its palette and plan screen, have
   `pointer_press` routes that give their fields the same; use those.
3. **Every held press can be lost.** On Windows winit sends no button release when the window
   loses the mouse in the middle of a drag. On `Focus(false)` (or the host's `Event::PressLost`,
   at a pin that has it) call `cancel` (or `let_go`) on every widget that can hold a press. Do
   not fake it with a release far off screen: a release acts on what is under the pointer.
4. **The edit menu speaks the app's language.** Give it the app's words (`EditLabels`,
   `TextField::context_menu_labelled`, `set_edit_labels`).
5. **Move the pin on purpose.** When it moves, read the toolkit's commits since the old pin for
   deprecations and new opt-ins, act on each warning (do not allow them wholesale), run this
   repository's checks, and send the toolkit a pull request for the sibling revision table in its
   `docs/CATALOGUE.md`.
6. **A widget that is hard to wire right is the toolkit's bug.** Fix it there, or add a roadmap
   row there (`docs/ROADMAP.md`), and say so in this app's pull request.
