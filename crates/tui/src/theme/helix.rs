use helix_view::{
    Theme as HelixTheme,
    graphics::{
        Color as HelixColor, Modifier as HelixModifier, Style as HelixStyle, UnderlineStyle,
    },
    theme::Loader as HelixThemeLoader,
};
use ratatui::style::{Color, Modifier, Style};

use super::UiTheme;

pub(crate) fn ui_theme(source: &HelixTheme) -> UiTheme {
    let base = source_style(source, "ui.background")
        .unwrap_or_default()
        .patch(source_style(source, "ui.text").unwrap_or_default());
    let border = source_style(source, "ui.window").unwrap_or_default();
    let status = source_style(source, "ui.statusline").unwrap_or_default();
    let selection = source_style(source, "ui.menu.selected")
        .or_else(|| source_style(source, "ui.selection"))
        .unwrap_or_else(|| Style::new().add_modifier(Modifier::REVERSED));
    let unfocused_selection = source_style(source, "ui.text.inactive")
        .map(|inactive| selection.patch(inactive))
        .unwrap_or(selection)
        .remove_modifier(Modifier::BOLD)
        .add_modifier(Modifier::DIM);
    UiTheme {
        base,
        muted: source_style(source, "ui.linenr")
            .or_else(|| source_style(source, "ui.text.inactive"))
            .unwrap_or_default(),
        selection,
        unfocused_selection,
        focused_border: border,
        border,
        error: source_style(source, "error")
            .or_else(|| source_style(source, "diagnostic.error"))
            .unwrap_or_else(|| Style::new().fg(Color::Red)),
        file_path: source_style(source, "ui.text.directory")
            .or_else(|| source_style(source, "ui.text"))
            .unwrap_or_else(|| Style::new().fg(Color::Cyan)),
        vcs_branch: source_style(source, "ui.vcs.branch")
            .or_else(|| source_style(source, "ui.virtual.inlay-hint"))
            .or_else(|| source_style(source, "ui.help"))
            .unwrap_or_default()
            .add_modifier(Modifier::BOLD),
        diff_add: source_style(source, "diff.plus")
            .unwrap_or_else(|| Style::new().fg(Color::Green)),
        diff_remove: source_style(source, "diff.minus")
            .unwrap_or_else(|| Style::new().fg(Color::Red)),
        diff_hunk: source_style(source, "diff.delta")
            .unwrap_or_else(|| Style::new().fg(Color::LightBlue)),
        action_hint: source_style(source, "ui.virtual.jump-label")
            .or_else(|| source_style(source, "ui.help"))
            .unwrap_or_else(|| Style::new().fg(Color::LightBlue))
            .add_modifier(Modifier::BOLD),
        key_hint: source_style(source, "ui.help")
            .unwrap_or_default()
            .add_modifier(Modifier::BOLD),
        popup: source_style(source, "ui.popup")
            .or_else(|| source_style(source, "ui.menu"))
            .unwrap_or_default(),
        status_normal: source_style(source, "ui.statusline.normal").unwrap_or(status),
        status_insert: source_style(source, "ui.statusline.insert").unwrap_or(status),
    }
}

pub(crate) fn syntax_styles(source: &HelixTheme) -> Vec<Style> {
    // DocumentFactory configures syntax queries with Helix's default recognized scopes.
    // Resolve the selected theme against those names instead of using theme-local indices.
    let recognized = HelixThemeLoader::new(&[]).default();
    recognized
        .scopes()
        .iter()
        .map(|scope| source.try_get(scope).map(convert_style).unwrap_or_default())
        .collect()
}

fn source_style(source: &HelixTheme, scope: &str) -> Option<Style> {
    source.try_get_exact(scope).map(convert_style)
}

fn convert_style(source: HelixStyle) -> Style {
    let mut style = Style::new();
    style.fg = source.fg.map(convert_color);
    style.bg = source.bg.map(convert_color);
    style.underline_color = source.underline_color.map(convert_color);
    style.add_modifier = convert_modifier(source.add_modifier);
    style.sub_modifier = convert_modifier(source.sub_modifier);
    match source.underline_style {
        Some(UnderlineStyle::Reset) => style.sub_modifier.insert(Modifier::UNDERLINED),
        Some(_) => style.add_modifier.insert(Modifier::UNDERLINED),
        None => {}
    }
    style
}

fn convert_color(color: HelixColor) -> Color {
    match color {
        HelixColor::Reset => Color::Reset,
        HelixColor::Black => Color::Black,
        HelixColor::Red => Color::Red,
        HelixColor::Green => Color::Green,
        HelixColor::Yellow => Color::Yellow,
        HelixColor::Blue => Color::Blue,
        HelixColor::Magenta => Color::Magenta,
        HelixColor::Cyan => Color::Cyan,
        HelixColor::Gray => Color::DarkGray,
        HelixColor::LightRed => Color::LightRed,
        HelixColor::LightGreen => Color::LightGreen,
        HelixColor::LightYellow => Color::LightYellow,
        HelixColor::LightBlue => Color::LightBlue,
        HelixColor::LightMagenta => Color::LightMagenta,
        HelixColor::LightCyan => Color::LightCyan,
        HelixColor::LightGray => Color::Gray,
        HelixColor::White => Color::White,
        HelixColor::Rgb(red, green, blue) => Color::Rgb(red, green, blue),
        HelixColor::Indexed(index) => Color::Indexed(index),
    }
}

fn convert_modifier(modifier: HelixModifier) -> Modifier {
    [
        (HelixModifier::BOLD, Modifier::BOLD),
        (HelixModifier::DIM, Modifier::DIM),
        (HelixModifier::ITALIC, Modifier::ITALIC),
        (HelixModifier::SLOW_BLINK, Modifier::SLOW_BLINK),
        (HelixModifier::RAPID_BLINK, Modifier::RAPID_BLINK),
        (HelixModifier::REVERSED, Modifier::REVERSED),
        (HelixModifier::HIDDEN, Modifier::HIDDEN),
        (HelixModifier::CROSSED_OUT, Modifier::CROSSED_OUT),
    ]
    .into_iter()
    .fold(Modifier::empty(), |mut result, (source, target)| {
        if modifier.contains(source) {
            result.insert(target);
        }
        result
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_underlines_and_modifier_removals() {
        let source = HelixStyle::new()
            .underline_color(HelixColor::Rgb(1, 2, 3))
            .add_modifier(HelixModifier::BOLD);
        let mut source = source;
        source.underline_style = Some(UnderlineStyle::Curl);
        source.sub_modifier = HelixModifier::ITALIC;

        let style = convert_style(source);
        assert_eq!(style.underline_color, Some(Color::Rgb(1, 2, 3)));
        assert!(style.add_modifier.contains(Modifier::BOLD));
        assert!(style.add_modifier.contains(Modifier::UNDERLINED));
        assert!(style.sub_modifier.contains(Modifier::ITALIC));
    }
}
