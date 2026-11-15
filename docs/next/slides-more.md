# More for presentation mode and the shell (`f/slides-more`)

Built on `f/shell-gaps` (which holds Copy as Markdown). Run on the Windows PC with
`cargo fmt --all`, `cargo clippy --locked --workspace --all-targets -- -D warnings` and
`cargo test --locked --release --workspace` (the `mdview` bin: 192 tests, 7 of them new; all
pass). Nothing was run in a real, interactive window: the only real runs are two `--shot` renders
(`present-overview-dark`, `present-select-light`, added to `tests/shots.sh`), which were rendered
and looked at.

## A1. Links on a slide

**Built.** A click on a link on a slide follows it as on the page. An in-document link
(`#name`) goes to the slide that holds the target (this slide first, then the others in order);
the page behind is not moved. Every other destination goes to `Workspace::open_link`, the same
call a click on the page makes: a web or mail address opens in the browser (`link_outcome`), a
Markdown file opens in a tab (which ends the presentation, since the shown tab is no longer the
active one), a file that is not Markdown or a missing one is refused with the same message, and a
scheme the page ignores (`javascript:`) is ignored. A wiki-link goes the page's way too. A name no
slide holds says "no anchor named ..." in the status (not visible while a slide shows; see
below). The pointer is a hand over a link.

**Decisions.** The page asks for no confirmation before opening a link, so a slide asks for none
either (nothing to copy). A click now acts when the button comes up (it did when it went down),
so a drag can select instead; a plain click still goes forward. Footnote references, task boxes
and `<details>` are not acted on in a slide (a click on them goes forward).

**Run.** Tests: a click on `#last-slide` goes to slide 3 and leaves the page's scroll alone; a
click on `other.md` opens a second tab and ends the show; the destination of a web link is the one
written, and the workspace answers `OpenUrl` for it (the real browser is not started by a test).

**Not verified.** A real click on a web link from a full-screen slide (the browser coming to the
front of the borderless window); the hand cursor. Slugs are made per slide, so a link to a
repeated heading's second copy (`#intro-1`) finds no slide.

**Release note.** Links on a slide can be clicked: a link to a heading goes to the slide that holds
it, and other links open as they do on the page.

## A2. Selecting and copying text on a slide

**Built.** Dragging on a slide selects text (tinted as on the page); Ctrl+C puts it on the
clipboard as text and as an HTML fragment (the page's Copy); Ctrl+A takes the slide's text. The
selection is dropped by a click, by changing slide or by new text. It is drawn with the slide's
own frame, nothing per frame beyond it.

**Decisions.** Double and triple click are not word and paragraph selection here: a click goes on
by design, and a double click would skip a slide. Dragging past the window's edge does not scroll
a tall slide. "Copied" is set as the status but a slide shows no status.

**Run.** A test drags along a line, checks the selection, the tint, that the slide did not move
on, the text that Ctrl+C copies (the test stand-in for the clipboard, so the machine's real
clipboard is untouched), and that a click clears it; Ctrl+A is tested. The shot
`present-select-light` was looked at: the tint covers the text exactly.

**Not verified.** The real clipboard and pasting into another program; a drag on a tall slide.

**Release note.** Text on a slide can be selected with the mouse and copied with Ctrl+C.

## A3. Slide overview

**Built.** **G** opens a list of the slides, "N. Title" (the first heading, else the first line of
text), with the keyboard on the slide showing. Up and Down move, Enter goes to the slide and
closes the list, Escape closes only the list (the next Escape leaves the show), a click on a row
goes there, the wheel scrolls it, and typing narrows it (a number or a word of a title). Shift+F5
and Ctrl+Q still work inside it. It is the shell's own palette widget (`mdshared_ui::palette`)
drawn over the slide, so it looks and behaves as the command palette does.

**Decisions.** G was free in a show (only Present and Exit were mapped; the slide keys are Right,
Enter, Left, Backspace, Space, the Page keys, Home, End, Up, Down). The palette's caret is drawn
solid, so no blink timer runs. It is built when opened and dropped when closed; a changed list is
drawn through a counter in the slide's signature, so an idle show draws nothing.

**Run.** Two tests: opening on the current slide, Down, Enter, Escape closing only the list,
typing "numb" then Enter, Shift+F5 from inside; the list is on screen and a click on a row goes
there. The shot `present-overview-dark` was looked at.

**Not verified.** Real keyboard typing (IME and AltGr) in the list; the list in a tiny window.

**Release note.** In a presentation, G lists the slides by number and title; Enter goes to one and
Escape closes the list.

## B. A covered window and animations on Windows

**Built.** winit never sends `Occluded` on Windows, so a window fully covered by another kept
playing GIF and WebP. `platform::is_covered` asks GDI (`GetDC`, `GetClipBox`, `ReleaseDC`: an
empty clip box, `NULLREGION`, is a covered window); `Win32_Graphics_Gdi` was already enabled, so
`Cargo.toml` and `Cargo.lock` are unchanged. In `timers`, when an animated image was playing at
the last look, the window is asked (the answer is kept for 1 s). Covered: no animation work, no
frame, and the next look is armed 1 s away. Uncovered: the animation goes on as before, so it
resumes within about a second.

**Idle cost.** No animated image on the page: nothing is asked and no timer is armed (a test
checks that the system is never asked). With one playing and the window visible: one GDI call
(microseconds) a second, on top of the animation's own frames. Covered: one wake and one GDI call a
second, no layout, no paint, no frame. Minimised, hidden or reported covered (other systems):
nothing, as before.

**Decisions.** "An animated image is playing" is what the last animation tick said, so the first
tick after a page change is made before the window is asked; an image whose frames are still being
decoded counts as not playing until its first tick.

**Run.** Test: covered answer gives only the slow re-look and sets no frame; an old answer is asked
again; a page without animation never asks. The old covered test still passes.

**Not verified.** That `GetClipBox` is empty for a window covered by another on this machine, with
DWM composition and with the frosted backdrop the app can use (a layered window may not report
it): no window was covered. The 1 s timing. Wayland (still never reported, still not asked).

**Release note.** A window that is completely covered by another no longer keeps animating GIFs
and WebP images on Windows.

## C. File types in the installer

**Already built; nothing changed.** `packaging/windows/mdview.iss` registers `.md`, `.markdown`,
`.mdown` and `.mkd` (open-with entries, supported types, capabilities), and the task says so.
`docs/PORT_CHECKLIST.md` is out of date on this. The app itself treats exactly those four as
Markdown (`mdview_core::links::MARKDOWN_EXTENSIONS`, the open dialog's filter), so `.mkdn` and
`.mdwn` are not added: the app would not open them as Markdown from a link. The installer was not
run. **Release note.** None.

## D. Access keys on menu items (report only)

**Not supported; nothing changed.** The menus are `mdshared_ui::menu` (shared crate, not edited).
It draws `Item::label` as it is (no `&` is read, no letter is underlined, the width is measured on
the whole label), and the shell's popup key handling (`app/input.rs`) knows Up, Down, Tab, Enter,
Space, Right and Escape only: no letter opens or chooses an item and Alt does nothing. The labels
in `overlays.rs` carry no `&`. The shell has a "..." menu rather than a menu bar, so the access
keys of menu titles do not apply either. To carry them: the shared menu parses `&` (underline in
`draw_in`, measure without it, a `Menu::mnemonic(char)` lookup), the popup in `input.rs` calls it
for a letter key, and the labels in `overlays.rs` get their `&`.
