pub mod editor;
pub mod vcs;
pub mod vcs_commit;
pub mod vcs_history;

use ::editor::EditMode;
use ratatui::layout::Rect;

use crate::data_object::{AppPage, Model};

/// Apply the active pages' viewport geometry without feature policy in the message coordinator.
pub fn resize(model: &mut Model, width: u16, height: u16) {
    let area = editor::editor_content_area(
        Rect::new(0, 0, width, height),
        model.explorer.visibility,
        model.tools.visibility,
        model.editor.active_snapshot().as_ref(),
    );
    model.editor.resize_active_view(area.width, area.height);
    let area = vcs_commit::message_content_area(
        Rect::new(0, 0, width, height),
        model.vcs.commit.editor.active_snapshot().as_ref(),
    );
    model.vcs.commit.resize_active_view(area.width, area.height);
}

pub fn active_mode(model: &Model) -> EditMode {
    match model.active_page {
        AppPage::VcsCommit => model.vcs.commit.editor.mode(),
        _ => model.editor.mode(),
    }
}

#[cfg(test)]
mod tests {
    use ::editor::DocumentFactory;

    use super::*;

    #[test]
    fn active_mode_uses_the_composed_page_editor() {
        let mut model = Model::default();
        model
            .editor
            .insert_document(DocumentFactory::new().create("main"));
        model.editor.enter_insert_mode();
        model.active_page = AppPage::VcsCommit;
        assert_eq!(active_mode(&model), EditMode::Normal);

        model.active_page = AppPage::Editor;
        assert_eq!(active_mode(&model), EditMode::Insert);
    }
}
