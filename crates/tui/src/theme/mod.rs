mod builtin;
mod discovery;
pub mod ghostty;
mod helix;
mod loader;
mod persistence;
pub mod provider;

pub use discovery::start_discovery;
pub use loader::{ThemeLoadState, start_loading, start_loading_and_persist};
pub use persistence::{ThemePersistenceState, persist_theme};

use color_eyre::eyre::bail;
use helix_core::syntax::Highlight;
use ratatui::style::Style;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ThemeName {
    #[default]
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
    name: String,
    builtin: Option<ThemeName>,
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
        builtin::resolve(name)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn builtin(&self) -> Option<ThemeName> {
        self.builtin
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
    pub(crate) unfocused_selection: Style,
    pub(crate) border: Style,
    pub(crate) focused_border: Style,
    pub(crate) error: Style,
    pub(crate) file_path: Style,
    pub(crate) vcs_branch: Style,
    pub(crate) diff_add: Style,
    pub(crate) diff_remove: Style,
    pub(crate) diff_hunk: Style,
    pub(crate) action_hint: Style,
    pub(crate) key_hint: Style,
    pub(crate) popup: Style,
    pub(crate) status_normal: Style,
    pub(crate) status_insert: Style,
}

impl UiTheme {
    pub(crate) fn popup_surface(&self) -> Style {
        self.base.patch(self.popup)
    }
}

#[cfg(test)]
mod tests {
    use helix_view::theme::Loader as HelixThemeLoader;
    use ratatui::style::{Color, Modifier};

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
}
