//! Everything drawn above the regions: popup menus (built here, drawn by `menu.rs`), the modal
//! dialogs, the status toast, the link hint and tooltips.

use crate::chrome::{Ctx, glyph, logo};
use crate::gfx::{Face, Rect, mix};
use crate::keys;
use crate::menu::{Item, Menu};
use crate::panes::{draw_scrollbar, scrollbar};
use crate::ui::{Act, Dialog, DialogKind, EditAct, Hit, Scroller, Ui};
use crate::workspace::{PageContext, Workspace};
pub use mdshared_ui::overlay::Bubble;
use mdshared_ui::overlay::{
    DIALOG_FOOTER, DIALOG_PAD, dialog_button_w, dialog_card, draw_backdrop, draw_dialog_button,
    draw_dialog_card, draw_keycaps,
};
use mdview_core::commands::{CommandId as C, shortcut_help, strings};
use mdview_core::session::PageWidth;
use mdview_render::SourceKind;

// ---- menus -----------------------------------------------------------------------------------

fn cmd(id: C, label: &str, icon: &'static str) -> Item<Act> {
    Item::new(Act::Cmd(id), label)
        .hint(keys::hint(id))
        .icon(icon)
}

/// The "⋯" menu: everything the bar does not show, in the Qt shell's order.
pub fn main_menu(ws: &Workspace, ui: &Ui, x: f32, y: f32) -> Menu<Act> {
    let has_tab = ws.active().is_some();
    let width = |choice: PageWidth, label: &str| {
        Item::new(Act::PageWidth(choice), label).checked(ws.page_width() == choice)
    };
    let items = vec![
        cmd(C::OpenFiles, "Open files…", glyph::OPEN),
        cmd(C::OpenFolder, "Open folder…", glyph::FOLDER_OPEN),
        Item::new(Act::RecentMenu, "Open recent")
            .icon(glyph::RECENT)
            .hint("›")
            .enabled(!ws.recents().is_empty()),
        cmd(C::ManageRecent, "Manage recent files…", ""),
        cmd(C::Refresh, "Reload", glyph::RELOAD)
            .sep()
            .enabled(has_tab),
        cmd(C::CloseTab, "Close tab", glyph::CLOSE).enabled(has_tab),
        cmd(C::CloseAllTabs, "Close all tabs", "").enabled(has_tab),
        cmd(C::ZoomIn, "Zoom in", glyph::PLUS).sep(),
        cmd(C::ZoomOut, "Zoom out", glyph::MINUS),
        Item::new(
            Act::Cmd(C::ZoomReset),
            format!("Reset zoom ({}%)", ws.zoom()),
        )
        .hint(keys::hint(C::ZoomReset))
        .icon(glyph::ZOOM),
        cmd(C::ToggleFiles, "Files pane", "")
            .sep()
            .checked(ui.panes.files && !ui.focus_mode),
        cmd(C::ToggleOutline, "Outline pane", "").checked(ui.panes.outline && !ui.focus_mode),
        cmd(C::FocusMode, "Focus reading mode", glyph::FULLSCREEN).checked(ui.focus_mode),
        Item::new(Act::RemoteAlways, "Always load remote images").checked(ws.remote_always()),
        Item::new(Act::EditAlways, "Tick task boxes without asking").checked(ws.edit_always()),
        width(PageWidth::Narrow, "Narrow page").sep(),
        width(PageWidth::Wide, "Wide page"),
        width(PageWidth::Full, "Full-width page"),
        cmd(C::ShowShortcuts, "Keyboard shortcuts", glyph::KEYBOARD).sep(),
        cmd(C::About, "About mdview", glyph::INFO),
        cmd(C::Exit, "Exit", glyph::POWER).sep(),
    ];
    Menu::new(items, x, y, true)
}

