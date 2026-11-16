use ratatui::style::Style;

use crate::theme::UiTheme;

pub fn line_style(styles: &UiTheme, line: &str) -> Style {
    match line.as_bytes().first() {
        Some(b'+') => styles.base.patch(styles.diff_add),
        Some(b'-') => styles.base.patch(styles.diff_remove),
        Some(b'@') if line.starts_with("@@") => styles.base.patch(styles.diff_hunk),
        _ => styles.base,
    }
}
