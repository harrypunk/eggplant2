use crate::{
    agent::AgentInputMode,
    capabilities::page_scroll::{PageScroll, ScrollDirection},
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ViewportMetrics {
    pub content_rows: usize,
    pub viewport_rows: usize,
}

impl ViewportMetrics {
    pub fn max_offset(self) -> usize {
        self.content_rows.saturating_sub(self.viewport_rows)
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PageScrollState {
    position: ScrollPosition,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum ScrollPosition {
    #[default]
    Bottom,
    Offset(usize),
}

impl PageScrollState {
    pub fn offset(self, metrics: ViewportMetrics) -> usize {
        match self.position {
            ScrollPosition::Bottom => metrics.max_offset(),
            ScrollPosition::Offset(offset) => offset.min(metrics.max_offset()),
        }
    }

    pub(crate) fn scroll(
        &mut self,
        page: PageScroll,
        direction: ScrollDirection,
        metrics: ViewportMetrics,
    ) {
        if metrics.viewport_rows == 0 {
            return;
        }
        let current = self.offset(metrics);
        let rows = page.rows(metrics.viewport_rows);
        let next = match direction {
            ScrollDirection::Up => current.saturating_sub(rows),
            ScrollDirection::Down => current.saturating_add(rows).min(metrics.max_offset()),
        };
        self.position = if next == metrics.max_offset() {
            ScrollPosition::Bottom
        } else {
            ScrollPosition::Offset(next)
        };
    }

    pub fn bottom(&mut self) {
        self.position = ScrollPosition::Bottom;
    }

    fn reveal(&mut self, row: usize, metrics: ViewportMetrics) {
        if metrics.viewport_rows == 0 {
            return;
        }
        let current = self.offset(metrics);
        let viewport_end = current.saturating_add(metrics.viewport_rows);
        let next = if row < current {
            row
        } else if row >= viewport_end {
            row.saturating_add(1).saturating_sub(metrics.viewport_rows)
        } else {
            current
        };
        self.position = if next == metrics.max_offset() {
            ScrollPosition::Bottom
        } else {
            ScrollPosition::Offset(next.min(metrics.max_offset()))
        };
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PanelNavigation {
    scroll: PageScrollState,
    cursor: TranscriptCursor,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum TranscriptCursor {
    #[default]
    Bottom,
    Row(usize),
}

impl PanelNavigation {
    pub fn offset(self, metrics: ViewportMetrics) -> usize {
        self.scroll.offset(metrics)
    }

    pub fn viewport(
        mut self,
        metrics: ViewportMetrics,
        mode: AgentInputMode,
    ) -> (usize, Option<usize>) {
        let cursor = match mode {
            AgentInputMode::Normal => self.cursor_row(metrics),
            AgentInputMode::Insert => None,
        };
        if let Some(row) = cursor {
            self.scroll.reveal(row, metrics);
        }
        (self.offset(metrics), cursor)
    }

    pub fn cursor_row(self, metrics: ViewportMetrics) -> Option<usize> {
        if metrics.content_rows == 0 {
            return None;
        }
        let last = metrics.content_rows - 1;
        Some(match self.cursor {
            TranscriptCursor::Bottom => last,
            TranscriptCursor::Row(row) => row.min(last),
        })
    }

    pub fn enter_normal(&mut self) {
        self.scroll.bottom();
        self.cursor = TranscriptCursor::Bottom;
    }

    pub(crate) fn move_cursor(&mut self, direction: ScrollDirection, metrics: ViewportMetrics) {
        self.move_cursor_rows(direction, 1, metrics);
    }

    pub(crate) fn page(
        &mut self,
        page: PageScroll,
        direction: ScrollDirection,
        metrics: ViewportMetrics,
    ) {
        if metrics.viewport_rows == 0 {
            return;
        }
        self.scroll.scroll(page, direction, metrics);
        self.move_cursor_rows(direction, page.rows(metrics.viewport_rows), metrics);
    }

    pub(crate) fn scroll(
        &mut self,
        page: PageScroll,
        direction: ScrollDirection,
        metrics: ViewportMetrics,
    ) {
        self.scroll.scroll(page, direction, metrics);
    }

    fn move_cursor_rows(
        &mut self,
        direction: ScrollDirection,
        rows: usize,
        metrics: ViewportMetrics,
    ) {
        let Some(current) = self.cursor_row(metrics) else {
            return;
        };
        let last = metrics.content_rows - 1;
        let next = match direction {
            ScrollDirection::Up => current.saturating_sub(rows),
            ScrollDirection::Down => current.saturating_add(rows).min(last),
        };
        self.cursor = if next == last {
            TranscriptCursor::Bottom
        } else {
            TranscriptCursor::Row(next)
        };
        self.scroll.reveal(next, metrics);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceNavigation {
    Dialog(PageScrollState),
    Panel(PanelNavigation),
}

impl Default for SurfaceNavigation {
    fn default() -> Self {
        Self::Dialog(PageScrollState::default())
    }
}

impl SurfaceNavigation {
    pub fn dialog(self) -> PageScrollState {
        match self {
            Self::Dialog(scroll) => scroll,
            Self::Panel(_) => PageScrollState::default(),
        }
    }

    pub fn panel(self) -> PanelNavigation {
        match self {
            Self::Panel(navigation) => navigation,
            Self::Dialog(_) => PanelNavigation::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const METRICS: ViewportMetrics = ViewportMetrics {
        content_rows: 100,
        viewport_rows: 20,
    };

    #[test]
    fn page_scroll_follows_bottom_until_manual_navigation() {
        let page = PageScroll::from_step("50%").unwrap();
        let mut scroll = PageScrollState::default();
        assert_eq!(scroll.offset(METRICS), 80);

        scroll.scroll(page, ScrollDirection::Up, METRICS);
        assert_eq!(scroll.offset(METRICS), 70);
        scroll.scroll(page, ScrollDirection::Up, METRICS);
        assert_eq!(scroll.offset(METRICS), 60);
        scroll.scroll(page, ScrollDirection::Down, METRICS);
        scroll.scroll(page, ScrollDirection::Down, METRICS);
        assert_eq!(scroll.offset(METRICS), 80);

        scroll.scroll(page, ScrollDirection::Up, METRICS);
        let resized = ViewportMetrics {
            viewport_rows: 50,
            ..METRICS
        };
        assert_eq!(scroll.offset(resized), 50);
    }

    #[test]
    fn panel_cursor_starts_at_bottom_and_reveals_navigation() {
        let page = PageScroll::default();
        let mut navigation = PanelNavigation::default();
        assert_eq!(navigation.cursor_row(METRICS), Some(99));
        assert_eq!(navigation.offset(METRICS), 80);

        navigation.move_cursor(ScrollDirection::Up, METRICS);
        assert_eq!(navigation.cursor_row(METRICS), Some(98));
        navigation.page(page, ScrollDirection::Up, METRICS);
        let cursor = navigation.cursor_row(METRICS).unwrap();
        assert_eq!(cursor, 83);
        assert_eq!(navigation.offset(METRICS), 65);

        navigation.move_cursor(ScrollDirection::Down, METRICS);
        navigation.move_cursor(ScrollDirection::Down, METRICS);
        assert_eq!(navigation.cursor_row(METRICS), Some(85));
        navigation.enter_normal();
        assert_eq!(navigation.cursor_row(METRICS), Some(99));
    }

    #[test]
    fn empty_content_and_unmeasured_viewports_are_safe() {
        let empty = ViewportMetrics::default();
        let mut navigation = PanelNavigation::default();
        navigation.move_cursor(ScrollDirection::Up, empty);
        navigation.page(PageScroll::default(), ScrollDirection::Down, empty);
        assert_eq!(navigation.cursor_row(empty), None);
        assert_eq!(navigation.offset(empty), 0);

        let unmeasured = ViewportMetrics {
            content_rows: 100,
            viewport_rows: 0,
        };
        navigation.page(PageScroll::default(), ScrollDirection::Up, unmeasured);
        assert_eq!(navigation.cursor_row(unmeasured), Some(99));
    }
}
