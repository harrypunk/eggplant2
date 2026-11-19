use std::{error::Error, fmt::Display};

use helix_view::Theme as HelixTheme;
use ratatui::style::Style;

use super::{Theme, ThemeName, UiTheme, helix};

#[derive(Debug, Clone, Default)]
pub struct UiThemePatch {
    pub(crate) base: Option<Style>,
    pub(crate) muted: Option<Style>,
    pub(crate) selection: Option<Style>,
    pub(crate) unfocused_selection: Option<Style>,
    pub(crate) border: Option<Style>,
    pub(crate) focused_border: Option<Style>,
    pub(crate) error: Option<Style>,
    pub(crate) file_path: Option<Style>,
    pub(crate) vcs_branch: Option<Style>,
    pub(crate) diff_add: Option<Style>,
    pub(crate) diff_remove: Option<Style>,
    pub(crate) diff_hunk: Option<Style>,
    pub(crate) action_hint: Option<Style>,
    pub(crate) key_hint: Option<Style>,
    pub(crate) popup: Option<Style>,
    pub(crate) status_normal: Option<Style>,
    pub(crate) status_insert: Option<Style>,
}

impl UiThemePatch {
    fn complete(theme: &UiTheme) -> Self {
        Self {
            base: Some(theme.base),
            muted: Some(theme.muted),
            selection: Some(theme.selection),
            unfocused_selection: Some(theme.unfocused_selection),
            border: Some(theme.border),
            focused_border: Some(theme.focused_border),
            error: Some(theme.error),
            file_path: Some(theme.file_path),
            vcs_branch: Some(theme.vcs_branch),
            diff_add: Some(theme.diff_add),
            diff_remove: Some(theme.diff_remove),
            diff_hunk: Some(theme.diff_hunk),
            action_hint: Some(theme.action_hint),
            key_hint: Some(theme.key_hint),
            popup: Some(theme.popup),
            status_normal: Some(theme.status_normal),
            status_insert: Some(theme.status_insert),
        }
    }

    fn resolve(self, fallback: Option<&UiTheme>) -> Result<UiTheme, ThemeResolutionError> {
        // Prefer the provider's value for every field. If neither the provider nor
        // the fallback supplies it, the resulting theme would be incomplete.
        macro_rules! field {
            ($name:ident) => {
                self.$name
                    .or_else(|| fallback.map(|fallback| fallback.$name))
                    .ok_or(ThemeResolutionError::MissingUiFallback)?
            };
        }

        Ok(UiTheme {
            base: field!(base),
            muted: field!(muted),
            selection: field!(selection),
            unfocused_selection: field!(unfocused_selection),
            border: field!(border),
            focused_border: field!(focused_border),
            error: field!(error),
            file_path: field!(file_path),
            vcs_branch: field!(vcs_branch),
            diff_add: field!(diff_add),
            diff_remove: field!(diff_remove),
            diff_hunk: field!(diff_hunk),
            action_hint: field!(action_hint),
            key_hint: field!(key_hint),
            popup: field!(popup),
            status_normal: field!(status_normal),
            status_insert: field!(status_insert),
        })
    }
}

#[derive(Debug)]
pub struct ThemeDraft {
    pub name: String,
    pub builtin: Option<ThemeName>,
    pub ui: UiThemePatch,
    pub syntax: Option<Vec<Style>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeAdapterError(pub String);

impl Display for ThemeAdapterError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for ThemeAdapterError {}

pub trait ThemeAdapter {
    type Document;

    fn adapt(&self, document: &Self::Document) -> Result<ThemeDraft, ThemeAdapterError>;
}

pub struct HelixThemeAdapter {
    name: ThemeName,
}

impl HelixThemeAdapter {
    pub fn new(name: ThemeName) -> Self {
        Self { name }
    }
}

impl ThemeAdapter for HelixThemeAdapter {
    type Document = HelixTheme;

    fn adapt(&self, document: &Self::Document) -> Result<ThemeDraft, ThemeAdapterError> {
        Ok(ThemeDraft {
            name: self.name.as_str().to_string(),
            builtin: Some(self.name),
            ui: UiThemePatch::complete(&helix::ui_theme(document)),
            syntax: Some(helix::syntax_styles(document)),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeResolutionError {
    MissingUiFallback,
    MissingSyntaxFallback,
}

impl Display for ThemeResolutionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingUiFallback => formatter.write_str("theme requires a UI fallback"),
            Self::MissingSyntaxFallback => formatter.write_str("theme requires a syntax fallback"),
        }
    }
}

impl Error for ThemeResolutionError {}

pub struct ThemeResolver;

impl ThemeResolver {
    /// Resolve UI and syntax independently so a provider can replace one while
    /// retaining the other's fallback. Missing fallbacks are explicit errors.
    pub fn resolve(
        draft: ThemeDraft,
        fallback: Option<&Theme>,
    ) -> Result<Theme, ThemeResolutionError> {
        let ui = draft.ui.resolve(fallback.map(|fallback| &fallback.ui))?;
        let syntax = match draft.syntax {
            Some(syntax) => syntax,
            None => fallback
                .ok_or(ThemeResolutionError::MissingSyntaxFallback)?
                .syntax
                .clone(),
        };
        Ok(Theme {
            name: draft.name,
            builtin: draft.builtin,
            ui,
            syntax,
        })
    }
}

#[cfg(test)]
mod tests {
    use helix_view::theme::Loader as HelixThemeLoader;

    use super::*;

    #[test]
    fn ui_only_draft_uses_the_explicit_syntax_fallback() {
        let source = HelixTheme::from(
            toml::from_str::<toml::Value>(include_str!("../../themes/tokyonight.toml")).unwrap(),
        );
        let mut draft = HelixThemeAdapter::new(ThemeName::TokyoNight)
            .adapt(&source)
            .unwrap();
        draft.syntax = None;
        let source_bg = draft.ui.base.and_then(|style| style.bg);
        let fallback = Theme::from_builtin(ThemeName::Default).unwrap();
        let resolved = ThemeResolver::resolve(draft, Some(&fallback)).unwrap();
        let recognized = HelixThemeLoader::new(&[]).default();
        let keyword = recognized.find_highlight_exact("keyword").unwrap();

        assert_eq!(resolved.ui().base.bg, source_bg);
        assert_eq!(
            resolved.highlight(&[keyword]),
            fallback.highlight(&[keyword])
        );
        assert_eq!(resolved.name(), "tokyonight");
    }

    #[test]
    fn ui_only_draft_requires_a_syntax_fallback() {
        let draft = ThemeDraft {
            name: "external".to_string(),
            builtin: None,
            ui: UiThemePatch::default(),
            syntax: None,
        };
        assert_eq!(
            ThemeResolver::resolve(draft, None).unwrap_err(),
            ThemeResolutionError::MissingUiFallback
        );
    }
}
