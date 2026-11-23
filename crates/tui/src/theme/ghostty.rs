use ratatui::style::{Color, Modifier, Style};

use super::provider::{ThemeAdapter, ThemeAdapterError, ThemeDraft, UiThemePatch};

const RED: usize = 1;
const GREEN: usize = 2;
const BLUE: usize = 4;
const BRIGHT_BLACK: usize = 8;
const BRIGHT_YELLOW: usize = 11;
const BRIGHT_BLUE: usize = 12;
const BRIGHT_MAGENTA: usize = 13;

#[derive(Debug, Clone)]
pub struct GhosttyDocument {
    background: Color,
    foreground: Color,
    selection_background: Option<Color>,
    selection_foreground: Option<Color>,
    palette: [Option<Color>; 16],
}

#[derive(Debug, Default, Clone)]
pub struct GhosttyThemeAdapter {
    pub name: String,
}

impl GhosttyThemeAdapter {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }

    pub fn parse(input: &str) -> Result<GhosttyDocument, ThemeAdapterError> {
        let mut background = None;
        let mut foreground = None;
        let mut selection_background = None;
        let mut selection_foreground = None;
        let mut palette = [None; 16];

        for (index, line) in input.lines().enumerate() {
            let line_number = index + 1;
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                return Err(ThemeAdapterError(format!(
                    "invalid Ghostty config at line {line_number}: expected `key = value`"
                )));
            };
            let key = key.trim();
            let value = value.trim();
            match key {
                "background" => background = Some(parse_color(value, line_number)?),
                "foreground" => foreground = Some(parse_color(value, line_number)?),
                "selection-background" => {
                    selection_background = Some(parse_color(value, line_number)?)
                }
                "selection-foreground" => {
                    selection_foreground = Some(parse_color(value, line_number)?)
                }
                "palette" => {
                    let Some((index, color)) = value.split_once('=') else {
                        return Err(ThemeAdapterError(format!(
                            "invalid Ghostty palette at line {line_number}: expected `N=COLOR`"
                        )));
                    };
                    let index = parse_palette_index(index.trim(), line_number)?;
                    if index < 16 {
                        palette[index as usize] = Some(parse_color(color.trim(), line_number)?);
                    }
                }
                // Ghostty themes share the general config format. Unsupported
                // options have no effect on Eggplant and are intentionally ignored.
                _ => {}
            }
        }

        Ok(GhosttyDocument {
            background: background.ok_or_else(|| {
                ThemeAdapterError("Ghostty theme is missing `background`".to_string())
            })?,
            foreground: foreground.ok_or_else(|| {
                ThemeAdapterError("Ghostty theme is missing `foreground`".to_string())
            })?,
            selection_background,
            selection_foreground,
            palette,
        })
    }
}

impl ThemeAdapter for GhosttyThemeAdapter {
    type Document = GhosttyDocument;

    fn adapt(&self, document: &Self::Document) -> Result<ThemeDraft, ThemeAdapterError> {
        let base = Style::new().fg(document.foreground).bg(document.background);
        let selection = document.selection_background.map(|background| {
            Style::new()
                .fg(document.selection_foreground.unwrap_or(document.foreground))
                .bg(background)
        });
        let unfocused_selection = selection.map(|selection| {
            selection
                .patch(Style::new().add_modifier(Modifier::DIM))
                .remove_modifier(Modifier::BOLD)
        });
        let palette = |index: usize| document.palette[index].map(|color| Style::new().fg(color));
        let status = |index: usize| {
            document.palette[index].map(|color| {
                Style::new()
                    .fg(document.background)
                    .bg(color)
                    .add_modifier(Modifier::BOLD)
            })
        };

        Ok(ThemeDraft {
            name: self.name.to_string(),
            builtin: None,
            ui: UiThemePatch {
                base: Some(base),
                muted: palette(BRIGHT_BLACK),
                selection,
                cursor_line: selection
                    .and_then(|style| style.bg)
                    .map(|bg| Style::new().bg(bg)),
                unfocused_selection,
                border: palette(BRIGHT_BLACK),
                focused_border: palette(BRIGHT_BLUE),
                error: palette(RED),
                warning: palette(BRIGHT_YELLOW),
                file_path: palette(BRIGHT_BLACK),
                vcs_branch: palette(BRIGHT_MAGENTA).map(|style| style.add_modifier(Modifier::BOLD)),
                diff_add: palette(GREEN),
                diff_remove: palette(RED),
                diff_hunk: palette(BLUE),
                action_hint: palette(BRIGHT_BLUE).map(|style| style.add_modifier(Modifier::BOLD)),
                key_hint: palette(BRIGHT_YELLOW).map(|style| style.add_modifier(Modifier::BOLD)),
                popup: Some(base),
                status_normal: status(BLUE),
                status_insert: status(GREEN),
            },
            syntax: None,
        })
    }
}

fn parse_palette_index(input: &str, line: usize) -> Result<u16, ThemeAdapterError> {
    let (radix, digits) = if let Some(digits) = input
        .strip_prefix("0x")
        .or_else(|| input.strip_prefix("0X"))
    {
        (16, digits)
    } else if let Some(digits) = input
        .strip_prefix("0o")
        .or_else(|| input.strip_prefix("0O"))
    {
        (8, digits)
    } else if let Some(digits) = input
        .strip_prefix("0b")
        .or_else(|| input.strip_prefix("0B"))
    {
        (2, digits)
    } else {
        (10, input)
    };
    let index = u16::from_str_radix(digits, radix).map_err(|_| {
        ThemeAdapterError(format!(
            "invalid Ghostty palette index `{input}` at line {line}"
        ))
    })?;
    if index > 255 {
        return Err(ThemeAdapterError(format!(
            "Ghostty palette index {index} exceeds 255 at line {line}"
        )));
    }
    Ok(index)
}

