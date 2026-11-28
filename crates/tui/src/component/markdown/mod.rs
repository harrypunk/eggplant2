use markdown_ratatui::{
    Document, Layout, LayoutError, LayoutOptions, ParseError, Theme, ViewState,
};
use ratatui::{buffer::Buffer, layout::Rect, style::Modifier, widgets::StatefulWidget};

use crate::theme::UiTheme;

/// A width-wrapped Markdown snapshot owned independently of its source text.
#[derive(Debug)]
pub struct PreparedMarkdown {
    layout: Layout,
    line_count: usize,
}

#[derive(Debug)]
pub enum MarkdownError {
    Parse(ParseError),
    Layout(LayoutError),
}

enum MarkdownRole {
    Answer,
    Thinking,
}

impl PreparedMarkdown {
    pub fn parse(source: &str, width: u16, theme: &UiTheme) -> Result<Self, MarkdownError> {
        Self::parse_with_role(source, width, theme, MarkdownRole::Answer)
    }

    pub fn parse_thinking(
        source: &str,
        width: u16,
        theme: &UiTheme,
    ) -> Result<Self, MarkdownError> {
        Self::parse_with_role(source, width, theme, MarkdownRole::Thinking)
    }

    fn parse_with_role(
        source: &str,
        width: u16,
        theme: &UiTheme,
        role: MarkdownRole,
    ) -> Result<Self, MarkdownError> {
        let document = Document::parse(source).map_err(MarkdownError::Parse)?;
        let (text, heading, code) = match role {
            MarkdownRole::Answer => (
                theme.base,
                theme.action_hint.add_modifier(Modifier::BOLD),
                theme.file_path,
            ),
            MarkdownRole::Thinking => (
                theme.muted.add_modifier(Modifier::ITALIC),
                theme.muted.add_modifier(Modifier::BOLD | Modifier::ITALIC),
                theme.muted.add_modifier(Modifier::ITALIC),
            ),
        };
        let options = LayoutOptions {
            theme: Theme {
                text,
                heading,
                link: theme.file_path.add_modifier(Modifier::UNDERLINED),
                code,
                diagram: theme.action_hint,
                muted: theme.muted,
                selected: theme.selection,
            },
            ..LayoutOptions::default()
        };
        let layout = Layout::new(&document, width, &options).map_err(MarkdownError::Layout)?;
        let trailing_blank = layout
            .plain_lines()
            .iter()
            .rev()
            .take_while(|line| line.is_empty())
            .count();
        let line_count = layout.line_count().saturating_sub(trailing_blank);
        Ok(Self { layout, line_count })
    }

    pub fn line_count(&self) -> usize {
        self.line_count
    }

    /// Render a prepared snapshot with a document-relative top clip.
    pub fn render(&self, area: Rect, top_clip: usize, buffer: &mut Buffer) {
        let mut state = ViewState::default();
        state.scroll_forward(top_clip);
        self.layout.widget().render(area, buffer, &mut state);
    }

    #[cfg(test)]
    fn plain_lines(&self) -> Vec<String> {
        self.layout.plain_lines()
    }
}

#[cfg(test)]
mod tests {
    use unicode_width::UnicodeWidthStr;

    use super::*;
    use crate::theme::Theme as EditorTheme;

    #[test]
    fn open_source_layout_renders_common_blocks_and_inline_styles() {
        let theme = EditorTheme::default();
        let markdown = PreparedMarkdown::parse(
            "# Title\n\n**bold** and `code`\n\n- one\n- two\n\n```rust\nfn main() {}\n```",
            40,
            theme.ui(),
        )
        .unwrap();
        let text = markdown.plain_lines().join("\n");

        assert!(text.contains("Title"));
        assert!(text.contains("bold"));
        assert!(text.contains("code"));
        assert!(text.contains("one"));
        assert!(text.contains("fn main()"));
        assert!(!text.contains("**bold**"));
        assert!(!text.contains("# Title"));
    }

    #[test]
    fn gfm_and_streaming_snapshots_remain_readable() {
        let theme = EditorTheme::default();
        let markdown = PreparedMarkdown::parse(
            "> quoted **text**\n\n- [x] done\n\n| Name | Value |\n| :--- | ---: |\n| 界 | 1 |",
            40,
            theme.ui(),
        )
        .unwrap();
        let text = markdown.plain_lines().join("\n");
        assert!(text.contains("quoted"));
        assert!(text.contains("[x] done"));
        assert!(text.contains("Name"));
        assert!(text.contains("Value"));
        assert!(text.contains('界'));
        assert!(!text.contains("**text**"));

        let streaming = PreparedMarkdown::parse("```rust\nfn main() {\n", 40, theme.ui()).unwrap();
        assert!(streaming.plain_lines().join("\n").contains("fn main()"));
    }

    #[test]
    fn inline_styles_are_applied_to_terminal_cells() {
        let theme = EditorTheme::default();
        let markdown = PreparedMarkdown::parse("**bold**", 20, theme.ui()).unwrap();
        let mut buffer = Buffer::empty(Rect::new(0, 0, 20, 1));
        markdown.render(Rect::new(0, 0, 20, 1), 0, &mut buffer);

        assert_eq!(markdown.plain_lines()[0], "bold");
        assert!(
            buffer.content()[..4]
                .iter()
                .all(|cell| { cell.style().add_modifier.contains(Modifier::BOLD) })
        );
    }

    #[test]
    fn thinking_markdown_uses_the_semantic_muted_italic_style() {
        let theme = EditorTheme::from_name("tokyonight").unwrap();
        let markdown = PreparedMarkdown::parse_thinking("plan", 20, theme.ui()).unwrap();
        let mut buffer = Buffer::empty(Rect::new(0, 0, 20, 1));
        markdown.render(Rect::new(0, 0, 20, 1), 0, &mut buffer);

        let style = buffer.content()[0].style();
        assert!(style.add_modifier.contains(Modifier::ITALIC), "{style:?}");
        assert_eq!(style.fg, theme.ui().muted.fg, "{style:?}");
    }

    #[test]
    fn layout_is_unicode_width_bounded_and_renders_a_top_clip() {
        let theme = EditorTheme::default();
        let markdown = PreparedMarkdown::parse(
            "A long paragraph with enough words to wrap across several narrow rows.",
            14,
            theme.ui(),
        )
        .unwrap();
        assert!(markdown.line_count() > 1);
        assert!(
            markdown
                .plain_lines()
                .iter()
                .all(|line| UnicodeWidthStr::width(line.as_str()) <= 14)
        );

        let mut buffer = Buffer::empty(Rect::new(0, 0, 14, 1));
        markdown.render(Rect::new(0, 0, 14, 1), 1, &mut buffer);
        let rendered = buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert_eq!(rendered.trim_end(), markdown.plain_lines()[1]);
    }
}
