use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::Line,
    widgets::{Block, BorderType, Paragraph, Widget},
};

use app::language::{Diagnostic, DiagnosticSeverity};

use crate::{data_object::FocusState, diagnostics::DiagnosticsState, theme::UiTheme};

pub struct DiagnosticsPanel<'a> {
    diagnostics: &'a DiagnosticsState,
    root: Option<&'a std::path::Path>,
    focus: FocusState,
    styles: &'a UiTheme,
}

impl<'a> DiagnosticsPanel<'a> {
    pub fn new(
        diagnostics: &'a DiagnosticsState,
        root: Option<&'a std::path::Path>,
        focus: FocusState,
        styles: &'a UiTheme,
    ) -> Self {
        Self {
            diagnostics,
            root,
            focus,
            styles,
        }
    }

    fn lines(&self) -> Vec<Line<'static>> {
        let diagnostics = self.diagnostics.all().collect::<Vec<_>>();
        if diagnostics.is_empty() {
            return vec![Line::raw("No diagnostics")];
        }
        diagnostics
            .into_iter()
            .map(|diagnostic| self.line(diagnostic))
            .collect()
    }

    fn line(&self, diagnostic: &Diagnostic) -> Line<'static> {
        let (label, style) = match diagnostic.severity {
            DiagnosticSeverity::Error => ("error", self.styles.error),
            DiagnosticSeverity::Warning => ("warning", self.styles.warning),
            DiagnosticSeverity::Information => ("info", self.styles.muted),
            DiagnosticSeverity::Hint => ("hint", self.styles.muted),
        };
        let path = self
            .root
            .and_then(|root| diagnostic.path.strip_prefix(root).ok())
            .unwrap_or(&diagnostic.path);
        Line::styled(
            format!(
                "{label} {}:{}:{}  {}",
                path.display(),
                diagnostic.start.line + 1,
                diagnostic.start.character + 1,
                diagnostic.message
            ),
            self.styles.base.patch(style),
        )
    }
}

impl Widget for DiagnosticsPanel<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let border = match self.focus {
            FocusState::Focused => self.styles.focused_border,
            FocusState::Unfocused => self.styles.border,
        };
        let block = Block::bordered()
            .border_type(match self.focus {
                FocusState::Focused => BorderType::Double,
                FocusState::Unfocused => BorderType::Plain,
            })
            .border_style(self.styles.base.patch(border))
            .style(self.styles.base)
            .title("Diagnostics");
        Paragraph::new(self.lines())
            .style(self.styles.base)
            .block(block)
            .render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use app::language::DiagnosticBatch;
    use editor::DocumentPosition;
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::theme::Theme;

    #[test]
    fn renders_severity_path_position_and_message() {
        let path = std::path::PathBuf::from("/workspace/main.rs");
        let diagnostics = vec![
            Diagnostic {
                path: path.clone(),
                start: DocumentPosition {
                    line: 0,
                    character: 1,
                },
                end: DocumentPosition {
                    line: 0,
                    character: 2,
                },
                severity: DiagnosticSeverity::Error,
                message: "bad value".to_string(),
                source: None,
            },
            Diagnostic {
                path,
                start: DocumentPosition {
                    line: 1,
                    character: 0,
                },
                end: DocumentPosition {
                    line: 1,
                    character: 1,
                },
                severity: DiagnosticSeverity::Warning,
                message: "unused value".to_string(),
                source: None,
            },
        ];
        let mut state = DiagnosticsState::default();
        state.replace(
            DiagnosticBatch {
                path: std::path::PathBuf::from("/workspace/main.rs"),
                version: None,
                diagnostics,
            },
            "rust-analyzer",
            0,
            None,
        );
        let theme = Theme::default();
        let backend = TestBackend::new(70, 6);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                frame.render_widget(
                    DiagnosticsPanel::new(
                        &state,
                        Some(std::path::Path::new("/workspace")),
                        FocusState::Unfocused,
                        theme.ui(),
                    ),
                    frame.area(),
                );
            })
            .unwrap();
        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("Diagnostics"));
        assert!(content.contains("error main.rs:1:2  bad value"));
        assert!(content.contains("warning main.rs:2:1  unused value"));
    }
}
