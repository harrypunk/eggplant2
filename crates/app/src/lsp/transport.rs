use std::io::{Read, Write};

use serde_json::Value;

use super::{decode_message, encode_message};
use crate::language::LanguageError;

/// Blocking LSP transport over a pair of byte streams.
pub struct LspTransport<R, W> {
    reader: R,
    writer: W,
    buffer: Vec<u8>,
}

impl<R: Read, W: Write> LspTransport<R, W> {
    pub fn new(reader: R, writer: W) -> Self {
        Self {
            reader,
            writer,
            buffer: Vec::new(),
        }
    }

    pub fn into_parts(self) -> (R, W, Vec<u8>) {
        (self.reader, self.writer, self.buffer)
    }

    pub fn from_parts(reader: R, writer: W, buffer: Vec<u8>) -> Self {
        Self {
            reader,
            writer,
            buffer,
        }
    }

    pub fn send(&mut self, message: &Value) -> Result<(), LanguageError> {
        self.writer
            .write_all(&encode_message(message))
            .and_then(|()| self.writer.flush())
            .map_err(|error| LanguageError(error.to_string()))
    }

    pub fn receive(&mut self) -> Result<Value, LanguageError> {
        loop {
            if let Some(message) = decode_message(&mut self.buffer)? {
                return Ok(message);
            }
            let mut chunk = [0; 8192];
            let count = self
                .reader
                .read(&mut chunk)
                .map_err(|error| LanguageError(error.to_string()))?;
            if count == 0 {
                let message = if self.buffer.is_empty() {
                    "LSP transport closed"
                } else {
                    "LSP transport closed during a partial frame"
                };
                return Err(LanguageError(message.to_string()));
            }
            self.buffer.extend_from_slice(&chunk[..count]);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use serde_json::json;

    use super::*;
    use crate::lsp::notification_message;

    struct ChunkedReader {
        cursor: Cursor<Vec<u8>>,
        chunk: usize,
    }

    impl Read for ChunkedReader {
        fn read(&mut self, destination: &mut [u8]) -> std::io::Result<usize> {
            let limit = destination.len().min(self.chunk);
            self.cursor.read(&mut destination[..limit])
        }
    }

    #[test]
    fn sends_and_receives_fragmented_frames() {
        let first = json!({"jsonrpc": "2.0", "method": "first"});
        let second = json!({"jsonrpc": "2.0", "method": "second"});
        let incoming = encode_message(&first)
            .into_iter()
            .chain(encode_message(&second))
            .collect::<Vec<_>>();
        let reader = ChunkedReader {
            cursor: Cursor::new(incoming),
            chunk: 3,
        };
        let mut transport = LspTransport::new(reader, Vec::new());

        assert_eq!(transport.receive().unwrap(), first);
        assert_eq!(transport.receive().unwrap(), second);

        let request = notification_message("workspace/didChange", json!({"version": 2}));
        transport.send(&request).unwrap();
        let mut written = transport.writer;
        assert_eq!(decode_message(&mut written).unwrap(), Some(request));
    }

    #[test]
    fn closed_or_partial_streams_report_errors() {
        let mut closed = LspTransport::new(Cursor::new(Vec::new()), Vec::new());
        assert!(closed.receive().is_err());

        let partial = encode_message(&json!({"partial": true}));
        let mut transport = LspTransport::new(
            Cursor::new(partial[..partial.len() - 1].to_vec()),
            Vec::new(),
        );
        assert!(transport.receive().is_err());
    }
}
