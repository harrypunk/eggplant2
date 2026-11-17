# Transient notifications

Transient notifications provide common one-shot feedback without changing focus, input context, or modal ownership.

## Behavior

- Notifications appear in a non-modal popup at the top-right corner.
- The default duration is **4 seconds**.
- Info and error messages have distinct theme styling.
- A source can provide an optional title such as `Yank`, `Rejected`, or `Git`.
- A new notification replaces the current one.
- An older expiry cannot dismiss a newer notification.
- The popup renders above every application page and clamps to small terminals.
- Persistent status, VCS notices, and modal-local errors remain available where needed.

## Source-independent core

`capabilities/notification.rs` contains no yank, VCS, application-model, message, effect, timer, or rendering types.

The publishing interface is deliberately small:

```rust
pub trait NotificationSink {
    fn notify(&mut self, notification: Notification) -> NotificationExpiry;
}
```

A source that owns a sink can publish without constructing `Model`:

```rust
let expiry = sink.notify(
    Notification::info("Yanked text")
        .with_title("Yank"),
);
```

`Notification::with_duration` overrides `DEFAULT_NOTIFICATION_DURATION` for a specific message.

## Application composition

`features/notifications.rs` adapts the core to application effects:

1. Publish to `Model.notifications`.
2. Capture the returned notification ID and duration.
3. Sleep asynchronously without blocking terminal input.
4. Return a typed expiry completion.
5. Dismiss only if that ID is still current.

Sources must compose the returned effect:

```rust
let notification = notifications::publish(
    model,
    Notification::info("Created commit abc1234").with_title("Git"),
);
Effect::combine(work, Some(notification))
```

**Do not publish and discard the returned effect.** Without the expiry effect, the notification remains visible indefinitely.

`publish_status` adapts a `StatusMessage` without making the core depend on application status types. `report_status` additionally preserves the editor status bar, while `report_rejection` adds a `Rejected` title.

## Connected sources

Current integrations include:

- Explicit main-editor and commit-message yanks.
- Unknown command errors and commit/lifecycle rejections.
- System clipboard write failures.
- VCS placeholders and in-flight index rejections.
- Stage/unstage completion feedback.
- Commit validation, draft close/discard, and final completion feedback.
- Merge clean-status rejection and successful merge feedback.
- Successful branch switching/creation.
- Commit-hash copy success and failure.

Progress notices and errors owned by an open modal remain in that workflow. Additional sources should use the same publisher rather than adding source-specific variants to the notification center.

## Presentation

`component/notification.rs` receives only a `Notification` and resolved theme. It owns wrapping and bounded placement; it does not read application state or schedule work.

The popup is limited to 48 columns and 8 rows, including borders. Runtime renders it after the active page so page implementations do not need individual notification arms.

## Tests

The suite covers:

- Four-second defaults and per-message duration overrides.
- Replacement and stale-expiry rejection.
- Current-expiry dismissal.
- Top-right placement and tiny-terminal clamping.
- Yank, rejection, and Git source titles/messages.
- Composition with existing register, refresh, and repository effects.
