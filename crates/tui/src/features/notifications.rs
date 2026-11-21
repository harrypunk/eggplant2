use ratatui::Frame;

use crate::{
    capabilities::notification::{Notification, NotificationExpiry, NotificationSink},
    component::notification::NotificationStack,
    data_object::{Message, Model, StatusKind, StatusMessage},
    manager::effect::{Completion, Effect, FeatureCompletion, FeatureEffect},
};

pub struct NotificationCompletion(pub Notification);

impl Completion for NotificationCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        Some(publish(model, self.0))
    }
}

/// Publish any source's notification and schedule its non-blocking expiry.
pub fn publish(model: &mut Model, notification: Notification) -> Effect {
    let expiry = model.notifications.notify(notification);
    Effect::feature(ExpiryRequest(expiry))
}

/// Adapt shared status values without making the notification core depend on them.
pub fn publish_status(model: &mut Model, status: StatusMessage, title: Option<&str>) -> Effect {
    let mut notification = match status.kind {
        StatusKind::Info => Notification::info(status.text),
        StatusKind::Error => Notification::error(status.text),
    };
    if let Some(title) = title {
        notification = notification.with_title(title);
    }
    publish(model, notification)
}

/// Preserve the persistent status bar while adding transient feedback.
pub fn report_status(model: &mut Model, status: StatusMessage) -> Effect {
    report_status_titled(model, status, None)
}

pub fn report_status_titled(
    model: &mut Model,
    status: StatusMessage,
    title: Option<&str>,
) -> Effect {
    model.status = Some(status.clone());
    publish_status(model, status, title)
}

/// Preserve status while marking a one-shot rejection explicitly.
pub fn report_rejection(model: &mut Model, message: impl Into<String>) -> Effect {
    let status = StatusMessage::info(message);
    model.status = Some(status.clone());
    publish_status(model, status, Some("Rejected"))
}

/// Render the stack above the active page without changing focus, context, or modality.
pub fn render(model: &Model, frame: &mut Frame) {
    NotificationStack::new(model.notifications.iter(), &model.theme).render(frame, frame.area());
}

struct ExpiryRequest(NotificationExpiry);

impl FeatureEffect for ExpiryRequest {
    fn perform(self: Box<Self>) -> Message {
        Message::FeatureCompletion(FeatureCompletion::new(ExpiryCompletion(self.0.id)))
    }

    fn execute(self: Box<Self>, sender: tokio::sync::mpsc::UnboundedSender<Message>) {
        tokio::spawn(async move {
            tokio::time::sleep(self.0.after).await;
            let _ = sender.send(self.perform());
        });
    }
}

struct ExpiryCompletion(u64);

impl Completion for ExpiryCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        model.notifications.dismiss(self.0);
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(effect: Effect) -> Box<dyn FeatureEffect> {
        let Effect::Feature(request) = effect else {
            panic!("expected notification expiry effect");
        };
        request
    }

    #[test]
    fn older_expiry_leaves_the_newest_notification_visible() {
        let mut model = Model::default();
        let first = request(publish(&mut model, Notification::info("first")));
        let _second = request(publish(&mut model, Notification::info("second")));

        let Message::FeatureCompletion(completion) = first.perform() else {
            panic!("expected expiry completion");
        };
        completion.update(&mut model);
        assert_eq!(model.notifications.current().unwrap().message(), "second");
        assert_eq!(model.notifications.iter().count(), 1);
    }

    #[test]
    fn newest_expiry_reveals_older_notifications() {
        let mut model = Model::default();
        let _first = request(publish(&mut model, Notification::info("first")));
        let second = request(publish(&mut model, Notification::info("second")));

        let Message::FeatureCompletion(completion) = second.perform() else {
            panic!("expected expiry completion");
        };
        completion.update(&mut model);
        assert_eq!(model.notifications.current().unwrap().message(), "first");
    }

    #[test]
    fn notification_completion_publishes_lsp_status() {
        let mut model = Model::default();
        let effect = Box::new(NotificationCompletion(
            Notification::info("Language server `rust-analyzer` started").with_title("LSP"),
        ))
        .update(&mut model);

        assert!(effect.is_some());
        let notification = model.notifications.current().unwrap();
        assert_eq!(notification.title(), Some("LSP"));
        assert_eq!(
            notification.message(),
            "Language server `rust-analyzer` started"
        );
    }

    #[test]
    fn current_expiry_clears_the_notification() {
        let mut model = Model::default();
        let effect = request(publish(&mut model, Notification::info("temporary")));
        let Message::FeatureCompletion(completion) = effect.perform() else {
            panic!("expected expiry completion");
        };
        completion.update(&mut model);
        assert!(model.notifications.current().is_none());
    }
}
