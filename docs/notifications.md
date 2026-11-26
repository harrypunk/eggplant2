# Transient notifications

Transient notifications provide common one-shot feedback without changing focus, input context, or modal ownership.

## Behavior

- Notifications appear in a non-modal stack at the top-right corner.
- The newest notification is on top and pushes older notifications downward.
- At most **8** notifications are retained; publishing a ninth evicts the oldest.
- The default duration is **4 seconds** per notification.
- Info, warning, and error messages have distinct theme styling.
- A source can provide an optional title such as `Yank`, `Rejected`, or `Git`.
- Every expiry removes only its matching notification; dismissing the newest reveals older entries.
- The stack uses vertical flex layout with one-row gaps and omits entries that cannot fit the terminal.
- Persistent status and modal-local errors remain available where needed.
- Migrated one-shot VCS feedback clears stale workflow notices; saving/committing progress remains in the page.

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
5. Dismiss the entry with that ID, leaving every other notification in place.

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
- LSP navigation-start feedback and response timeouts.

Progress notices and errors owned by an open modal remain in that workflow. Additional sources should use the same publisher rather than adding source-specific variants to the notification center.

## Presentation

`component/notification.rs` receives borrowed notifications and a resolved theme. It owns wrapping, vertical `Layout`/`Flex::Start` stacking, and bounded placement; it does not read application state or schedule work.

Each popup is limited to 48 columns and 8 rows, including borders. The stack has one-row gaps, retains at most eight entries in the core, and renders only boxes that fit the available terminal height. Runtime renders it after the active page so page implementations do not need individual notification arms.

## Tests

The suite covers:

- Four-second defaults and per-message duration overrides.
- Newest-first ordering and eight-entry retention.
- Independent older/newest expiry behavior.
- Vertical stack placement, terminal-height clipping, and tiny-terminal clamping.
- Yank, rejection, and Git source titles/messages.
- Composition with existing register, refresh, and repository effects.
