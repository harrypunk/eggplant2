use ratatui::{
    style::Modifier,
    text::{Line, Span},
};

use ::vcs::CommitSummary;

use crate::{component::relative_time, theme::UiTheme};

/// Render one commit summary, varying only its prefix and selection state.
pub fn line(
    commit: &CommitSummary,
    prefix: String,
    selected: bool,
    styles: &UiTheme,
) -> Line<'static> {
    let subject_style = if selected {
        styles
            .base
            .patch(styles.key_hint)
            .add_modifier(Modifier::BOLD)
    } else {
        styles.base
    };
    let relative = relative_time::describe(commit.timestamp, relative_time::now());
    let branches =
        (!commit.branches.is_empty()).then(|| format!("[{}]", commit.branches.join(", ")));
    let mut spans = vec![
        Span::styled(prefix, styles.key_hint),
        Span::styled(format!("{}  ", commit.id), styles.key_hint),
        Span::styled(commit.subject.clone(), subject_style),
        Span::styled(
            format!("  {}  {relative}", commit.author),
            styles.base.patch(styles.muted),
        ),
    ];
    if let Some(branches) = branches {
        spans.push(Span::styled(
            format!("  {branches}"),
            styles.base.patch(styles.action_hint),
        ));
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;

    #[test]
    fn renders_a_complete_commit_message_with_a_custom_prefix() {
        let theme = Theme::default();
        let commit = CommitSummary {
            id: "abc1234".to_string(),
            subject: "subject".to_string(),
            author: "Eggplant".to_string(),
            timestamp: relative_time::now(),
            branches: vec!["main".to_string(), "origin/main".to_string()],
        };

        let line = line(&commit, "HEAD main  ".to_string(), false, theme.ui());

        assert_eq!(
            line.to_string(),
            "HEAD main  abc1234  subject  Eggplant  just now  [main, origin/main]"
        );
    }
}
