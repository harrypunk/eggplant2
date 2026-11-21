//! OpenAI-compatible streaming provider for `eggplant-agent`.

use std::time::Duration;

use eggplant_agent::{
    AgentError, AgentEvent, AgentProvider, AgentRequest, ProviderMessageRole, ToolCall,
};
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
                    "messages": request_messages(&request),
                    "tools": tool_declarations(),
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

fn request_messages(request: &AgentRequest) -> Value {
    if request.history.is_empty() {
        return serde_json::json!([{"role": "user", "content": request.prompt}]);
    }
    request
        .history
        .iter()
        .map(|message| {
            let mut value = serde_json::json!({
                "role": match message.role {
                    ProviderMessageRole::System => "system",
                    ProviderMessageRole::User => "user",
                    ProviderMessageRole::Assistant => "assistant",
                    ProviderMessageRole::Tool => "tool",
                },
                "content": message.content,
            });
            if !message.tool_calls.is_empty() {
                value["tool_calls"] = message
                    .tool_calls
                    .iter()
                    .map(|call| {
                        serde_json::json!({
                            "id": call.id,
                            "type": "function",
                            "function": {"name": call.name, "arguments": call.arguments},
                        })
                        .clone()
                    })
                    .collect();
            }
            if let Some(call_id) = &message.tool_call_id {
                value["tool_call_id"] = Value::String(call_id.clone());
            }
            value
        })
        .collect()
}

fn tool_declarations() -> Value {
    serde_json::json!([
        {
            "type": "function",
            "function": {
                "name": "read",
                "description": "Read a bounded range from one workspace text file",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": {"type": "string", "description": "Workspace-relative file path"},
                        "offset": {"type": "integer", "minimum": 1},
                        "limit": {"type": "integer", "minimum": 1, "maximum": 200}
                    },
                    "required": ["path"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "grep",
                "description": "Search workspace text with a smart-case regex",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": {"type": "string"},
                        "path": {"type": "string", "description": "Optional workspace-relative file path"},
                        "context": {"type": "integer", "minimum": 0, "maximum": 2}
                    },
                    "required": ["query"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "write",
                "description": "Request one approved exact-text replacement in an open buffer. Read the buffer first for its ID and revision. This must be the only tool call in its round.",
                "parameters": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "buffer_id": {"type": "integer", "minimum": 1},
                        "path": {"type": "string", "description": "Workspace-relative canonical buffer path"},
                        "expected_revision": {"type": "integer"},
                        "old_text": {"type": "string", "minLength": 1},
                        "new_text": {"type": "string"}
                    },
                    "required": ["buffer_id", "path", "expected_revision", "old_text", "new_text"]
                }
            }
        }
    ])
}

#[derive(Default)]
struct SseDecoder {
    buffer: Vec<u8>,
    pending: std::collections::BTreeMap<usize, ToolCall>,
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
            if let Some(event) = decode_event(self, data) {
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

fn decode_event(decoder: &mut SseDecoder, data: &[u8]) -> Option<AgentEvent> {
    if data == b"[DONE]" {
        return Some(AgentEvent::Complete);
    }
    let value: Value = serde_json::from_slice(data).ok()?;
    if let Some(message) = value["error"]["message"].as_str() {
        return Some(AgentEvent::Failed(bounded(message)));
    }
    let choice = &value["choices"][0];
    if choice["finish_reason"].as_str() == Some("length") {
        std::mem::take(&mut decoder.pending);
        return Some(AgentEvent::Failed(
            "OpenAI response stopped for length".to_string(),
        ));
    }
    if let Some(calls) = choice["delta"]["tool_calls"].as_array() {
        for (position, call) in calls.iter().enumerate() {
            let index = call["index"].as_u64().unwrap_or(position as u64) as usize;
            let pending = decoder.pending.entry(index).or_default();
            if let Some(id) = call["id"].as_str() {
                pending.id = id.to_string();
            }
            if let Some(name) = call["function"]["name"].as_str() {
                pending.name = name.to_string();
            }
            if let Some(arguments) = call["function"]["arguments"].as_str() {
                pending.arguments.push_str(arguments);
            }
        }
    }
    let finished = choice["finish_reason"].as_str();
    if finished == Some("tool_calls") || (finished == Some("stop") && !decoder.pending.is_empty()) {
        return Some(AgentEvent::ToolCalls(
            std::mem::take(&mut decoder.pending).into_values().collect(),
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

    use eggplant_agent::{CancellationToken, ProviderMessage};

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
    fn merges_function_call_deltas_and_serializes_tool_history() {
        let mut decoder = SseDecoder::default();
        let first = br#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call-1","function":{"name":"read","arguments":"{\"path\":\""}}]}}]}
"#;
        let second = br#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"main.rs\"}"}}]},"finish_reason":"tool_calls"}]}
"#;
        let mut events = Vec::new();
        decoder.push(first, &mut |event| events.push(event));
        decoder.push(second, &mut |event| events.push(event));
        let Some(AgentEvent::ToolCalls(calls)) = events.pop() else {
            panic!("tool call should be emitted");
        };
        assert_eq!(calls[0].id, "call-1");
        assert_eq!(calls[0].name, "read");
        assert_eq!(calls[0].arguments, "{\"path\":\"main.rs\"}");

        let request = AgentRequest::follow_up(
            vec![
                ProviderMessage::user("read it".to_string()),
                ProviderMessage::assistant(String::new(), calls),
                ProviderMessage::tool("call-1".to_string(), "1:hello".to_string()),
            ],
            CancellationToken::default(),
        );
        let messages = request_messages(&request).to_string();
        assert!(messages.contains("\"role\":\"tool\""));
        assert!(messages.contains("\"tool_call_id\":\"call-1\""));
    }

    #[test]
    fn length_stop_discards_pending_function_calls() {
        let body = concat!(
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call-1\",\"function\":{\"name\":\"read\",\"arguments\":\"{\\\"path\\\":\\\"main.rs\\\"}\"}}]}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"length\"}]}\n\n",
            "data: [DONE]\n\n"
        );
        let mut decoder = SseDecoder::default();
        let mut events = Vec::new();
        body.as_bytes()
            .chunks(3)
            .for_each(|chunk| decoder.push(chunk, &mut |event| events.push(event)));

        assert_eq!(
            events,
            vec![
                AgentEvent::Failed("OpenAI response stopped for length".to_string()),
                AgentEvent::Complete,
            ]
        );
        assert!(decoder.pending.is_empty());
    }

    #[test]
    fn length_stop_and_server_errors_do_not_execute_or_emit_content() {
        let length = br#"{"choices":[{"delta":{"content":"partial"},"finish_reason":"length"}]}"#;
        let mut decoder = SseDecoder::default();
        assert!(matches!(
            decode_event(&mut decoder, length),
            Some(AgentEvent::Failed(message)) if message == "OpenAI response stopped for length"
        ));

        let error = br#"{"error":{"message":"invalid request"}}"#;
        assert_eq!(
            decode_event(&mut decoder, error),
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
                AgentRequest::new("hello".to_string(), CancellationToken::default()),
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
        assert!(request.contains("write"));
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
                AgentRequest::new("hello".to_string(), CancellationToken::default()),
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