pub fn recent_menu(ws: &Workspace, x: f32, y: f32, align_right: bool) -> Menu<Act> {
    let mut items: Vec<Item<Act>> = Vec::new();
    let entries = ws.recent_entries();
    if entries.is_empty() {
        items.push(Item::new(Act::Cmd(C::OpenRecent), strings::RECENT_EMPTY).enabled(false));
    }
    for e in entries {
        let folder = e
            .path
            .parent()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let icon = if e.missing { glyph::WARN } else { glyph::DOC };
        items.push(
            Item::new(Act::Recent(e.path.clone()), e.label)
                .hint(folder)
                .icon(icon),
        );
    }
    let any = !ws.recents().is_empty();
    items.push(
        Item::new(Act::Cmd(C::ManageRecent), "Manage recent files…")
            .icon(glyph::RECENT)
            .sep(),
    );
    items.push(
        Item::new(Act::Cmd(C::ClearRecent), "Clear recent files")
            .icon(glyph::DELETE)
            .enabled(any),
    );
    Menu::new(items, x, y, align_right)
}

pub fn tab_menu(ws: &Workspace, index: usize, x: f32, y: f32) -> Menu<Act> {
    let many = ws.tab_count() > 1;
    let items = vec![
        Item::new(Act::TabCmd(C::CloseTab, index), "Close tab").icon(glyph::CLOSE),
        Item::new(Act::TabCmd(C::CloseOtherTabs, index), "Close other tabs").enabled(many),
        Item::new(Act::TabCmd(C::CloseAllTabs, index), "Close all tabs"),
        Item::new(Act::TabCmd(C::CopyFilePath, index), "Copy file path")
            .icon(glyph::COPY)
            .sep(),
        Item::new(
            Act::TabCmd(C::ShowInFileManager, index),
            mdview_core::commands::command(C::ShowInFileManager).label,
        )
        .icon(glyph::FOLDER_OPEN),
    ];
    Menu::new(items, x, y, false)
}

pub fn page_menu(ctx: &PageContext, x: f32, y: f32) -> Menu<Act> {
    let mut items = Vec::new();
    if let Some(link) = &ctx.link {
        items.push(Item::new(Act::OpenLink(link.clone()), "Open link").icon(glyph::OPEN_WITH));
        items.push(Item::new(Act::CopyLink(link.clone()), "Copy link address").icon(glyph::LINK));
    }
    if let Some((kind, text)) = &ctx.source {
        let label = match kind {
            SourceKind::Formula => "Copy formula source",
            SourceKind::Diagram => "Copy diagram source",
        };
        let mut item = Item::new(Act::CopySource(*kind, text.clone()), label).icon(glyph::COPY);
        if !items.is_empty() {
            item = item.sep();
        }
        items.push(item);
    }
    let first = items.is_empty();
    let mut copy = Item::new(Act::Copy, "Copy")
        .hint("Ctrl+C")
        .icon(glyph::COPY)
        .enabled(ctx.has_selection);
    if !first {
        copy = copy.sep();
    }
    items.push(copy);
    items.push(
        Item::new(Act::SelectAll, "Select all")
            .hint("Ctrl+A")
            .icon(glyph::SELECT_ALL),
    );
    items.push(cmd(C::Find, "Find…", glyph::SEARCH).sep());
    items.push(cmd(C::Refresh, "Reload", glyph::RELOAD));
    Menu::new(items, x, y, false)
}

/// The find box's menu: the editing commands a text box has everywhere.
pub fn find_menu(has_text: bool, has_selection: bool, x: f32, y: f32) -> Menu<Act> {
    let items = vec![
        Item::new(Act::Edit(EditAct::Cut), "Cut")
            .hint("Ctrl+X")
            .icon(glyph::CUT)
            .enabled(has_selection),
        Item::new(Act::Edit(EditAct::Copy), "Copy")
            .hint("Ctrl+C")
            .icon(glyph::COPY)
            .enabled(has_selection),
        Item::new(Act::Edit(EditAct::Paste), "Paste")
            .hint("Ctrl+V")
            .icon(glyph::PASTE),
        Item::new(Act::Edit(EditAct::SelectAll), "Select all")
            .hint("Ctrl+A")
            .icon(glyph::SELECT_ALL)
            .enabled(has_text)
            .sep(),
    ];
    Menu::new(items, x, y, false)
}

// ---- dialogs ---------------------------------------------------------------------------------

