//! Keyboard shortcuts read from core's command table, so the keys that work are exactly the keys
//! the Keyboard shortcuts dialog lists. The parser and key names are `mdshared_ui::keys`.

pub use mdshared_ui::keys::{KeyName, Mods, key_name, parse};
use mdview_core::commands::{COMMANDS, CommandId, Shortcut, StandardKey};

/// The shortcut strings of a table entry; platform-standard keys spelled the way the Keyboard
/// shortcuts dialog spells them.
pub fn shortcut_strings(shortcuts: &[Shortcut]) -> impl Iterator<Item = &'static str> + '_ {
    shortcuts.iter().map(|s| match s {
        Shortcut::Keys(k) => k,
        Shortcut::Standard(StandardKey::Quit) => "Ctrl+Q",
        Shortcut::Standard(StandardKey::ZoomIn) => "Ctrl++",
        Shortcut::Standard(StandardKey::ZoomOut) => "Ctrl+-",
    })
}

/// The first shortcut of a command, for menu hints and tooltips.
pub fn hint(id: CommandId) -> &'static str {
    let c = mdview_core::commands::command(id);
    shortcut_strings(c.shortcuts).next().unwrap_or("")
}

/// The command bound to a key, from core's table.
pub fn command_for(key: &KeyName, mods: Mods) -> Option<CommandId> {
    COMMANDS.iter().find_map(|c| {
        shortcut_strings(c.shortcuts)
            .filter_map(parse)
            .any(|chord| chord.matches(key, mods))
            .then_some(c.id)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: Mods = Mods {
        ctrl: false,
        shift: false,
        alt: false,
    };
    const CTRL: Mods = Mods {
        ctrl: true,
        shift: false,
        alt: false,
    };
    const CTRL_SHIFT: Mods = Mods {
        ctrl: true,
        shift: true,
        alt: false,
    };
    const SHIFT: Mods = Mods {
        ctrl: false,
        shift: true,
        alt: false,
    };

    #[test]
    fn every_table_shortcut_parses() {
        for c in COMMANDS {
            for s in shortcut_strings(c.shortcuts) {
                assert!(parse(s).is_some(), "{s} for {:?}", c.id);
            }
        }
    }

    #[test]
    fn table_lookup() {
        assert_eq!(
            command_for(&KeyName::Char('o'), CTRL),
            Some(CommandId::OpenFiles)
        );
        assert_eq!(
            command_for(&KeyName::Char('o'), CTRL_SHIFT),
            Some(CommandId::OpenFolder)
        );
        assert_eq!(command_for(&KeyName::F(5), NONE), Some(CommandId::Refresh));
        assert_eq!(
            command_for(&KeyName::F(3), SHIFT),
            Some(CommandId::FindPrevious)
        );
        assert_eq!(command_for(&KeyName::F(4), CTRL), Some(CommandId::CloseTab));
        assert_eq!(
            command_for(&KeyName::Char('w'), CTRL),
            Some(CommandId::CloseTab)
        );
        assert_eq!(
            command_for(&KeyName::Tab, CTRL_SHIFT),
            Some(CommandId::PreviousTab)
        );
        assert_eq!(command_for(&KeyName::Tab, CTRL), Some(CommandId::NextTab));
        assert_eq!(
            command_for(&KeyName::Char('1'), CTRL_SHIFT),
            Some(CommandId::ToggleFiles)
        );
        assert_eq!(
            command_for(&KeyName::Char('0'), CTRL),
            Some(CommandId::ZoomReset)
        );
        assert_eq!(
            command_for(&KeyName::Char('-'), CTRL),
            Some(CommandId::ZoomOut)
        );
        assert_eq!(
            command_for(&KeyName::Char('q'), CTRL),
            Some(CommandId::Exit)
        );
        assert_eq!(command_for(&KeyName::Escape, NONE), Some(CommandId::Escape));
        assert_eq!(command_for(&KeyName::Char('o'), NONE), None);
        assert_eq!(hint(CommandId::FindNext), "F3");
        assert_eq!(hint(CommandId::About), "");
    }
}
