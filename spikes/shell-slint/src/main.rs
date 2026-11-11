//! Layout A in Slint (software renderer on winit). Throwaway mockup: the page is the stand-in
//! image from `look-shared`, and only the pane toggles, tabs and selection react.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

slint::slint! {
    import { ScrollView } from "std-widgets.slint";

    global C {
        out property <color> page: #202020;
        out property <color> title: #2e2e2e;
        out property <color> bar: #292929;
        out property <color> pane: #262626;
        out property <color> text: #e6e6e6;
        out property <color> muted: #9e9e9e;
        out property <color> hover: #3a3a3a;
        out property <color> line: #383838;
        out property <color> accent: #4c8dff;
        out property <color> on: #333a48;
        out property <string> icons: "Segoe Fluent Icons";
    }

    component ToolButton inherits Rectangle {
        in property <string> icon;
        in property <string> label;
        in property <bool> on;
        callback clicked;
        width: label == "" ? 36px : 44px + t.preferred-width;
        height: 32px;
        border-radius: 6px;
        background: area.pressed ? #444 : area.has-hover ? C.hover : on ? C.on : transparent;
        animate background { duration: 90ms; }
        area := TouchArea { clicked => { root.clicked(); } }
        HorizontalLayout {
            padding-left: 10px; spacing: 8px;
            Text { text: icon; font-family: C.icons; font-size: 16px; color: on ? C.accent : C.text; vertical-alignment: center; }
            t := Text { text: label; font-size: 14px; color: C.text; vertical-alignment: center; visible: label != ""; }
        }
    }

    component Sep inherits Rectangle {
        width: 13px;
        Rectangle { x: 6px; y: 8px; width: 1px; height: parent.height - 16px; background: C.line; }
    }

    component Tab inherits Rectangle {
        in property <string> name;
        in property <bool> active;
        callback clicked;
        width: 70px + n.preferred-width;
        height: 34px;
        y: 6px;
        TouchArea { clicked => { root.clicked(); } }
        Rectangle {
            y: active ? 0 : 4px;
            height: active ? parent.height + 8px : parent.height - 8px;
            x: active ? 0 : 2px; width: active ? parent.width : parent.width - 4px;
            border-radius: active ? 8px : 6px;
            background: active ? C.bar : area.has-hover ? C.hover : transparent;
        }
        area := TouchArea { clicked => { root.clicked(); } }
        HorizontalLayout {
            padding-left: 12px; padding-right: 10px; spacing: 8px;
            Text { text: "\u{E8A5}"; font-family: C.icons; font-size: 13px; color: active ? C.accent : C.muted; vertical-alignment: center; }
            n := Text { text: name; font-size: 13px; font-weight: active ? 600 : 400; color: active ? C.text : C.muted; vertical-alignment: center; }
            Rectangle { width: 16px; }
            Text { text: "\u{E711}"; font-family: C.icons; font-size: 9px; color: C.muted; vertical-alignment: center; visible: active || area.has-hover; }
        }
    }

    struct Row { level: int, name: string, folder: bool }

    component Pane inherits Rectangle {
        in property <string> heading;
        in property <[Row]> rows;
        in property <bool> icons;
        in-out property <int> selected;
        background: C.pane;
        clip: true;
        Text { x: 16px; y: 10px; text: heading; font-size: 11px; font-weight: 600; color: C.muted; }
        for row[i] in rows: Rectangle {
            x: 6px; y: 38px + i * 30px; width: parent.width - 12px; height: 28px;
            border-radius: 5px;
            background: i == selected ? C.on : ta.has-hover ? C.hover : transparent;
            ta := TouchArea { clicked => { root.selected = i; } }
            Rectangle { x: 0; y: 7px; width: 3px; height: 14px; border-radius: 1.5px; background: C.accent; visible: i == selected; }
            HorizontalLayout {
                padding-left: 10px + row.level * 16px; spacing: 8px;
                if icons: Text { text: row.folder ? "\u{E8B7}" : "\u{E8A5}"; font-family: C.icons; font-size: 13px; color: row.folder ? #e8b339 : C.muted; vertical-alignment: center; }
                Text { text: row.name; horizontal-stretch: 1; horizontal-alignment: left; font-size: 13px; color: i == selected ? C.text : #ccc; vertical-alignment: center; overflow: elide; }
            }
        }
    }

    export component Main inherits Window {
        title: "mdview — showcase.md";
        preferred-width: 1280px;
        preferred-height: 800px;
        background: C.title;
        default-font-family: "Segoe UI Variable Text";
        in property <image> page-image;
        in-out property <bool> files-on: true;
        in-out property <bool> outline-on: true;
        in-out property <int> active-tab: 0;
        in-out property <bool> find-focus;

        VerticalLayout {
            // Title row
            HorizontalLayout {
                height: 40px; padding-left: 10px; padding-right: 10px; spacing: 2px;
                Rectangle {
                    width: 36px;
                    Rectangle { x: 4px; y: 9px; width: 22px; height: 22px; border-radius: 6px; background: C.accent;
                        Text { text: "M"; color: white; font-weight: 600; font-size: 13px; }
                    }
                }
                Tab { name: "showcase.md"; active: active-tab == 0; clicked => { active-tab = 0; } }
                Tab { name: "linked.md"; active: active-tab == 1; clicked => { active-tab = 1; } }
                Tab { name: "HANDOFF.md"; active: active-tab == 2; clicked => { active-tab = 2; } }
                VerticalLayout { alignment: center; ToolButton { icon: "\u{E710}"; width: 32px; } }
                Rectangle { horizontal-stretch: 1; }
            }
            // Command bar
            Rectangle {
                height: 40px; background: C.bar;
                HorizontalLayout {
                    padding-left: 8px; padding-right: 8px; padding-top: 4px; padding-bottom: 4px; spacing: 2px;
                    ToolButton { icon: "\u{E8E5}"; label: "Open"; }
                    ToolButton { icon: "\u{E72C}"; }
                    ToolButton { icon: "\u{E81C}"; label: "Recent"; }
                    Sep {}
                    ToolButton { icon: "\u{E8B7}"; label: "Files"; on: files-on; clicked => { files-on = !files-on; } }
                    ToolButton { icon: "\u{E8FD}"; label: "Outline"; on: outline-on; clicked => { outline-on = !outline-on; } }
                    Sep {}
                    ToolButton { icon: "\u{E738}"; }
                    Text { text: "100%"; width: 44px; font-size: 13px; color: C.text; vertical-alignment: center; horizontal-alignment: center; }
                    ToolButton { icon: "\u{E710}"; }
                    Rectangle { horizontal-stretch: 1; }
                    Text { text: "3 of 12"; width: 64px; font-size: 13px; color: C.muted; vertical-alignment: center; }
                    Rectangle {
                        width: 260px; border-radius: 6px; background: C.line;
                        Rectangle {
                            x: 1px; y: 1px; width: parent.width - 2px; height: parent.height - 2px;
                            border-radius: 5px; background: #1c1c1c;
                            Text { x: 10px; text: "\u{E721}"; font-family: C.icons; font-size: 13px; color: C.muted; }
                            input := TextInput {
                                x: 32px; width: parent.width - 40px; font-size: 13px; color: C.text;
                                vertical-alignment: center; selection-background-color: C.accent;
                            }
                            Text { x: 32px; text: "Find in document"; font-size: 13px; color: #777; visible: input.text == "" && !input.has-focus; }
                            Rectangle { x: 6px; y: parent.height - 2px; width: parent.width - 12px; height: 2px; background: C.accent; visible: input.has-focus; }
                        }
                    }
                    ToolButton { icon: "\u{E70E}"; }
                    ToolButton { icon: "\u{E70D}"; }
                    Sep {}
                    ToolButton { icon: "\u{E708}"; label: "Dark"; }
                    ToolButton { icon: "\u{E712}"; }
                }
            }
            // Body
            HorizontalLayout {
                if files-on: Pane {
                    width: 250px; heading: "FILES"; icons: true; selected: 6;
                    rows: [
                        { level: 0, name: "docs", folder: true },
                        { level: 1, name: "HANDOFF.md" },
                        { level: 1, name: "PLAN.md" },
                        { level: 1, name: "PORT_CHECKLIST.md" },
                        { level: 0, name: "fixtures", folder: true },
                        { level: 1, name: "linked.md" },
                        { level: 1, name: "showcase.md" },
                        { level: 0, name: "spikes", folder: true },
                        { level: 0, name: "CLAUDE.md" },
                        { level: 0, name: "README.md" },
                    ];
                }
                Rectangle {
                    background: C.page;
                    horizontal-stretch: 1;
                    clip: true;
                    page := Flickable {
                        content-height: 2400px;
                        content-width: self.width;
                        Image { source: page-image; x: (parent.width - 780px) / 2; width: 780px; height: 2400px; }
                    }
                    // Overlay scrollbar
                    bar := TouchArea {
                        x: parent.width - 16px; width: 16px;
                        Rectangle {
                            property <length> len: max(32px, page.height / 2400px * page.height);
                            width: bar.has-hover ? 8px : 4px;
                            x: parent.width - self.width - 4px;
                            y: -page.content-y / max(1px, 2400px - page.height) * (page.height - len) + 2px;
                            height: len - 4px;
                            border-radius: self.width / 2;
                            background: bar.has-hover ? #777 : #555;
                            animate width { duration: 120ms; }
                        }
                    }
                }
                if outline-on: Pane {
                    width: 230px; heading: "OUTLINE"; selected: 1;
                    rows: [
                        { level: 0, name: "Showcase" }, { level: 1, name: "Text" }, { level: 1, name: "Lists" },
                        { level: 2, name: "Task lists" }, { level: 1, name: "Code" }, { level: 1, name: "Tables" },
                        { level: 1, name: "Math" }, { level: 1, name: "Diagrams" }, { level: 2, name: "Flowchart" },
                        { level: 2, name: "Sequence" }, { level: 1, name: "Images" }, { level: 1, name: "Details" },
                    ];
                }
            }
        }
    }
}

fn main() {
    let started = std::time::Instant::now();
    let ui = Main::new().unwrap();
    if let Ok(image) = slint::Image::load_from_path(std::path::Path::new("../look-shared/content-dark@1.0.png")) {
        ui.set_page_image(image);
    }
    #[cfg(windows)]
    {
        use slint::winit_030::WinitWindowAccessor;
        use slint::winit_030::winit::platform::windows::{Color, WindowExtWindows};
        ui.window().with_winit_window(|w| {
            w.set_title_background_color(Some(Color::from_rgb(0x2e, 0x2e, 0x2e)));
            w.set_title_text_color(Color::from_rgb(0xe6, 0xe6, 0xe6));
        });
    }
    if std::env::var_os("MOCKUP_TRACE").is_some() {
        slint::Timer::single_shot(std::time::Duration::ZERO, move || {
            eprintln!("shell-slint: first frame after {:.1} ms", started.elapsed().as_secs_f64() * 1000.0);
        });
    }
    ui.run().unwrap();
}
