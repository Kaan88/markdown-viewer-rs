//! Chrome colours, one set per page theme. The page itself is coloured by `mdview_render::Theme`;
//! the accent is the page theme's (Windows 11's default blue) rather than the desktop's, so links,
//! checkboxes and the chrome agree.

use crate::gfx::{Rgb, rgb};

#[derive(Clone, Copy, Debug, PartialEq, Hash)]
pub struct Palette {
    pub dark: bool,
    /// Behind the document.
    pub page: Rgb,
    /// The title row, behind the tabs.
    pub title: Rgb,
    /// The command bar and the active tab, which join.
    pub bar: Rgb,
    pub pane: Rgb,
    pub text: Rgb,
    pub muted: Rgb,
    pub faint: Rgb,
    pub hover: Rgb,
    pub press: Rgb,
    /// A toggled button or the selected row.
    pub on: Rgb,
    pub line: Rgb,
    pub accent: Rgb,
    /// Text drawn on an accent fill.
    pub on_accent: Rgb,
    pub field: Rgb,
    pub popup: Rgb,
    pub popup_line: Rgb,
    pub folder: Rgb,
    pub thumb: Rgb,
    pub find_mark: Rgb,
    pub find_current: Rgb,
    pub selection: Rgb,
    /// The close button's hover fill.
    pub danger: Rgb,
    /// Missing-file marks.
    pub warn: Rgb,
    pub warn_bg: Rgb,
}

pub const DARK: Palette = Palette {
    dark: true,
    page: rgb(0x202020),
    title: rgb(0x2e2e2e),
    bar: rgb(0x292929),
    pane: rgb(0x262626),
    text: rgb(0xe6e6e6),
    muted: rgb(0xa3a3a3),
    faint: rgb(0x767676),
    hover: rgb(0x3a3a3a),
    press: rgb(0x444444),
    on: rgb(0x333a48),
    line: rgb(0x383838),
    accent: rgb(0x60cdff),
    on_accent: rgb(0x0b1a24),
    field: rgb(0x1d1d1d),
    popup: rgb(0x2c2c2c),
    popup_line: rgb(0x424242),
    folder: rgb(0xdcab3f),
    thumb: rgb(0x9a9a9a),
    find_mark: rgb(0xc9a400),
    find_current: rgb(0xff8c1a),
    selection: rgb(0x3d7fd6),
    danger: rgb(0xc42b1c),
    warn: rgb(0xfcb54a),
    warn_bg: rgb(0x3a3020),
};

pub const LIGHT: Palette = Palette {
    dark: false,
    page: rgb(0xfbfbfb),
    title: rgb(0xe9e9e9),
    bar: rgb(0xf5f5f5),
    pane: rgb(0xf3f3f3),
    text: rgb(0x1b1b1b),
    muted: rgb(0x5c5c5c),
    faint: rgb(0x858585),
    hover: rgb(0xe2e2e2),
    press: rgb(0xd6d6d6),
    on: rgb(0xdce7f6),
    line: rgb(0xdcdcdc),
    accent: rgb(0x005fb8),
    on_accent: rgb(0xffffff),
    field: rgb(0xffffff),
    popup: rgb(0xfcfcfc),
    popup_line: rgb(0xd5d5d5),
    folder: rgb(0xc8901a),
    thumb: rgb(0x8a8a8a),
    find_mark: rgb(0xffd400),
    find_current: rgb(0xff8a1f),
    selection: rgb(0x3b8eea),
    danger: rgb(0xc42b1c),
    warn: rgb(0x9d5d00),
    warn_bg: rgb(0xfff4ce),
};
