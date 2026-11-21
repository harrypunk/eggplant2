//! OpenAI-compatible streaming provider for `eggplant-agent`.

use std::time::Duration;

use eggplant_agent::{AgentError, AgentEvent, AgentProvider, AgentRequest};
use futures_util::StreamExt as _;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct OpenAiConfig {
    pub base_url: String,
    pub model: String,
    pub api_key_env: String,
}

impl Default for OpenAiConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.openai.com/v1".to_string(),
            model: "gpt-4o-mini".to_string(),
            api_key_env: "OPENAI_API_KEY".to_string(),
        }
    }
}

#[derive(Debug)]
pub struct OpenAiProvider {
    client: reqwest::Client,
    config: OpenAiConfig,
}

impl OpenAiProvider {
    pub fn new(config: OpenAiConfig) -> Result<Self, AgentError> {
        if config.base_url.trim().is_empty()
            || !(config.base_url.starts_with("https://") || config.base_url.starts_with("http://"))
        {
            return Err(AgentError(
                "OpenAI base URL must use http or https".to_string(),
            ));
        }
        if config.model.trim().is_empty() {
            return Err(AgentError("OpenAI model must not be empty".to_string()));
        }
        if config.api_key_env.trim().is_empty() {
            return Err(AgentError(
                "OpenAI API key environment variable must not be empty".to_string(),
            ));
        }
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|_| AgentError("failed to initialize OpenAI client".to_string()))?;
        Ok(Self { client, config })
    }

    fn api_key(&self) -> Result<String, AgentError> {
        std::env::var(&self.config.api_key_env)
            .map_err(|_| AgentError(format!("{} is not set", self.config.api_key_env)))
    }

    fn endpoint(&self) -> String {
        format!(
            "{}/chat/completions",
            self.config.base_url.trim_end_matches('/')
        )
    }
}

impl AgentProvider for OpenAiProvider {
    fn stream<'a>(
        &'a self,
        request: AgentRequest,
        mut on_event: Box<dyn FnMut(AgentEvent) + Send + 'a>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), AgentError>> + Send + 'a>>
    {
        Box::pin(async move {
            if request.cancellation.is_cancelled() {
                return Ok(());
            }
            let api_key = self.api_key()?;
            let response = self
                .client
                .post(self.endpoint())
                .bearer_auth(api_key)
                .json(&serde_json::json!({
                    "model": self.config.model,
                    "messages": [{"role": "user", "content": request.prompt}],
                    "stream": true,
                }))
                .send()
                .await
                .map_err(|_| AgentError("OpenAI request failed: transport error".to_string()))?;
            let status = response.status();
            if !status.is_success() {
                return Err(AgentError(format!(
                    "OpenAI request failed with status {status}"
                )));
            }

            let mut decoder = SseDecoder::default();
            let mut stream = response.bytes_stream();
            while let Some(chunk) = stream.next().await {
                if request.cancellation.is_cancelled() {
                    return Ok(());
                }
                let chunk = chunk.map_err(|_| {
                    AgentError("OpenAI response stream failed: transport error".to_string())
                })?;
                decoder.push(&chunk, &mut on_event);
            }
            Ok(())
        })
    }
}

#[derive(Default)]
struct SseDecoder {
    buffer: Vec<u8>,
}

impl SseDecoder {
    fn push(&mut self, chunk: &[u8], on_event: &mut dyn FnMut(AgentEvent)) {
        self.buffer.extend_from_slice(chunk);
        while let Some(newline) = self.buffer.iter().position(|byte| *byte == b'\n') {
            let mut line = self.buffer.drain(..=newline).collect::<Vec<_>>();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            if line.last() == Some(&b'\n') {
                line.pop();
            }
            let Some(data) = line.strip_prefix(b"data:") else {
                continue;
            };
            let data = trim_ascii_start(data);
            if let Some(event) = decode_event(data) {
                on_event(event);
            }
        }
    }
}

fn trim_ascii_start(mut value: &[u8]) -> &[u8] {
    while value.first().is_some_and(u8::is_ascii_whitespace) {
        value = &value[1..];
    }
    value
}

fn decode_event(data: &[u8]) -> Option<AgentEvent> {
    if data == b"[DONE]" {
        return Some(AgentEvent::Complete);
    }
    let value: Value = serde_json::from_slice(data).ok()?;
    if let Some(message) = value["error"]["message"].as_str() {
        return Some(AgentEvent::Failed(bounded(message)));
    }
    let choice = &value["choices"][0];
    if choice["finish_reason"].as_str() == Some("length") {
        return Some(AgentEvent::Failed(
            "OpenAI response stopped for length".to_string(),
        ));
    }
    choice["delta"]["content"]
        .as_str()
        .filter(|content| !content.is_empty())
        .map(|content| AgentEvent::TextDelta(content.to_string()))
}

