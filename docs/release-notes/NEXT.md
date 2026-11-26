mdview next: menus fade in as they open, folders' arrows turn, toasts rise in and confirmations stack, Settings moves, tooltips wait for the pointer to rest, and an Animations setting.

## New

- **Shared Markdown renderer.** mdview now uses the toolkit's renderer and brings in fixes that restrict network image paths, SVG file access, and engine discovery; bound WebP and preview sizes; avoid blocking on named pipes on Unix; and keep inline math and export from slowing down on large inputs.

- **Menus fade in.** The main menu, the context menus and the Open recent submenu fade in and settle into place as they open, quickly; they close at once.

- **A menu row's hover fills in.** The row the pointer or the keys reach fills in quickly instead of at once, and fades out as they leave it; with animations off or the system asking for less motion it is at once.

- **Title row hovers fill in.** The window buttons, a tab and the new tab button fill in quickly instead of at once, and fade out as the pointer leaves; with animations off or the system asking for less motion they are at once.

- **The command palette fades in.** It fades and drops into place as it opens, quickly; it closes at once.

- **A notice bar slides in.** The update bar, last time's crash report and the offer to close old tabs slide down into place and the page moves under them, quickly; a bar that is dismissed goes at once.

- **A folder's arrow turns.** In the Files pane, and for a group or a section of the Notes pane, the arrow beside a folder turns down as it opens and back as it closes.

- **Toasts rise in, and confirmations stack.** A message rises and fades in at the bottom of the page, and fades and sinks away when its time is up. A message still takes the place of the one showing (a zoom step, an export and its result), but a confirmation such as "Copied" or "Link address copied" comes in under the ones showing, which move up first so that nothing covers it; at most three show at once.

- **Settings in motion.** In the Settings dialog a switch's knob slides across and its fill comes in, the chosen option of a segmented row slides to its place, and the page list's highlight fades in and out. Under a high-contrast theme the highlights change at once, as the system's own controls do.

- **Tooltips wait for the pointer to rest.** A tooltip now appears after the pointer has rested on its control for half a second, and fades in; moving across the control starts that wait again. Tooltips go at once when the pointer leaves.

- **An "Animations" setting.** On the Appearance page of Settings, on unless turned off: off, menus open at once with no fade, folders' arrows do not turn, toasts come and go at once, Settings changes at once and tooltips appear without a fade. When the system asks for less motion (Windows: Settings, Accessibility, Visual effects, "Animation effects" off; macOS: "Reduce motion"), nothing moves whatever this setting says. "Play animated images" under Reading is a separate setting, for animated GIF and WebP pictures only, and is unchanged.
