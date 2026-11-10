use color_eyre::eyre::bail;
use helix_core::syntax::Highlight;
use helix_view::{
    Theme as HelixTheme,
    graphics::{
        Color as HelixColor, Modifier as HelixModifier, Style as HelixStyle, UnderlineStyle,
    },
    theme::Loader as HelixThemeLoader,
};
use ratatui::style::{Color, Modifier, Style};

const DEFAULT: &str = include_str!("../themes/default.toml");
const TOKYONIGHT: &str = include_str!("../themes/tokyonight.toml");
const CATPPUCCIN_MOCHA: &str = include_str!("../themes/catppuccin_mocha.toml");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeName {
    Default,
    TokyoNight,
    CatppuccinMocha,
}

impl ThemeName {
    pub const ALL: [Self; 3] = [Self::Default, Self::TokyoNight, Self::CatppuccinMocha];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::TokyoNight => "tokyonight",
            Self::CatppuccinMocha => "catppuccin_mocha",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == name)
    }
}

/// A fully resolved application theme. Parsing and scope lookup stay out of components.
#[derive(Debug, Clone)]
pub struct Theme {
    name: ThemeName,
    ui: UiTheme,
    // Resolved in the same order as the editor's recognized Helix scopes.
    syntax: Vec<Style>,
}

impl Theme {
    pub fn from_name(name: &str) -> color_eyre::Result<Self> {
        let Some(name) = ThemeName::from_name(name) else {
            let supported = ThemeName::ALL.map(ThemeName::as_str).join("`, `");
            bail!("unknown theme `{name}`; expected `{supported}`");
        };
        Self::from_builtin(name)
    }

    pub fn from_builtin(name: ThemeName) -> color_eyre::Result<Self> {
        let source = match name {
            ThemeName::Default => DEFAULT,
            ThemeName::TokyoNight => TOKYONIGHT,
            ThemeName::CatppuccinMocha => CATPPUCCIN_MOCHA,
        };
        let value: toml::Value = toml::from_str(source)?;
        let source = HelixTheme::from(value);
        let ui = UiTheme::from_helix(&source);
        let syntax = syntax_styles(&source);
        Ok(Self { name, ui, syntax })
    }

    pub fn name(&self) -> ThemeName {
        self.name
    }

    pub fn ui(&self) -> &UiTheme {
        &self.ui
    }

    /// Compose all active syntax scopes, from broad to specific, into one style.
    pub fn highlight(&self, highlights: &[Highlight]) -> Style {
        highlights.iter().fold(Style::new(), |style, highlight| {
            let highlight = self
                .syntax
                .get(highlight.idx())
                .copied()
                .unwrap_or_default();
            style.patch(highlight)
        })
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::from_name("default").expect("the embedded default theme must be valid")
    }
}

/// Semantic UI styles consumed by components without knowledge of source scopes or palettes.
#[derive(Debug, Clone)]
pub struct UiTheme {
    pub(crate) base: Style,
    pub(crate) muted: Style,
    pub(crate) selection: Style,
    pub(crate) border: Style,
    pub(crate) focused_border: Style,
    pub(crate) error: Style,
    pub(crate) key_hint: Style,
    pub(crate) popup: Style,
    pub(crate) status_normal: Style,
    pub(crate) status_insert: Style,
}

impl UiTheme {
    fn from_helix(source: &HelixTheme) -> Self {
        let base = source_style(source, "ui.background")
            .unwrap_or_default()
            .patch(source_style(source, "ui.text").unwrap_or_default());
        let border = source_style(source, "ui.window").unwrap_or_default();
        let status = source_style(source, "ui.statusline").unwrap_or_default();
        Self {
            base,
            muted: source_style(source, "ui.linenr")
                .or_else(|| source_style(source, "ui.text.inactive"))
                .unwrap_or_default(),
            selection: source_style(source, "ui.menu.selected")
                .or_else(|| source_style(source, "ui.selection"))
                .unwrap_or_else(|| Style::new().add_modifier(Modifier::REVERSED)),
            focused_border: border,
            border,
            error: source_style(source, "error")
                .or_else(|| source_style(source, "diagnostic.error"))
                .unwrap_or_else(|| Style::new().fg(Color::Red)),
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

    pub(crate) fn popup_surface(&self) -> Style {
        self.base.patch(self.popup)
    }
}

fn syntax_styles(source: &HelixTheme) -> Vec<Style> {
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
    fn resolves_all_embedded_themes_and_rejects_unknown_names() {
        for name in ["default", "tokyonight", "catppuccin_mocha"] {
            Theme::from_name(name).unwrap();
        }
        let error = Theme::from_name("unknown").unwrap_err().to_string();
        assert!(error.contains("tokyonight"));
        assert!(error.contains("catppuccin_mocha"));
    }

    #[test]
    fn imports_exact_tokyonight_syntax_and_ui_colors() {
        let theme = Theme::from_name("tokyonight").unwrap();
        let recognized = HelixThemeLoader::new(&[]).default();
        let keyword = recognized.find_highlight_exact("keyword").unwrap();

        let style = theme.highlight(&[keyword]);
        assert_eq!(style.fg, Some(Color::Rgb(157, 124, 216)));
        assert!(style.add_modifier.contains(Modifier::ITALIC));
        assert_eq!(theme.ui().base.bg, Some(Color::Rgb(26, 27, 38)));
        assert_eq!(
            theme.highlight(&[Highlight::new(10_000)]),
            Style::new(),
            "unknown recognized-scope IDs must not index into the theme"
        );
    }

    #[test]
    fn imports_catppuccin_palette_and_selection() {
        let theme = Theme::from_name("catppuccin_mocha").unwrap();
        let recognized = HelixThemeLoader::new(&[]).default();
        let keyword = recognized.find_highlight_exact("keyword").unwrap();

        assert_eq!(
            theme.highlight(&[keyword]).fg,
            Some(Color::Rgb(203, 166, 247))
        );
        assert_eq!(theme.ui().base.bg, Some(Color::Rgb(30, 30, 46)));
        assert_eq!(theme.ui().selection.bg, Some(Color::Rgb(69, 71, 90)));
    }

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
