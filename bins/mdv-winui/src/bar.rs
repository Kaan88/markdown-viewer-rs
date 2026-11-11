//! The command bar and the title row (layout A in the mockups): the logo and tabs drawn in the
//! title bar, and one row of commands under it instead of a menu bar.
//!
//! The controls come from `bar_bindings.rs`, a second generated module (see `gen/`), so the
//! hand-edited `bindings.rs` stays as it is. Objects move between the two with `cast()`.
use crate::bar_bindings as b;
use crate::bindings::{
    Brush, Color, ContentControl, FlyoutBase, Grid, GridLength, GridUnitType, IPanel, MenuFlyout,
    MenuFlyoutItem, MenuFlyoutItemBase, SolidColorBrush, UIElement,
};
use crate::tabs;

use windows_core::{Interface, Result};

/// Segoe Fluent Icons code points.
pub mod glyph {
    pub const OPEN: &str = "\u{E838}";
    pub const RELOAD: &str = "\u{E72C}";
    pub const FILES: &str = "\u{E90C}";
    pub const OUTLINE: &str = "\u{E8FD}";
    pub const RECENT: &str = "\u{E81C}";
    pub const ZOOM_OUT: &str = "\u{E71F}";
    pub const ZOOM_IN: &str = "\u{E8A3}";
    pub const THEME: &str = "\u{E793}";
    pub const MORE: &str = "\u{E712}";
    pub const SYSTEM: &str = "\u{E7F4}";
    pub const LIGHT: &str = "\u{E706}";
    pub const DARK: &str = "\u{E708}";
    pub const UP: &str = "\u{E70E}";
    pub const DOWN: &str = "\u{E70D}";
}

fn thickness(left: f64, top: f64, right: f64, bottom: f64) -> b::Thickness {
    b::Thickness {
        left,
        top,
        right,
        bottom,
    }
}

fn transparent() -> Result<b::Brush> {
    let brush = SolidColorBrush::new()?;
    brush.SetColor(Color {
        a: 0,
        r: 0,
        g: 0,
        b: 0,
    })?;
    brush.cast::<Brush>()?.cast()
}

pub fn append(panel: &impl Interface, child: &impl Interface) -> Result<()> {
    panel
        .cast::<IPanel>()?
        .Children()?
        .Append(&child.cast::<UIElement>()?)
}

pub fn row() -> Result<b::StackPanel> {
    let panel = b::StackPanel::new()?;
    panel.SetOrientation(b::Orientation::Horizontal)?;
    panel.SetSpacing(2.0)?;
    panel
        .cast::<b::IFrameworkElement>()?
        .SetVerticalAlignment(b::VerticalAlignment::Center)?;
    Ok(panel)
}

pub fn text(value: &str, size: f64) -> Result<b::TextBlock> {
    let block = b::TextBlock::new()?;
    block.SetText(value)?;
    block.SetFontSize(size)?;
    block
        .cast::<b::IFrameworkElement>()?
        .SetVerticalAlignment(b::VerticalAlignment::Center)?;
    Ok(block)
}

fn icon(glyph: &str) -> Result<b::FontIcon> {
    let icon = b::FontIcon::new()?;
    icon.SetGlyph(glyph)?;
    icon.SetFontSize(16.0)?;
    Ok(icon)
}

/// A flat bar button: an icon, an optional label beside it, a tooltip and an accessible name.
pub fn button(glyph: &str, label: Option<&str>, tip: &str) -> Result<b::Button> {
    Ok(button_parts(glyph, label, tip)?.0)
}

/// `button`, also handing back its icon and label so they can change later (the Theme button).
pub fn button_parts(
    glyph: &str,
    label: Option<&str>,
    tip: &str,
) -> Result<(b::Button, b::FontIcon, Option<b::TextBlock>)> {
    let button = b::Button::new()?;
    let content = row()?;
    content.SetSpacing(8.0)?;
    let glyph_icon = icon(glyph)?;
    append(&content, &glyph_icon)?;
    let mut text_block = None;
    if let Some(label) = label {
        let block = text(label, 14.0)?;
        append(&content, &block)?;
        text_block = Some(block);
    }
    button.cast::<ContentControl>()?.SetContent(&content)?;
    let control: b::IControl = button.cast()?;
    control.SetBackground(&transparent()?)?;
    control.SetBorderThickness(thickness(0.0, 0.0, 0.0, 0.0))?;
    control.SetPadding(thickness(
        if label.is_some() { 10.0 } else { 0.0 },
        0.0,
        if label.is_some() { 12.0 } else { 0.0 },
        0.0,
    ))?;
    let element: b::IFrameworkElement = button.cast()?;
    element.SetHeight(36.0)?;
    element.SetMinWidth(36.0)?;
    element.SetVerticalAlignment(b::VerticalAlignment::Center)?;
    tabs::set_tooltip(&button, tip)?;
    b::AutomationProperties::SetName(&button.cast::<b::DependencyObject>()?, tip)?;
    Ok((button, glyph_icon, text_block))
}

