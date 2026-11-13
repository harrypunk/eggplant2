use color_eyre::eyre::bail;
use editor::VerticalDirection;

use crate::{
    config::PageScrollConfig,
    data_object::{CommandId, Model, StatusMessage},
    manager::effect::Effect,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageScrollState {
    step_percent: u8,
}

impl PageScrollState {
    pub fn from_config(config: &PageScrollConfig) -> color_eyre::Result<Self> {
        let configured = config.step.trim();
        let Some(percent) = configured.strip_suffix('%') else {
            bail!("page-scroll step must be a percentage such as `75%`");
        };
        let Ok(step_percent) = percent.parse::<u8>() else {
            bail!(
                "invalid page-scroll step `{configured}`; expected a percentage from `1%` to `100%`"
            );
        };
        if !(1..=100).contains(&step_percent) {
            bail!("page-scroll step `{configured}` is outside `1%`..=`100%`");
        }
        Ok(Self { step_percent })
    }

    fn rows(self, viewport_height: usize) -> usize {
        (viewport_height.saturating_mul(usize::from(self.step_percent)) / 100).max(1)
    }
}

impl Default for PageScrollState {
    fn default() -> Self {
        Self { step_percent: 75 }
    }
}

pub fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    let direction = match id.as_str() {
        "page_scroll_down" => VerticalDirection::Down,
        "page_scroll_up" => VerticalDirection::Up,
        _ => {
            model.status = Some(StatusMessage::error(format!(
                "unknown page-scroll command `{}`",
                id.as_str()
            )));
            return None;
        }
    };
    let viewport_height = model.editor.active_view_height()?;
    let rows = model.page_scroll.rows(viewport_height);
    let _ = model.editor.move_cursor_lines(direction, rows);
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        data_object::Message, manager::message_manager::MessageManager, test_utils::create_buffer,
    };

    #[test]
    fn validates_percentage_steps_and_computes_viewport_rows() {
        let config = PageScrollConfig {
            step: "25%".to_string(),
        };
        let state = PageScrollState::from_config(&config).unwrap();
        assert_eq!(state.rows(20), 5);
        assert_eq!(state.rows(1), 1);

        for step in ["", "75", "abc%", "0%", "101%"] {
            let config = PageScrollConfig {
                step: step.to_string(),
            };
            assert!(PageScrollState::from_config(&config).is_err(), "{step}");
        }
    }

    #[test]
    fn configured_step_scrolls_down_and_up_without_an_effect() {
        let mut model = Model::default();
        let text = (0..100)
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        create_buffer(&mut model, &text);
        model.page_scroll = PageScrollState::from_config(&PageScrollConfig {
            step: "25%".to_string(),
        })
        .unwrap();
        let manager = MessageManager::default();
        manager.update(&mut model, Message::Resize(80, 12));
        let height = model.editor.active_view_height().unwrap();
        let expected = model.page_scroll.rows(height);

        assert!(
            manager
                .update(
                    &mut model,
                    Message::InvokeCommand(CommandId("page_scroll_down"))
                )
                .is_none()
        );
        let snapshot = model.editor.active_snapshot().unwrap();
        assert_eq!(
            snapshot.text.first_line + snapshot.cursor.unwrap().row,
            expected
        );

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("page_scroll_up")),
        );
        let snapshot = model.editor.active_snapshot().unwrap();
        assert_eq!(snapshot.text.first_line + snapshot.cursor.unwrap().row, 0);
    }

    #[test]
    fn empty_and_unsized_documents_are_safe() {
        let mut model = Model::default();
        create_buffer(&mut model, "");
        let manager = MessageManager::default();
        manager.update(&mut model, Message::Resize(0, 0));

        assert!(
            manager
                .update(
                    &mut model,
                    Message::InvokeCommand(CommandId("page_scroll_down"))
                )
                .is_none()
        );
        assert_eq!(model.editor.active_snapshot().unwrap().text.line_count, 1);
    }
}