fn parse_color(input: &str, line: usize) -> Result<Color, ThemeAdapterError> {
    let input = input.trim();
    let hexadecimal = input.strip_prefix('#').unwrap_or(input);
    if [3, 6, 10, 13].contains(&hexadecimal.len())
        && hexadecimal
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        let channels = match hexadecimal.len() {
            3 => [&hexadecimal[0..1], &hexadecimal[1..2], &hexadecimal[2..3]],
            6 => [&hexadecimal[0..2], &hexadecimal[2..4], &hexadecimal[4..6]],
            10 => [&hexadecimal[0..3], &hexadecimal[3..6], &hexadecimal[6..9]],
            13 => [&hexadecimal[0..4], &hexadecimal[4..8], &hexadecimal[8..12]],
            _ => unreachable!(),
        };
        return Ok(Color::Rgb(
            parse_color_channel(channels[0], line)?,
            parse_color_channel(channels[1], line)?,
            parse_color_channel(channels[2], line)?,
        ));
    }
    match input {
        "black" => Ok(Color::Rgb(0, 0, 0)),
        "red" => Ok(Color::Rgb(255, 0, 0)),
        "green" => Ok(Color::Rgb(0, 255, 0)),
        "yellow" => Ok(Color::Rgb(255, 255, 0)),
        "blue" => Ok(Color::Rgb(0, 0, 255)),
        "magenta" => Ok(Color::Rgb(255, 0, 255)),
        "cyan" => Ok(Color::Rgb(0, 255, 255)),
        "white" => Ok(Color::Rgb(255, 255, 255)),
        _ => Err(ThemeAdapterError(format!(
            "unsupported Ghostty color `{input}` at line {line}"
        ))),
    }
}

fn parse_color_channel(input: &str, line: usize) -> Result<u8, ThemeAdapterError> {
    let value = u32::from_str_radix(input, 16).map_err(|_| {
        ThemeAdapterError(format!(
            "invalid Ghostty color channel `{input}` at line {line}"
        ))
    })?;
    let maximum = (1_u32 << (input.len() * 4)) - 1;
    Ok((value * 255 / maximum) as u8)
}

#[cfg(test)]
mod tests {
    use helix_view::theme::Loader as HelixThemeLoader;

    use super::*;
    use crate::theme::{Theme, ThemeName, provider::ThemeResolver};

    const EXAMPLE: &str = r#"
# UI colors
background = #101010
foreground = #f0f0f0
selection-background = #202040
palette = 1=#ff0000
palette = 2=#00ff00
palette = 3=#ffff00
palette = 4=#0000ff
palette = 5=#ff00ff
palette = 8=#606060
palette = 11=#ffff00
palette = 12=#8080ff
palette = 13=#ff00ff
"#;

    #[test]
    fn adapts_ghostty_config_and_resolves_syntax_fallback() {
        let document = GhosttyThemeAdapter::parse(EXAMPLE).unwrap();
        let draft = GhosttyThemeAdapter::new("ghostty-test")
            .adapt(&document)
            .unwrap();
        let fallback = Theme::from_builtin(ThemeName::Default).unwrap();
        let theme = ThemeResolver::resolve(draft, Some(&fallback)).unwrap();
        let recognized = HelixThemeLoader::new(&[]).default();
        let keyword = recognized.find_highlight_exact("keyword").unwrap();

        assert_eq!(theme.name(), "ghostty-test");
        assert_eq!(theme.builtin(), None);
        assert_eq!(theme.ui().base.bg, Some(Color::Rgb(16, 16, 16)));
        assert_eq!(theme.ui().selection.bg, Some(Color::Rgb(32, 32, 64)));
        assert_eq!(theme.ui().error.fg, Some(Color::Rgb(255, 0, 0)));
        assert_eq!(theme.ui().vcs_branch.fg, Some(Color::Rgb(255, 0, 255)));
        assert_eq!(theme.highlight(&[keyword]), fallback.highlight(&[keyword]));
    }

    #[test]
    fn parses_palette_bases_and_color_precision() {
        let document = GhosttyThemeAdapter::parse(
            "background = #123\nforeground = white\npalette = 0x1=#aabbcc\npalette = 0b10=#010203",
        )
        .unwrap();
        assert_eq!(document.background, Color::Rgb(17, 34, 51));
        assert_eq!(document.foreground, Color::Rgb(255, 255, 255));
        assert_eq!(document.palette[1], Some(Color::Rgb(170, 187, 204)));
        assert_eq!(document.palette[2], Some(Color::Rgb(1, 2, 3)));
    }

    #[test]
    fn rejects_missing_colors_invalid_indices_and_unknown_names() {
        assert!(GhosttyThemeAdapter::parse("background = #000000").is_err());
        assert!(
            GhosttyThemeAdapter::parse(
                "background = #000000\nforeground = #ffffff\npalette = 256=#000000"
            )
            .is_err()
        );
        assert!(
            GhosttyThemeAdapter::parse("background = not-a-color\nforeground = #ffffff").is_err()
        );
    }
}