pub fn on_click(button: &b::Button, action: impl Fn() + 'static) -> Result<()> {
    button
        .cast::<b::ButtonBase>()?
        .Click(move |_, _| action())?
        .forget();
    Ok(())
}

/// Opens `menu` under the button when it is clicked, lined up with its left edge, or with its right
/// edge for a button at the right of the bar (XAML otherwise picks a side, often above).
pub fn set_menu(button: &b::Button, menu: &MenuFlyout, right: bool) -> Result<()> {
    let flyout: b::FlyoutBase = menu.cast::<FlyoutBase>()?.cast()?;
    flyout.SetPlacement(if right {
        b::FlyoutPlacementMode::BottomEdgeAlignedRight
    } else {
        b::FlyoutPlacementMode::BottomEdgeAlignedLeft
    })?;
    button.SetFlyout(&flyout)
}

/// A flat, full-width, left-aligned row button, for the outline pane.
pub fn list_button(label: &str, indent: f64) -> Result<b::Button> {
    let button = b::Button::new()?;
    button
        .cast::<ContentControl>()?
        .SetContent(&text(label, 13.0)?)?;
    let control: b::IControl = button.cast()?;
    control.SetBackground(&transparent()?)?;
    control.SetBorderThickness(thickness(0.0, 0.0, 0.0, 0.0))?;
    control.SetPadding(thickness(10.0 + indent, 5.0, 8.0, 5.0))?;
    control.SetHorizontalContentAlignment(b::HorizontalAlignment::Left)?;
    let element: b::IFrameworkElement = button.cast()?;
    element.SetHorizontalAlignment(b::HorizontalAlignment::Stretch)?;
    b::AutomationProperties::SetName(&button.cast::<b::DependencyObject>()?, label)?;
    Ok(button)
}

/// A vertical list in a scrolling area, for the outline pane. Returns the area and the list.
pub fn scrolling_list() -> Result<(b::ScrollViewer, b::StackPanel)> {
    let list = b::StackPanel::new()?;
    list.SetOrientation(b::Orientation::Vertical)?;
    list.SetSpacing(0.0)?;
    list.cast::<b::IFrameworkElement>()?
        .SetMargin(thickness(4.0, 8.0, 4.0, 8.0))?;
    let area = b::ScrollViewer::new()?;
    area.cast::<ContentControl>()?.SetContent(&list)?;
    Ok((area, list))
}

/// Whether the right (or middle) button is down for a pointer event's point.
pub fn buttons(point: &crate::bindings::PointerPoint) -> (bool, bool) {
    let Ok(properties) = point.Properties() else {
        return (false, false);
    };
    let Ok(properties) = properties.cast::<b::IPointerPointProperties>() else {
        return (false, false);
    };
    (
        properties.IsRightButtonPressed().unwrap_or(false),
        properties.IsMiddleButtonPressed().unwrap_or(false),
    )
}

pub fn separator() -> Result<Grid> {
    let line = Grid::new()?;
    let brush = SolidColorBrush::new()?;
    brush.SetColor(Color {
        a: 60,
        r: 128,
        g: 128,
        b: 128,
    })?;
    line.cast::<IPanel>()?
        .SetBackground(&brush.cast::<Brush>()?)?;
    let element: b::IFrameworkElement = line.cast()?;
    element.SetWidth(1.0)?;
    element.SetHeight(24.0)?;
    element.SetMargin(thickness(6.0, 0.0, 6.0, 0.0))?;
    element.SetVerticalAlignment(b::VerticalAlignment::Center)?;
    Ok(line)
}

