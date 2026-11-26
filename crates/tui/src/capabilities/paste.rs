use std::fmt;

use crate::capabilities::clipboard::{ClipboardError, ClipboardReader};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasteError {
    Clipboard(ClipboardError),
    EmptyClipboard,
    Multiline,
}

impl fmt::Display for PasteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Clipboard(error) => error.fmt(formatter),
            Self::EmptyClipboard => formatter.write_str("Clipboard is empty"),
            Self::Multiline => formatter.write_str("Pasted text must be single-line"),
        }
    }
}

impl std::error::Error for PasteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Clipboard(error) => Some(error),
            Self::EmptyClipboard | Self::Multiline => None,
        }
    }
}

impl From<ClipboardError> for PasteError {
    fn from(error: ClipboardError) -> Self {
        Self::Clipboard(error)
    }
}

/// Component-local paste policy with no application-model or effect dependency.
pub trait PasteTarget {
    type Revision: Send + 'static;
    type Error;

    fn begin_paste(&mut self) -> Option<Self::Revision>;

    fn complete_paste(
        &mut self,
        revision: Self::Revision,
        result: Result<String, ClipboardError>,
    ) -> Option<Result<(), Self::Error>>;
}

/// Capture a target revision and an owned clipboard reader for later completion.
pub fn paste_with_reader<T, C>(target: &mut T, clipboard: C) -> Option<PasteRequest<C, T::Revision>>
where
    T: PasteTarget,
{
    let revision = target.begin_paste()?;
    Some(PasteRequest {
        clipboard,
        revision,
    })
}

pub struct PasteRequest<C, R> {
    clipboard: C,
    revision: R,
}

impl<C: ClipboardReader, R> PasteRequest<C, R> {
    pub fn perform(self) -> PasteCompletion<R> {
        PasteCompletion {
            revision: self.revision,
            result: self.clipboard.read_text(),
        }
    }
}

/// Plain owned completion data; feature adapters decide how to route it into effects.
pub struct PasteCompletion<R> {
    revision: R,
    result: Result<String, ClipboardError>,
}

impl<R> PasteCompletion<R> {
    pub fn apply<T>(self, target: &mut T) -> Option<Result<(), T::Error>>
    where
        T: PasteTarget<Revision = R>,
    {
        target.complete_paste(self.revision, self.result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestClipboard(Result<String, ClipboardError>);

    impl ClipboardReader for TestClipboard {
        fn read_text(&self) -> Result<String, ClipboardError> {
            self.0.clone()
        }
    }

    type Completed = Vec<(u64, Result<String, ClipboardError>)>;

    struct FakeTarget {
        revision: u64,
        completed: Completed,
    }

    impl PasteTarget for FakeTarget {
        type Revision = u64;
        type Error = PasteError;

        fn begin_paste(&mut self) -> Option<Self::Revision> {
            self.revision = 7;
            Some(self.revision)
        }

        fn complete_paste(
            &mut self,
            revision: Self::Revision,
            result: Result<String, ClipboardError>,
        ) -> Option<Result<(), Self::Error>> {
            if revision != self.revision {
                return None;
            }
            self.completed.push((revision, result));
            Some(Ok(()))
        }
    }

    #[test]
    fn owned_request_completes_through_a_component_local_target() {
        let mut target = FakeTarget {
            revision: 0,
            completed: Vec::new(),
        };
        let request =
            paste_with_reader(&mut target, TestClipboard(Ok("pasted".to_string()))).unwrap();

        let outcome = request.perform().apply(&mut target);

        assert_eq!(outcome, Some(Ok(())));
        assert_eq!(target.completed, [(7, Ok("pasted".to_string()))]);
    }
}
