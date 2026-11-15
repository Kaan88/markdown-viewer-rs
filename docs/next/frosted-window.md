# Frosted window (Windows 11), mdview side

Feature key `frosted-window`. Viewer branch `f/frosted-window`, made from `f/finer-sync` with
`feat/restart-after-update` merged in. Pins the shared branch `merge/editor-next` (`c9ba573`),
where the code is. Off by default. **Prepared, not merged** into `merge/next-release`.

The feature was built for mdedit first (its DESIGN section 67 has the full description: the
conditions, the key-out arithmetic, the veil, why Mica Alt is left out). This file says what
mdview adds and what was checked here.

## README

Feature list:

- **Window backdrop** (Windows 11, off by default): Settings > Window backdrop lets the system's
  Mica or Acrylic material show through the title row and tab strip, the bar and the Files and
  Outline panes. The page, menus and dialogs stay solid.

No shortcut, no command.

## DESIGN

**The setting.** `backdrop=mica|acrylic` in the session file (`Session::backdrop`, default off;
the line is written only when it is not off, so a session that never chose it is the file it
was). The row "Window backdrop" is under Theme in Settings, with the same title, choices and help
line as mdedit's (`mdshared_app::backdrop::Backdrop::{TITLE, LABELS, HELP}`), and exists only in
a Windows build. `--backdrop off|mica|acrylic` sets it for a run (for a capture; with
`--no-session` nothing is saved).

**What is frosted.** The regions Title (title row and tab strip), Bar, Files and Outline. The
page with its banners and the update bar, menus, the palette, tooltips and dialog cards stay
opaque; a dialog's dimming and a menu's shadow darken the frosted chrome as they darken the flat
one.

**When, and how.** As in mdedit, by the same code: `mdshared_ui::frost::Frost` decides (the
setting, Windows 11 22H2 or later, transparency effects on, no high contrast, no battery saver,
the calls accepted, the window has the keyboard) and asks the system through
`frost::WindowSystem`; a frosted region's pixels are marked after it is drawn (`frost::mark`) and
keyed out in the copy that goes to the window (`frost::key_out` in `App::present`, shifted by the
client margin). With the setting off the cost is one comparison a frame and no system call.

**Where the code is.** `bins/mdv-app/src/frost.rs` (which regions), `App::sync_frost`, the mark
in the region loop of `compose`, the key-out in `present`, `App::focused` (set by
`WindowEvent::Focused`), `Workspace::backdrop` / `set_backdrop`, the row in `settings.rs`.

## Release note

- Windows 11: an optional frosted window. Settings > Window backdrop (Mica or Acrylic) lets the
  system material show through the bars and the side panes. Off by default.

## Handoff

**Built.** As above, plus tests: `session::the_window_backdrop_is_off_unless_chosen_and_then_kept`,
`frost::tests::the_page_stays_opaque`, and five in `app/frost_tests.rs` on composed frames (the
chrome marked and the page not, at scale 1 and 1.5, and still so after a scroll; a menu over the
chrome opaque and its shadow not; the Settings dialog dims without making opaque, and leaves
nothing behind when closed; unfocused or off: every pixel opaque; the row sits under Theme on
Windows and is absent elsewhere). Shot `dialog-settings-backdrop` (the row set to Mica; a shot
has no window, so its chrome is flat).

**Setting off is unchanged.** `tests/shots.sh` against the set of `f/finer-sync`: 67 of 74
byte-identical. The seven that differ: the five Settings shots (`dialog-settings-light`, `-dark`,
`-scale-150`, `-760`, `-640`: one more row), `update-installed` (the Restart button's branch:
the button and the new sentence) and `dialog-recent` (it prints the folder the checkout is in).
New: `dialog-settings-backdrop`, `update-installed-plain`.

**Seen in a real window** (Windows 11 build 26300, release build, `--no-session` and a scratch
settings file, started and closed by a script; the window was put above the others for a moment
and the screen where it is was saved): Mica in the dark and the light theme and Acrylic in the
dark theme, each with both panes open. The title row, the bar and both panes show the material,
the page is solid, the text of the chrome is sharp. With the window not in front the chrome was
the flat colours, the same pixels as with the setting off. `docs/next/frosted-shots/` has
`dark-mica-active.png`, `light-mica-active.png` and `dark-off-active.png` (the Acrylic capture
shows what was behind the window on this desktop and was not kept).

**Numbers** (release builds, this PC, three runs each of `--scroll-steps 120` on
`fixtures/showcase.md` with both panes open; "before" is `f/finer-sync`):

| | before | off | Mica | Acrylic |
|---|---|---|---|---|
| first frame, ms | 83.2 to 92.6 | 80.8 to 89.8 | 81.9 to 95.6 | 84.2 to 89.8 |
| scroll frame, own work, ms (median / p90) | 0.53-0.54 / 0.77-0.81 | 0.51-0.54 / 0.77-0.87 | 0.67-0.69 / 0.89-1.05 | 0.66-0.67 / 0.92-0.96 |

Off is the same as before within the runs' spread. On: about 0.14 ms more on a scroll frame, and
only with the Outline pane open (without it, or with only the Files pane, the figure is the one
with the setting off: 0.56 to 0.58 ms against 0.57 to 0.59). The Outline pane is repainted while
the page scrolls (the heading in view is marked in it), and a repainted frosted region is marked
and keyed out again. Memory was not measured.

**Decisions taken, for the owner to confirm.**

1. Both side panes are frosted, as mdedit's one is.
2. The row is in the Page group under Theme (mdview's Settings has no View group).
3. `--backdrop` was added as a launch argument, for captures.
4. The order on this branch is Restart first, then the backdrop: the shared head this branch
   pins has the Restart button's bar, so the viewer needs that branch's code to build.

**Not run.** Everything in mdedit's list (transparency effects off, battery saver, high
contrast, Windows 10, Windows 11 before 22H2, maximized, snapped, a second monitor, scales other
than 1 in a real window, remote desktop); changing the setting in the dialog by hand; a menu, a
dialog, the palette or a tooltip over the frosted chrome in a real window (a menu and a dialog
are tested headless, the palette and tooltips are not); Linux
(the row is absent and the calls return false; not built there in this session). Nobody has
looked at it by eye.