const PAD: f32 = DIALOG_PAD;
const FOOTER: f32 = DIALOG_FOOTER;
const KEY_ROW: f32 = 34.0;
const RECENT_ROW: f32 = 52.0;

/// The card for a dialog in a `w` x `h` window, sized to its list where it has one.
pub fn card(kind: DialogKind, rows: usize, w: f32, h: f32) -> Rect {
    let list =
        |row: f32, min: f32| (PAD + 36.0 + (rows as f32 * row + 12.0).max(min) + FOOTER).round();
    let (cw, ch): (f32, f32) = match kind {
        DialogKind::About => (460.0, 236.0),
        DialogKind::Shortcuts => (580.0, list(KEY_ROW, 0.0).min(700.0)),
        DialogKind::Recent => (600.0, list(RECENT_ROW, 80.0).min(560.0)),
        DialogKind::AllowEdit => (500.0, 232.0),
    };
    dialog_card(cw, ch, w, h)
}

fn shortcut_label(label: &str) -> String {
    if label == strings::FILES_DOCK {
        "Files pane".into()
    } else if label == strings::OUTLINE_DOCK {
        "Outline pane".into()
    } else {
        label.to_string()
    }
}

fn button(c: &mut Ctx, r: Rect, hit: Hit, label: &str, primary: bool) {
    let hovered = c.hover == Some(hit);
    draw_dialog_button(
        c.p,
        c.pal,
        r,
        label,
        primary,
        hovered,
        c.pressed == Some(hit),
    );
    c.hit(hit, r);
}

use dialog_button_w as button_w;

