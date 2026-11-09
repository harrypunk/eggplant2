use color_eyre::eyre::bail;
use editor::HighlightKind;
use ratatui::style::{Color, Modifier, Style};

/// Built-in mapping from semantic highlights to terminal styles.
#[derive(Debug, Default, Clone, Copy)]
pub struct Theme;

impl Theme {
    pub fn from_name(name: &str) -> color_eyre::Result<Self> {
        match name {
            "default" => Ok(Self),
            other => bail!("unknown theme `{other}`; expected `default`"),
        }
    }

    pub fn highlight(self, kind: HighlightKind) -> Style {
        match kind {
            HighlightKind::Keyword => Style::new().fg(Color::Magenta).add_modifier(Modifier::BOLD),
            HighlightKind::String => Style::new().fg(Color::Green),
            HighlightKind::Comment => Style::new()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
            HighlightKind::Function => Style::new().fg(Color::Blue),
            HighlightKind::Type => Style::new().fg(Color::Yellow),
            HighlightKind::Constant => Style::new().fg(Color::Cyan),
            HighlightKind::Number => Style::new().fg(Color::LightYellow),
            HighlightKind::Boolean => Style::new().fg(Color::LightRed),
            HighlightKind::Variable => Style::new(),
            HighlightKind::Operator => Style::new().fg(Color::LightMagenta),
            HighlightKind::Macro => Style::new().fg(Color::LightBlue),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_theme_is_explicit_and_other_names_are_rejected() {
        assert!(Theme::from_name("default").is_ok());
        assert!(Theme::from_name("unknown").is_err());
    }
}
