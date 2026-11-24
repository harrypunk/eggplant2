mod effects;
mod model;

pub use effects::initial_effects;
pub use model::{initial_model, initial_model_with_languages};

use std::{ffi::OsString, path::PathBuf};

use color_eyre::eyre::bail;

/// Parse the optional file or folder argument without adding a CLI framework.
pub fn startup_path_from(
    args: impl IntoIterator<Item = OsString>,
) -> color_eyre::Result<Option<PathBuf>> {
    let mut args = args.into_iter();
    let path = args.next().map(PathBuf::from);
    if args.next().is_some() {
        bail!("expected at most one file or folder path; usage: tui [PATH]");
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_multiple_paths() {
        let args = [OsString::from("first"), OsString::from("second")];
        let error = startup_path_from(args).unwrap_err();

        assert!(error.to_string().contains("at most one"));
    }
}