pub fn draw_dialog(
    c: &mut Ctx,
    ws: &Workspace,
    d: &mut Dialog,
    w: f32,
    h: f32,
    hover_row: Option<usize>,
) {
    let pal = *c.pal;
    c.hit(Hit::Backdrop, Rect::new(0.0, 0.0, w, h));
    draw_backdrop(c.p, &pal, w, h);
    let rows = match d.kind {
        DialogKind::About => 0,
        DialogKind::Shortcuts => shortcut_help().len(),
        DialogKind::Recent => ws.recents().len(),
        DialogKind::AllowEdit => 0,
    };
    let r = card(d.kind, rows, w, h);
    c.hit(Hit::DlgCard, r);
    let footer = draw_dialog_card(c.p, &pal, r);
    let by = footer.y + (FOOTER - 32.0) / 2.0;
    let title = match d.kind {
        DialogKind::About => strings::ABOUT_TITLE,
        DialogKind::Shortcuts => strings::SHORTCUTS_TITLE,
        DialogKind::Recent => strings::MANAGE_RECENT_TITLE,
        DialogKind::AllowEdit => "Change this file?",
    };
    match d.kind {
        DialogKind::About => {
            logo(c.p, Rect::new(r.x + PAD, r.y + PAD + 4.0, 56.0, 56.0));
            let x = r.x + PAD + 56.0 + 20.0;
            let tw = r.right() - x - PAD;
            c.p.text(
                Face::Semibold,
                18.0,
                x,
                r.y + PAD + 14.0,
                strings::APP_NAME,
                pal.text,
                tw,
            );
            c.p.text(
                Face::Ui,
                12.5,
                x,
                r.y + PAD + 38.0,
                &format!("mdview {}", env!("CARGO_PKG_VERSION")),
                pal.muted,
                tw,
            );
            let y = r.y + PAD + 60.0;
            let used =
                c.p.paragraph(Face::Ui, 13.5, x, y, tw, strings::ABOUT, pal.text);
            c.p.paragraph(
                Face::Ui,
                12.5,
                x,
                y + used + 6.0,
                tw,
                "winit and softbuffer, with every pixel drawn by mdview's own renderer. No webview.",
                pal.muted,
            );
            let bw = button_w(c.p, "OK");
            button(
                c,
                Rect::new(r.right() - PAD - bw, by, bw, 32.0),
                Hit::DlgButton(0),
                "OK",
                true,
            );
        }
        DialogKind::AllowEdit => {
            let x = r.x + PAD;
            let tw = r.w - 2.0 * PAD;
            c.p.text(
                Face::Semibold,
                17.0,
                x,
                r.y + PAD + 10.0,
                title,
                pal.text,
                tw,
            );
            let name = ws.edit_target().unwrap_or_default();
            let y = r.y + PAD + 36.0;
            let used = c.p.paragraph(
                Face::Ui,
                13.5,
                x,
                y,
                tw,
                &format!(
                    "Ticking this box saves the change to {name}. Only the mark between the \
                     brackets is rewritten; the rest of the file stays as it is."
                ),
                pal.text,
            );
            c.p.paragraph(
                Face::Ui,
                12.5,
                x,
                y + used + 8.0,
                tw,
                "\"Always allow\" stops asking, for every file. The main menu turns it off again.",
                pal.muted,
            );
            let ew = button_w(c.p, "Change this file");
            button(
                c,
                Rect::new(r.right() - PAD - ew, by, ew, 32.0),
                Hit::DlgButton(1),
                "Change this file",
                true,
            );
            let aw = button_w(c.p, "Always allow");
            button(
                c,
                Rect::new(r.right() - PAD - ew - 8.0 - aw, by, aw, 32.0),
                Hit::DlgButton(2),
                "Always allow",
                false,
            );
            let cw = button_w(c.p, "Cancel");
            button(
                c,
                Rect::new(r.x + PAD, by, cw, 32.0),
                Hit::DlgButton(0),
                "Cancel",
                false,
            );
        }
        DialogKind::Shortcuts => {
            c.p.text(
                Face::Semibold,
                17.0,
                r.x + PAD,
                r.y + PAD + 10.0,
                title,
                pal.text,
                r.w - 2.0 * PAD,
            );
            let list = Rect::new(
                r.x + 8.0,
                r.y + PAD + 36.0,
                r.w - 16.0,
                footer.y - (r.y + PAD + 36.0) - 4.0,
            );
            let help = shortcut_help();
            d.list = list;
            d.view = list.h;
            d.content = help.len() as f32 * KEY_ROW + 8.0;
            d.scroll = d.scroll.clamp(0.0, (d.content - d.view).max(0.0));
            let old = c.p.clip(list);
            for (i, (keys, label)) in help.iter().enumerate() {
                let y = list.y + i as f32 * KEY_ROW - d.scroll;
                if y + KEY_ROW < list.y || y > list.bottom() {
                    continue;
                }
                let row = Rect::new(list.x + 8.0, y, list.w - 16.0, KEY_ROW);
                if i % 2 == 1 {
                    c.p.rbox(row, 5.0, mix(pal.popup, pal.text, 0.025));
                }
                c.p.text(
                    Face::Ui,
                    13.0,
                    row.x + 12.0,
                    row.cy(),
                    &shortcut_label(label),
                    pal.text,
                    row.w * 0.5,
                );
                draw_keycaps(c.p, &pal, row, keys);
            }
            c.p.restore(old);
            dialog_scrollbar(c, d);
            let bw = button_w(c.p, "Close");
            button(
                c,
                Rect::new(r.right() - PAD - bw, by, bw, 32.0),
                Hit::DlgButton(0),
                "Close",
                true,
            );
        }
        DialogKind::Recent => {
            c.p.text(
                Face::Semibold,
                17.0,
                r.x + PAD,
                r.y + PAD + 10.0,
                title,
                pal.text,
                r.w - 2.0 * PAD,
            );
            let list = Rect::new(
                r.x + 8.0,
                r.y + PAD + 36.0,
                r.w - 16.0,
                footer.y - (r.y + PAD + 36.0) - 4.0,
            );
            let entries = ws.recent_entries();
            d.list = list;
            d.view = list.h;
            d.content = entries.len() as f32 * RECENT_ROW + 8.0;
            d.scroll = d.scroll.clamp(0.0, (d.content - d.view).max(0.0));
            if entries.is_empty() {
                c.p.text_center(
                    Face::Ui,
                    13.5,
                    Rect::new(list.x, list.y + 30.0, list.w, 24.0),
                    strings::RECENT_EMPTY,
                    pal.faint,
                );
            }
            let old = c.p.clip(list);
            for (i, e) in entries.iter().enumerate() {
                let y = list.y + i as f32 * RECENT_ROW - d.scroll;
                if y + RECENT_ROW < list.y || y > list.bottom() {
                    continue;
                }
                let row = Rect::new(list.x + 8.0, y + 2.0, list.w - 16.0, RECENT_ROW - 4.0);
                if hover_row == Some(i) {
                    c.p.rbox(row, 6.0, mix(pal.popup, pal.text, 0.05));
                }
                c.p.icon(
                    if e.missing { glyph::WARN } else { glyph::DOC },
                    15.0,
                    row.x + 20.0,
                    row.cy(),
                    if e.missing { pal.warn } else { pal.accent },
                );
                let tx = row.x + 40.0;
                let tw = row.w - 40.0 - 150.0;
                let name = e
                    .path
                    .file_name()
                    .map_or_else(|| e.label.clone(), |n| n.to_string_lossy().into_owned());
                let shown = if e.missing {
                    format!("{name}  (missing)")
                } else {
                    name
                };
                c.p.text(
                    Face::Ui,
                    13.5,
                    tx,
                    row.y + 15.0,
                    &shown,
                    if e.missing { pal.muted } else { pal.text },
                    tw,
                );
                let dir = e
                    .path
                    .parent()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
                c.p.text(Face::Ui, 11.5, tx, row.y + 33.0, &dir, pal.faint, tw);
                let open = Rect::new(row.right() - 112.0, row.cy() - 15.0, 70.0, 30.0);
                if !e.missing {
                    button(c, open, Hit::DlgOpen(i), "Open", false);
                }
                let rm = Rect::new(row.right() - 36.0, row.cy() - 15.0, 30.0, 30.0);
                let hit = Hit::DlgRemove(i);
                if c.hover == Some(hit) {
                    c.p.rbox(
                        rm,
                        5.0,
                        if c.pressed == Some(hit) {
                            pal.press
                        } else {
                            pal.hover
                        },
                    );
                }
                c.p.icon(glyph::DELETE, 14.0, rm.cx(), rm.cy(), pal.muted);
                c.hit(hit, rm);
            }
            c.p.restore(old);
            dialog_scrollbar(c, d);
            let bw = button_w(c.p, strings::RECENT_DONE);
            button(
                c,
                Rect::new(r.right() - PAD - bw, by, bw, 32.0),
                Hit::DlgButton(0),
                strings::RECENT_DONE,
                true,
            );
            if !entries.is_empty() {
                let cw = button_w(c.p, strings::RECENT_CLEAR_LIST);
                button(
                    c,
                    Rect::new(r.x + PAD, by, cw, 32.0),
                    Hit::DlgButton(1),
                    strings::RECENT_CLEAR_LIST,
                    false,
                );
            }
        }
    }
}

fn dialog_scrollbar(c: &mut Ctx, d: &Dialog) {
    if let Some((track, thumb)) = scrollbar(d.list, d.content, d.view, d.scroll) {
        let hit = Hit::ScrollBar(Scroller::Dialog);
        let pal = *c.pal;
        draw_scrollbar(c.p, &pal, track, thumb, c.hover == Some(hit));
        c.hit(hit, track);
    }
}

/// The Recent dialog row under `y`, for hover.
pub fn dialog_row_at(d: &Dialog, x: f32, y: f32) -> Option<usize> {
    if d.kind != DialogKind::Recent || !d.list.contains(x, y) {
        return None;
    }
    Some(((y - d.list.y + d.scroll) / RECENT_ROW).floor() as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cards_fit_small_windows() {
        for kind in [
            DialogKind::About,
            DialogKind::Shortcuts,
            DialogKind::Recent,
            DialogKind::AllowEdit,
        ] {
            let r = card(kind, 30, 520.0, 360.0);
            assert!(r.x >= 0.0 && r.y >= 0.0 && r.right() <= 520.0 && r.bottom() <= 360.0);
        }
    }
}