pub fn menu_item(
    menu: &MenuFlyout,
    label: &str,
    action: impl Fn() + 'static,
) -> Result<MenuFlyoutItem> {
    let item = MenuFlyoutItem::new()?;
    item.SetText(label)?;
    item.Click(move |_, _| action())?.forget();
    menu.Items()?.Append(&item.cast::<MenuFlyoutItemBase>()?)?;
    Ok(item)
}

/// The logo mark and the app name, at the left of the title row.
pub fn brand() -> Result<b::StackPanel> {
    let panel = row()?;
    panel.SetSpacing(8.0)?;
    panel
        .cast::<b::IFrameworkElement>()?
        .SetMargin(thickness(14.0, 0.0, 14.0, 0.0))?;
    // The logo, drawn to a PNG (the same mark as the Qt shell's); the plain glyph if that fails.
    match logo() {
        Some(mark) => append(&panel, &mark)?,
        None => append(&panel, &icon("\u{E8A5}")?)?,
    }
    let name = text("mdview", 13.0)?;
    name.SetFontWeight(b::FontWeight { weight: 600 })?;
    append(&panel, &name)?;
    Ok(panel)
}

/// The mdview mark: a blue rounded square with an "M" and a down arrow, as a 20 px icon.
pub fn logo_sized(size: f64) -> Option<b::BitmapIcon> {
    let mark = logo()?;
    let element: b::IFrameworkElement = mark.cast().ok()?;
    element.SetWidth(size).ok()?;
    element.SetHeight(size).ok()?;
    Some(mark)
}

fn logo() -> Option<b::BitmapIcon> {
    let dir = std::env::temp_dir().join("mdview-icons");
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join("logo-winui.png");
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24"><rect x="2" y="2" width="20" height="20" rx="5" fill="#005fb8"/><path d="M6.5 16V8.5l3 3.5 3-3.5V16M16.5 8.5v7.5m-2-2 2 2.2 2-2.2" fill="none" stroke="#ffffff" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/></svg>"##;
    let tree = resvg::usvg::Tree::from_str(svg, &resvg::usvg::Options::default()).ok()?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(48, 48)?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(2.0, 2.0),
        &mut pixmap.as_mut(),
    );
    pixmap.save_png(&path).ok()?;
    let uri = format!("file:///{}", path.display().to_string().replace('\\', "/"));
    let mark = b::BitmapIcon::new().ok()?;
    mark.SetShowAsMonochrome(false).ok()?;
    mark.SetUriSource(&b::Uri::CreateUri(&uri).ok()?).ok()?;
    let element: b::IFrameworkElement = mark.cast().ok()?;
    element.SetWidth(20.0).ok()?;
    element.SetHeight(20.0).ok()?;
    Some(mark)
}

/// A grid column definition, for the title row.
pub fn column(grid: &Grid, unit: GridUnitType, value: f64) -> Result<()> {
    let column = crate::bindings::ColumnDefinition::new()?;
    column.SetWidth(GridLength {
        value,
        grid_unit_type: unit,
    })?;
    grid.ColumnDefinitions()?.Append(&column)
}

/// The find box: a text field 260 px wide.
pub fn find_box() -> Result<b::ITextBox> {
    let field = crate::bindings::TextBox::new()?;
    field.SetPlaceholderText("Find in document  (Ctrl+F)")?;
    let element: b::IFrameworkElement = field.cast()?;
    element.SetWidth(260.0)?;
    element.SetVerticalAlignment(b::VerticalAlignment::Center)?;
    b::AutomationProperties::SetName(&field.cast::<b::DependencyObject>()?, "Find in document")?;
    field.cast()
}

pub fn set_margin(element: &impl Interface, left: f64, right: f64) -> Result<()> {
    element
        .cast::<b::IFrameworkElement>()?
        .SetMargin(thickness(left, 0.0, right, 0.0))
}

pub fn extend_into_title_bar(window: &crate::bindings::Window, drag: &Grid) -> Result<()> {
    let window: b::IWindow = window.cast()?;
    window.SetExtendsContentIntoTitleBar(true)?;
    window.SetTitleBar(&drag.cast::<b::UIElement>()?)
}

pub fn set_tab_strip_footer(
    view: &crate::bindings::TabView,
    footer: &impl Interface,
) -> Result<()> {
    view.cast::<b::ITabView>()?
        .SetTabStripFooter(&footer.cast::<windows_core::IInspectable>()?)
}
