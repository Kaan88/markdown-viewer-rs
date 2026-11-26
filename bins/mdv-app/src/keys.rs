//! Keyboard shortcuts read from core's command table, so the keys that work are exactly the keys
//! the Keyboard shortcuts dialog lists. The parser and key names are `mdshared_ui::keys`.

#[cfg(test)]
use mdshared_ui::keys::MAC;
pub use mdshared_ui::keys::{KeyName, Mods, key_name, parse};
use mdshared_ui::keys::{for_platform, label};
use mdview::commands::{COMMANDS, CommandId, Shortcut, StandardKey};

/// The shortcut strings of a table entry on this platform (`keys::for_platform`: on a Mac the
/// keys the system keeps move); platform-standard keys spelled the way the Keyboard shortcuts
/// dialog spells them.
pub fn shortcut_strings(shortcuts: &[Shortcut]) -> impl Iterator<Item = &'static str> + '_ {
    shortcuts.iter().map(|s| {
        for_platform(match s {
            Shortcut::Keys(k) => k,
            Shortcut::Standard(StandardKey::Quit) => "Ctrl+Q",
            Shortcut::Standard(StandardKey::ZoomIn) => "Ctrl++",
            Shortcut::Standard(StandardKey::ZoomOut) => "Ctrl+-",
        })
    })
}

/// The first shortcut of a command, for menu hints and tooltips, printed the platform's way
/// ("⌘O" on a Mac).
pub fn hint(id: CommandId) -> &'static str {
    let c = mdview::commands::command(id);
    label(shortcut_strings(c.shortcuts).next().unwrap_or(""))
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

/// The Keyboard shortcuts dialog's lines: core's `shortcut_help`, with each shortcut as this
/// platform has it and prints it.
pub fn shortcut_help() -> Vec<(String, &'static str)> {
    use mdview::commands::Menu;
    COMMANDS
        .iter()
        .filter(|c| !c.shortcuts.is_empty() && c.menu != Menu::TabContext)
        .map(|c| {
            let keys: Vec<&str> = shortcut_strings(c.shortcuts).map(label).collect();
            (keys.join(", "), c.label)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: Mods = Mods {
        ctrl: false,
        shift: false,
        alt: false,
        control: false,
    };
    const CTRL: Mods = Mods {
        ctrl: true,
        shift: false,
        alt: false,
        control: false,
    };
    const CTRL_SHIFT: Mods = Mods {
        ctrl: true,
        shift: true,
        alt: false,
        control: false,
    };
    const SHIFT: Mods = Mods {
        ctrl: false,
        shift: true,
        alt: false,
        control: false,
    };

    #[test]
    fn the_dialog_lists_what_core_lists_off_the_mac() {
        if !MAC {
            assert_eq!(shortcut_help(), mdview::commands::shortcut_help());
        }
    }

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
        // On a Mac, Command+Tab is the system's: the tabs take Control+Tab.
        let tab = if MAC {
            Mods {
                control: true,
                ..NONE
            }
        } else {
            CTRL
        };
        let tab_shift = Mods { shift: true, ..tab };
        assert_eq!(
            command_for(&KeyName::Tab, tab_shift),
            Some(CommandId::PreviousTab)
        );
        assert_eq!(command_for(&KeyName::Tab, tab), Some(CommandId::NextTab));
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
        // On a Mac, Option+arrows move by word: Back and Forward are Command+[ and ].
        if MAC {
            assert_eq!(
                command_for(&KeyName::Char('['), CTRL),
                Some(CommandId::Back)
            );
            assert_eq!(
                command_for(&KeyName::Char(']'), CTRL),
                Some(CommandId::Forward)
            );
        } else {
            let alt = Mods { alt: true, ..NONE };
            assert_eq!(command_for(&KeyName::Left, alt), Some(CommandId::Back));
            assert_eq!(command_for(&KeyName::Right, alt), Some(CommandId::Forward));
        }
        assert_eq!(command_for(&KeyName::Left, NONE), None);
        assert_eq!(command_for(&KeyName::Char('o'), NONE), None);
        assert_eq!(hint(CommandId::FindNext), "F3");
        assert_eq!(hint(CommandId::About), "");
    }
}
