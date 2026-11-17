//! Percentage-based viewport scrolling independent of application composition.

use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScrollDirection {
    Down,
    Up,
}

/// The only viewport operations required by [`PageScroll`].
pub(crate) trait ScrollViewport {
    fn viewport_height(&self) -> Option<usize>;
    fn scroll_lines(&mut self, direction: ScrollDirection, rows: usize);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageScrollError(String);

impl Display for PageScrollError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for PageScrollError {}

/// Reusable page-scroll policy configured as a percentage of the viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageScroll {
    step_percent: u8,
}

impl PageScroll {
    pub fn from_step(step: &str) -> Result<Self, PageScrollError> {
        let configured = step.trim();
        let Some(percent) = configured.strip_suffix('%') else {
            return Err(PageScrollError(
                "page-scroll step must be a percentage such as `75%`".to_string(),
            ));
        };
        let Ok(step_percent) = percent.parse::<u8>() else {
            return Err(PageScrollError(format!(
                "invalid page-scroll step `{configured}`; expected a percentage from `1%` to `100%`"
            )));
        };
        if !(1..=100).contains(&step_percent) {
            return Err(PageScrollError(format!(
                "page-scroll step `{configured}` is outside `1%`..=`100%`"
            )));
        }
        Ok(Self { step_percent })
    }

    /// Scroll a composed viewport. An unsized viewport is deliberately a no-op.
    pub(crate) fn scroll(&self, viewport: &mut impl ScrollViewport, direction: ScrollDirection) {
        let Some(height) = viewport.viewport_height() else {
            return;
        };
        viewport.scroll_lines(direction, self.rows(height));
    }

    fn rows(self, viewport_height: usize) -> usize {
        (viewport_height.saturating_mul(usize::from(self.step_percent)) / 100).max(1)
    }
}

impl Default for PageScroll {
    fn default() -> Self {
        Self { step_percent: 75 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Default)]
    struct FakeViewport {
        height: Option<usize>,
        calls: Vec<(ScrollDirection, usize)>,
    }

    impl ScrollViewport for FakeViewport {
        fn viewport_height(&self) -> Option<usize> {
            self.height
        }

        fn scroll_lines(&mut self, direction: ScrollDirection, rows: usize) {
            self.calls.push((direction, rows));
        }
    }

    #[test]
    fn validates_percentage_steps_and_computes_viewport_rows() {
        let scroll = PageScroll::from_step("25%").unwrap();
        assert_eq!(scroll.rows(20), 5);
        assert_eq!(scroll.rows(1), 1);

        for step in ["", "75", "abc%", "0%", "101%"] {
            assert!(PageScroll::from_step(step).is_err(), "{step}");
        }
    }

    #[test]
    fn scrolls_only_through_the_composed_viewport_interface() {
        let scroll = PageScroll::from_step("25%").unwrap();
        let mut viewport = FakeViewport {
            height: Some(20),
            calls: Vec::new(),
        };

        scroll.scroll(&mut viewport, ScrollDirection::Down);
        scroll.scroll(&mut viewport, ScrollDirection::Up);

        assert_eq!(
            viewport.calls,
            [(ScrollDirection::Down, 5), (ScrollDirection::Up, 5)]
        );
    }

    #[test]
    fn unsized_viewport_does_not_receive_a_scroll() {
        let mut viewport = FakeViewport::default();

        PageScroll::default().scroll(&mut viewport, ScrollDirection::Down);

        assert!(viewport.calls.is_empty());
    }
}
