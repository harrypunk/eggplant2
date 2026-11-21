use std::{collections::VecDeque, time::Duration};

pub const DEFAULT_NOTIFICATION_DURATION: Duration = Duration::from_secs(4);
pub const MAX_NOTIFICATIONS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationKind {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    kind: NotificationKind,
    title: Option<String>,
    message: String,
    duration: Duration,
}

impl Notification {
    pub fn info(message: impl Into<String>) -> Self {
        Self::new(NotificationKind::Info, message)
    }

    pub fn warning(message: impl Into<String>) -> Self {
        Self::new(NotificationKind::Warning, message)
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self::new(NotificationKind::Error, message)
    }

    fn new(kind: NotificationKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            title: None,
            message: message.into(),
            duration: DEFAULT_NOTIFICATION_DURATION,
        }
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

    pub fn kind(&self) -> NotificationKind {
        self.kind
    }

    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn duration(&self) -> Duration {
        self.duration
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotificationExpiry {
    pub id: u64,
    pub after: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ActiveNotification {
    id: u64,
    notification: Notification,
}

/// Source-independent notification stack, ordered newest first.
#[derive(Debug, Default)]
pub struct NotificationCenter {
    active: VecDeque<ActiveNotification>,
    next_id: u64,
}

/// Minimal publishing port for features that do not need application state.
pub trait NotificationSink {
    fn notify(&mut self, notification: Notification) -> NotificationExpiry;
}

impl NotificationCenter {
    pub fn current(&self) -> Option<&Notification> {
        self.active.front().map(|active| &active.notification)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Notification> {
        self.active.iter().map(|active| &active.notification)
    }

    pub fn dismiss(&mut self, id: u64) -> bool {
        let Some(index) = self.active.iter().position(|active| active.id == id) else {
            return false;
        };
        self.active.remove(index);
        true
    }
}

impl NotificationSink for NotificationCenter {
    fn notify(&mut self, notification: Notification) -> NotificationExpiry {
        self.next_id = self.next_id.wrapping_add(1);
        let expiry = NotificationExpiry {
            id: self.next_id,
            after: notification.duration(),
        };
        self.active.push_front(ActiveNotification {
            id: expiry.id,
            notification,
        });
        self.active.truncate(MAX_NOTIFICATIONS);
        expiry
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_four_seconds_and_expires_each_message_independently() {
        let mut center = NotificationCenter::default();
        let first = center.notify(Notification::info("first"));
        assert_eq!(first.after, Duration::from_secs(4));
        assert_eq!(center.current().unwrap().message(), "first");

        let second = center.notify(
            Notification::error("second")
                .with_title("Git")
                .with_duration(Duration::from_millis(250)),
        );
        assert_eq!(second.after, Duration::from_millis(250));
        assert_eq!(center.iter().count(), 2);
        assert!(center.dismiss(first.id));
        assert_eq!(center.current().unwrap().message(), "second");
        assert_eq!(center.current().unwrap().title(), Some("Git"));
        assert!(center.dismiss(second.id));
        assert!(center.current().is_none());
    }

    #[test]
    fn retains_only_the_newest_eight_notifications() {
        let mut center = NotificationCenter::default();
        for index in 0..10 {
            center.notify(Notification::info(index.to_string()));
        }

        let messages = center
            .iter()
            .map(|notification| notification.message())
            .collect::<Vec<_>>();
        assert_eq!(messages, ["9", "8", "7", "6", "5", "4", "3", "2"]);
    }
}
