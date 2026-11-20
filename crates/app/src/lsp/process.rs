use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::Value;
use url::Url;

use super::{
    ServerMessage, decode_server_message, notification_message, request_message,
    transport::LspTransport,
};
use crate::language::{LanguageError, LanguageServerSpec};

/// A live language-server child process and its stdio transport.
pub struct LspProcess {
    child: Child,
    transport: LspTransport<ChildStdout, ChildStdin>,
}

impl LspProcess {
    pub fn spawn(
        spec: &LanguageServerSpec,
        workspace_root: &std::path::Path,
    ) -> Result<Self, LanguageError> {
        let mut child = Command::new(spec.command())
            .args(spec.args())
            .current_dir(workspace_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| LanguageError(error.to_string()))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| LanguageError("language server stdin is unavailable".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| LanguageError("language server stdout is unavailable".to_string()))?;
        let transport = LspTransport::new(stdout, stdin);
        let mut process = Self { child, transport };
        process.initialize(workspace_root)?;
        Ok(process)
    }

    fn initialize(&mut self, workspace_root: &std::path::Path) -> Result<(), LanguageError> {
        let root_uri = Url::from_file_path(workspace_root)
            .map_err(|_| LanguageError("workspace root must be an absolute path".to_string()))?;
        self.transport.send(&request_message(
            1,
            "initialize",
            serde_json::json!({
                "processId": std::process::id(),
                "rootUri": root_uri.to_string(),
                "capabilities": {},
            }),
        ))?;

        loop {
            let message = decode_server_message(self.transport.receive()?)?;
            if let ServerMessage::Response { id: 1, result } = message {
                result.map_err(|error| LanguageError(error.message))?;
                break;
            }
        }
        self.transport
            .send(&notification_message("initialized", Value::Null))
    }
}

impl Drop for LspProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAKE_SERVER: &str = r#"
read_frame() {
  local length=0 line
  while IFS= read -r line; do
    line=${line%$'\r'}
    if [[ -z $line ]]; then
      break
    fi
    if [[ $line == Content-Length:* ]]; then
      length=${line#Content-Length: }
      length=${length//[[:space:]]/}
    fi
  done
  dd if=/dev/stdin bs=1 count="$length" of=/dev/null status=none
}
read_frame
body='{"jsonrpc":"2.0","id":1,"result":{"capabilities":{}}}'
printf 'Content-Length: %s\r\n\r\n%s' "${#body}" "$body"
read_frame
"#;

    #[test]
    fn launches_and_initializes_a_real_stdio_process() {
        let workspace = tempfile::tempdir().unwrap();
        let spec = LanguageServerSpec::new("test", "bash", &["-c", FAKE_SERVER]);

        let process = LspProcess::spawn(&spec, workspace.path()).unwrap();
        drop(process);
    }

    #[test]
    fn failed_handshake_cleans_up_the_process() {
        let workspace = tempfile::tempdir().unwrap();
        let spec = LanguageServerSpec::new("test", "bash", &["-c", "exit 0"]);

        assert!(LspProcess::spawn(&spec, workspace.path()).is_err());
    }
}
