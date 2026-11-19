use helix_view::Theme as HelixTheme;

use super::{
    Theme, ThemeName,
    provider::{HelixThemeAdapter, ThemeAdapter, ThemeResolver},
};

const DEFAULT: &str = include_str!("../../themes/default.toml");
const TOKYONIGHT: &str = include_str!("../../themes/tokyonight.toml");
const CATPPUCCIN_MOCHA: &str = include_str!("../../themes/catppuccin_mocha.toml");

pub(crate) fn resolve(name: ThemeName) -> color_eyre::Result<Theme> {
    let source = match name {
        ThemeName::Default => DEFAULT,
        ThemeName::TokyoNight => TOKYONIGHT,
        ThemeName::CatppuccinMocha => CATPPUCCIN_MOCHA,
    };
    let value: toml::Value = toml::from_str(source)?;
    let source = HelixTheme::from(value);
    let draft = HelixThemeAdapter::new(name).adapt(&source)?;
    Ok(ThemeResolver::resolve(draft, None).expect("built-in themes must be complete"))
}
