use eggplant_agent::SessionId;
use eggplant_agent_storage::{RecentPage, SessionCursor, SessionRecord};

use crate::{component::list::ListState, data_object::Context};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionSelectorPage {
    Closed,
    Loading,
    LoadingMore,
    Ready,
    Restoring,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionSelectorNotice {
    Info(String),
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSelectorState {
    page: SessionSelectorPage,
    sessions: ListState<SessionRecord>,
    next: Option<SessionCursor>,
    active_session: Option<SessionId>,
    return_context: Option<Context>,
    generation: u64,
    notice: Option<SessionSelectorNotice>,
}

impl Default for SessionSelectorState {
    fn default() -> Self {
        Self {
            page: SessionSelectorPage::Closed,
            sessions: ListState::default(),
            next: None,
            active_session: None,
            return_context: None,
            generation: 0,
            notice: None,
        }
    }
}

impl SessionSelectorState {
    pub(crate) fn open(
        &mut self,
        return_context: Option<Context>,
        active_session: SessionId,
    ) -> u64 {
        self.page = SessionSelectorPage::Loading;
        self.sessions = ListState::default();
        self.next = None;
        self.active_session = Some(active_session);
        self.return_context = return_context;
        self.notice = None;
        self.generation = self.generation.wrapping_add(1);
        self.generation
    }

    pub(super) fn close(&mut self) -> Option<Context> {
        self.page = SessionSelectorPage::Closed;
        self.sessions = ListState::default();
        self.next = None;
        self.active_session = None;
        self.notice = None;
        self.generation = self.generation.wrapping_add(1);
        self.return_context.take()
    }

    pub(crate) fn complete_recent(
        &mut self,
        generation: u64,
        result: Result<RecentPage, String>,
    ) -> bool {
        if self.page != SessionSelectorPage::Loading || generation != self.generation {
            return false;
        }
        self.page = SessionSelectorPage::Ready;
        match result {
            Ok(page) => {
                let selected = self.active_session.clone();
                self.notice = page.sessions.is_empty().then(|| {
                    SessionSelectorNotice::Info("No saved sessions in this workspace".to_string())
                });
                self.next = page.next;
                self.replace_sessions(page.sessions, selected);
            }
            Err(error) => {
                self.sessions = ListState::default();
                self.next = None;
                self.notice = Some(SessionSelectorNotice::Error(error));
            }
        }
        true
    }

    pub(super) fn begin_next_page(&mut self) -> Option<(SessionCursor, u64)> {
        if self.page != SessionSelectorPage::Ready {
            return None;
        }
        let cursor = self.next.clone()?;
        self.page = SessionSelectorPage::LoadingMore;
        self.notice = None;
        Some((cursor, self.generation))
    }

    pub(super) fn complete_next(
        &mut self,
        generation: u64,
        result: Result<RecentPage, String>,
    ) -> bool {
        if self.page != SessionSelectorPage::LoadingMore || generation != self.generation {
            return false;
        }
        self.page = SessionSelectorPage::Ready;
        match result {
            Ok(page) => {
                let selected = self.selected_session().map(|session| session.id.clone());
                let sessions = self
                    .sessions
                    .items()
                    .iter()
                    .cloned()
                    .chain(page.sessions)
                    .collect();
                self.next = page.next;
                self.notice = None;
                self.replace_sessions(sessions, selected);
            }
            Err(error) => self.notice = Some(SessionSelectorNotice::Error(error)),
        }
        true
    }

    pub(super) fn begin_restore(&mut self) -> Option<(SessionId, u64)> {
        if self.page != SessionSelectorPage::Ready {
            return None;
        }
        let session_id = self.sessions.selected_item()?.id.clone();
        self.page = SessionSelectorPage::Restoring;
        self.notice = None;
        Some((session_id, self.generation))
    }

    pub(super) fn fail_restore(&mut self, generation: u64, error: String) -> bool {
        if !self.is_current_restoring(generation) {
            return false;
        }
        self.page = SessionSelectorPage::Ready;
        self.notice = Some(SessionSelectorNotice::Error(error));
        true
    }

    pub(super) fn is_current_restoring(&self, generation: u64) -> bool {
        self.page == SessionSelectorPage::Restoring && self.generation == generation
    }

    pub(super) fn select_next(&mut self) {
        if self.page == SessionSelectorPage::Ready {
            self.sessions.select_next();
        }
    }

    pub(super) fn select_previous(&mut self) {
        if self.page == SessionSelectorPage::Ready {
            self.sessions.select_previous();
        }
    }

    fn selected_session(&self) -> Option<&SessionRecord> {
        self.sessions.selected_item()
    }

    fn replace_sessions(&mut self, sessions: Vec<SessionRecord>, selected: Option<SessionId>) {
        let selected = selected
            .and_then(|selected| sessions.iter().position(|session| session.id == selected))
            .or(Some(0));
        self.sessions = ListState::new(sessions);
        self.sessions.select(selected);
    }

    pub fn page(&self) -> SessionSelectorPage {
        self.page
    }

    pub fn sessions(&self) -> &[SessionRecord] {
        self.sessions.items()
    }

    pub fn has_next(&self) -> bool {
        self.next.is_some()
    }

    pub fn selected(&self) -> Option<usize> {
        self.sessions.selected()
    }

    pub fn active_session(&self) -> Option<&SessionId> {
        self.active_session.as_ref()
    }

    pub fn notice(&self) -> Option<&SessionSelectorNotice> {
        self.notice.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(id: &str) -> SessionRecord {
        SessionRecord {
            id: SessionId::from(id),
            created_at: 1,
            updated_at: 2,
            workspace: Some("/workspace".to_string()),
            title: Some(format!("Title {id}")),
        }
    }

    fn page(sessions: Vec<SessionRecord>) -> RecentPage {
        RecentPage {
            sessions,
            next: None,
        }
    }

    #[test]
    fn recent_completion_selects_active_session_and_restore_uses_identity() {
        let mut state = SessionSelectorState::default();
        let generation = state.open(None, SessionId::from("session-02"));
        assert!(state.complete_recent(
            generation,
            Ok(page(vec![record("session-01"), record("session-02")]))
        ));
        assert_eq!(state.selected(), Some(1));

        state.select_next();
        let (selected, generation) = state.begin_restore().unwrap();
        assert_eq!(selected, SessionId::from("session-01"));
        assert_eq!(state.page(), SessionSelectorPage::Restoring);

        assert!(state.fail_restore(generation, "load failed".to_string()));
        assert_eq!(state.page(), SessionSelectorPage::Ready);
        assert_eq!(
            state.notice(),
            Some(&SessionSelectorNotice::Error("load failed".to_string()))
        );
    }

    #[test]
    fn next_page_appends_and_preserves_selection_by_session_id() {
        let mut state = SessionSelectorState::default();
        let generation = state.open(None, SessionId::from("session-02"));
        let cursor = SessionCursor {
            updated_at: 2,
            id: SessionId::from("session-02"),
        };
        assert!(state.complete_recent(
            generation,
            Ok(RecentPage {
                sessions: vec![record("session-01"), record("session-02")],
                next: Some(cursor.clone()),
            })
        ));
        assert_eq!(state.selected(), Some(1));

        assert_eq!(state.begin_next_page(), Some((cursor, generation)));
        assert_eq!(state.page(), SessionSelectorPage::LoadingMore);
        assert!(state.complete_next(
            generation,
            Ok(RecentPage {
                sessions: vec![record("session-03")],
                next: None,
            })
        ));

        assert_eq!(state.page(), SessionSelectorPage::Ready);
        assert_eq!(state.sessions().len(), 3);
        assert_eq!(state.selected(), Some(1));
        assert_eq!(
            state.selected_session().map(|session| &session.id),
            Some(&SessionId::from("session-02"))
        );
        assert!(!state.has_next());
        assert!(!state.complete_next(generation, Ok(page(vec![record("stale")]))));
    }

    #[test]
    fn stale_completions_cannot_change_a_closed_or_newer_selector() {
        let mut state = SessionSelectorState::default();
        let generation = state.open(None, SessionId::from("current"));
        state.close();
        assert!(!state.complete_recent(generation, Ok(page(vec![record("old")]))));
        assert_eq!(state.page(), SessionSelectorPage::Closed);

        let generation = state.open(None, SessionId::from("current"));
        assert!(state.complete_recent(generation, Ok(page(vec![record("stored")]))));
        let (_, restore_generation) = state.begin_restore().unwrap();
        state.close();
        assert!(!state.fail_restore(restore_generation, "stale".to_string()));
        assert_eq!(state.page(), SessionSelectorPage::Closed);
    }
}
