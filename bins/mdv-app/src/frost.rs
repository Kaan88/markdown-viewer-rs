//! The "Window backdrop" setting in this window: which regions are chrome. When the chrome is
//! frosted and what the compositor was asked for is `mdshared_ui::frost::Frost`, which mdedit
//! uses too; the drawing is `mdshared_ui::frost` (the chrome is drawn as always, marked, and its
//! flat colour taken out again when the frame goes to the window).

pub use mdshared_ui::frost::Frost;

use crate::ui::Region;

/// The regions that let the backdrop through: the title row with its tabs, the bar (with the
/// notices under it) and the two side panes. The page stays opaque; menus and dialogs are drawn
/// over the chrome with opaque fills and stay opaque by that (see `mdshared_ui::frost`).
pub fn frosted(r: Region) -> bool {
    matches!(
        r,
        Region::Title | Region::Bar | Region::Files | Region::Outline
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_stays_opaque() {
        use crate::ui::REGIONS;
        let frosted: Vec<Region> = REGIONS.iter().copied().filter(|r| frosted(*r)).collect();
        assert_eq!(
            frosted,
            [Region::Title, Region::Bar, Region::Files, Region::Outline]
        );
    }
}