fn bounded(message: &str) -> String {
    message.chars().take(256).collect()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use eggplant_agent::CancellationToken;

    use super::*;

    #[test]
    fn decodes_fragmented_utf8_deltas_and_completion() {
        let body = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"Hello 界🙂\"}}]}\n\n",
            "data: [DONE]\n\n"
        );
        let mut decoder = SseDecoder::default();
        let mut events = Vec::new();
        for byte in body.as_bytes() {
            decoder.push(&[*byte], &mut |event| events.push(event));
        }
        assert_eq!(events.len(), 2);
        assert_eq!(events[0], AgentEvent::TextDelta("Hello 界🙂".to_string()));
        assert_eq!(events[1], AgentEvent::Complete);
    }

    #[test]
    fn length_stop_and_server_errors_do_not_execute_or_emit_content() {
        let length = br#"{"choices":[{"delta":{"content":"partial"},"finish_reason":"length"}]}"#;
        assert!(matches!(
            decode_event(length),
            Some(AgentEvent::Failed(message)) if message == "OpenAI response stopped for length"
        ));

        let error = br#"{"error":{"message":"invalid request"}}"#;
        assert_eq!(
            decode_event(error),
            Some(AgentEvent::Failed("invalid request".to_string()))
        );
    }

    #[test]
    fn ignores_comments_empty_deltas_and_non_data_lines() {
        let mut decoder = SseDecoder::default();
        let mut events = Vec::new();
        decoder.push(
            b": keepalive\n\ndata: {\"choices\":[{\"delta\":{}}]}\n\n",
            &mut |event| events.push(event),
        );
        assert!(events.is_empty());
    }

    static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    fn spawn_server(status: &str, body: String) -> (String, std::thread::JoinHandle<String>) {
        use std::io::{Read as _, Write as _};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let status = status.to_string();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut bytes = [0; 16_384];
            let count = stream.read(&mut bytes).unwrap();
            let request = String::from_utf8_lossy(&bytes[..count]).into_owned();
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
            request
        });
        (format!("http://{address}/v1"), server)
    }

    #[tokio::test]
    async fn streams_from_a_real_openai_compatible_http_endpoint() {
        let _guard = ENV_LOCK.lock().await;
        let body = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"Hello\"}}]}\n\n",
            "data: [DONE]\n\n"
        )
        .to_string();
        let (base_url, server) = spawn_server("200 OK", body);
        let key_env = "EGGPLANT_TEST_OPENAI_KEY";
        unsafe { std::env::set_var(key_env, "test-key") };
        let provider = OpenAiProvider::new(OpenAiConfig {
            base_url,
            model: "gpt-test".to_string(),
            api_key_env: key_env.to_string(),
        })
        .unwrap();
        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let output = events.clone();
        let result = provider
            .stream(
                AgentRequest {
                    prompt: "hello".to_string(),
                    cancellation: CancellationToken::default(),
                },
                Box::new(move |event| output.lock().unwrap().push(event)),
            )
            .await;

        assert!(result.is_ok());
        assert_eq!(events.lock().unwrap().len(), 2);
        assert_eq!(
            events.lock().unwrap()[0],
            AgentEvent::TextDelta("Hello".to_string())
        );
        assert_eq!(events.lock().unwrap()[1], AgentEvent::Complete);
        let request = server.join().unwrap().to_lowercase();
        assert!(request.contains("post /v1/chat/completions"));
        assert!(request.contains("authorization: bearer test-key"));
        assert!(request.contains("gpt-test"));
        assert!(request.contains("hello"));
        unsafe { std::env::remove_var(key_env) };
    }

    #[tokio::test]
    async fn http_failures_are_redacted() {
        let _guard = ENV_LOCK.lock().await;
        let (base_url, server) = spawn_server("401 Unauthorized", "{}".to_string());
        let key_env = "EGGPLANT_TEST_OPENAI_KEY";
        unsafe { std::env::set_var(key_env, "test-key") };
        let provider = OpenAiProvider::new(OpenAiConfig {
            base_url,
            model: "gpt-test".to_string(),
            api_key_env: key_env.to_string(),
        })
        .unwrap();
        let result = provider
            .stream(
                AgentRequest {
                    prompt: "hello".to_string(),
                    cancellation: CancellationToken::default(),
                },
                Box::new(|_| {}),
            )
            .await;

        let message = result.unwrap_err().0;
        assert!(message.contains("401"));
        assert!(!message.contains("test-key"));
        server.join().unwrap();
        unsafe { std::env::remove_var(key_env) };
    }

    #[test]
    fn config_validation_rejects_invalid_urls_and_models() {
        let config = OpenAiConfig {
            base_url: "file:///tmp/key".to_string(),
            ..OpenAiConfig::default()
        };
        assert!(OpenAiProvider::new(config).is_err());
        let config = OpenAiConfig {
            model: " ".to_string(),
            ..OpenAiConfig::default()
        };
        assert!(OpenAiProvider::new(config).is_err());
    }

    #[test]
    fn cancellation_is_checked_without_contacts() {
        let cancellation = CancellationToken::default();
        cancellation.cancel();
        assert!(cancellation.is_cancelled());
    }
}
