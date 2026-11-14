use ratatui::{
    Frame,
    layout::{Rect, Size},
    widgets::Paragraph,
};

use crate::{
    data_object::{CommandId, Message, Model, StatusMessage, Visibility},
    manager::{
        command_registry::{Command, CommandRegistry},
        effect::{Completion, Effect, FeatureCompletion, FeatureEffect},
    },
    ui::{OverlayContribution, OverlayKind, OverlayPlacement, UiRegistry},
};

/// Test-only typed feature state used by the M6 architecture proof.
#[derive(Debug)]
pub struct ExampleFeatureState {
    pending: u32,
    completed: u32,
    visibility: Visibility,
}

impl Default for ExampleFeatureState {
    fn default() -> Self {
        Self {
            pending: 0,
            completed: 0,
            visibility: Visibility::Hidden,
        }
    }
}

pub fn register(commands: &mut CommandRegistry, ui: &mut UiRegistry) -> color_eyre::Result<()> {
    commands.register(Command::feature(
        CommandId("example.increment"),
        "increment example",
        handle_command,
    ))?;
    ui.register_overlay("example", overlay)
}

fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    match id.as_str() {
        "example.increment" => {
            model.example_feature.pending += 1;
            model.example_feature.visibility = Visibility::Displayed;
            Some(Effect::feature(ExampleEffect))
        }
        _ => {
            model.status = Some(StatusMessage::error(format!(
                "unknown example command `{}`",
                id.as_str()
            )));
            None
        }
    }
}

struct ExampleCompletion;

impl Completion for ExampleCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        model.example_feature.completed += 1;
        model.example_feature.visibility = Visibility::Hidden;
        None
    }
}

struct ExampleEffect;

impl FeatureEffect for ExampleEffect {
    fn perform(self: Box<Self>) -> Message {
        Message::FeatureCompletion(FeatureCompletion::new(ExampleCompletion))
    }
}

fn overlay(model: &Model) -> Option<OverlayContribution> {
    match model.example_feature.visibility {
        Visibility::Displayed => Some(OverlayContribution::new(
            "example",
            OverlayKind::Modal,
            OverlayPlacement::Centered(Size::new(24, 4)),
            render_example,
        )),
        Visibility::Hidden => None,
    }
}

fn render_example(model: &Model, frame: &mut Frame, area: Rect) {
    let text = format!("Example: {}", model.example_feature.pending);
    frame.render_widget(Paragraph::new(text), area);
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::{manager::message_manager::MessageManager, page::editor::Editor};

    #[test]
    fn feature_works_through_command_effect_and_ui_registration_only() {
        let mut commands = CommandRegistry::builtin();
        let mut ui = UiRegistry::builtin();
        register(&mut commands, &mut ui).unwrap();
        let manager = MessageManager::new(
            commands,
            crate::manager::text_input_registry::TextInputRegistry::builtin(),
        );
        let mut model = Model::default();

        let effect = manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("example.increment")),
        );
        let Some(Effect::Feature(effect)) = effect else {
            panic!("expected the example effect");
        };
        assert_eq!(model.example_feature.pending, 1);
        assert!(
            ui.overlays(&model)
                .iter()
                .any(|overlay| overlay.id == "example")
        );

        let page = Editor::new(ui.clone());
        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| page.view(&model, frame)).unwrap();
        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("Example: 1"));

        let completion = effect.perform();
        manager.update(&mut model, completion);

        assert_eq!(model.example_feature.completed, 1);
        assert_eq!(model.example_feature.visibility, Visibility::Hidden);
        assert!(ui.overlays(&model).is_empty());
    }
}
