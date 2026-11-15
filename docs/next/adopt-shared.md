# Adopting the shared additions (`f/adopt-shared`)

Pins `065cbdc` (shared `f/linux-frost`, containing `681baf8`). Details are folded into
`docs/HANDOFF.md`, "After 0.4.0: gaps closed".

## Built

Snap-layouts flyout (`chrome::max_button_rect`, `App::sync_snap`), menu access keys (`overlays.rs`,
`app/input.rs`, `Popup::access`), the Vue and Svelte export test, and off Windows the blur
(`BlurSystem`) with an alpha surface while the chrome is frosted.

## Run

Build with `--locked`, fmt, clippy `-D warnings`, the whole test suite; a real window with
`--no-session`: `WM_NCHITTEST` answers `HTMAXBUTTON` over the maximize button and `HTCLIENT`
elsewhere in the title row, and the button maximizes and restores; one shot with underlines, looked
at.

## NOT verified

The flyout itself (needs a real pointer on Windows 11); access keys with a real keyboard; the blur
on Linux (never run or seen there, only compiled by CI).

## Release note

Menus show underlined access keys with Alt; on Windows 11 the maximize button shows the
snap-layouts flyout; Vue and Svelte are highlighted; KDE Plasma can blur the backdrop (unseen).
