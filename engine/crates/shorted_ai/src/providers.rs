//! AI providers (docs/07 §3): OpenAI and OpenAI-compatible endpoints, Anthropic, Google Gemini.
//! Requests are built and answers read here; sending them is the caller's `Http`, so this crate
//! needs no network stack and tests can see every byte. The key never leaves a `Secret`.

use std::fmt;

use serde_json::{Value, json};

const ANTHROPIC_URL: &str = "https://api.anthropic.com";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const GEMINI_URL: &str = "https://generativelanguage.googleapis.com";
const OPENAI_URL: &str = "https://api.openai.com";
const MAX_OUTPUT_TOKENS: u32 = 4_096;
/// The structured output's name, for the providers that ask for one.
const PLAN_TOOL: &str = "edit_plan";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderKind {
    OpenAi,
    Anthropic,
    Gemini,
    /// OpenRouter, Ollama, LM Studio and others speaking the OpenAI API.
    OpenAiCompatible,
}

impl ProviderKind {
    pub fn id(self) -> &'static str {
        match self {
            Self::OpenAi => "openai",
            Self::Anthropic => "anthropic",
            Self::Gemini => "gemini",
            Self::OpenAiCompatible => "openai-compatible",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        [
            Self::OpenAi,
            Self::Anthropic,
            Self::Gemini,
            Self::OpenAiCompatible,
        ]
        .into_iter()
        .find(|kind| kind.id() == id)
    }

    /// A sensible model to start from; the user can change it.
    pub fn default_model(self) -> &'static str {
        match self {
            Self::OpenAi => "gpt-4.1-mini",
            Self::Anthropic => "claude-sonnet-5-5",
            Self::Gemini => "gemini-2.5-flash",
            Self::OpenAiCompatible => "",
        }
    }
}

/// An API key. It cannot be printed or formatted, only handed to a request.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: String) -> Self {
        Self(value)
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(hidden)")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
}

/// One HTTP exchange. Header values may hold the key, so `Debug` leaves them out.
#[derive(Clone, PartialEq)]
pub struct HttpRequest {
    pub method: Method,
    pub url: String,
    pub headers: Vec<(String, String)>,
    /// Exactly the bytes sent: what the consent sheet shows (T-0806).
    pub body: Option<String>,
}

impl fmt::Debug for HttpRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<&str> = self.headers.iter().map(|(name, _)| name.as_str()).collect();
        f.debug_struct("HttpRequest")
            .field("method", &self.method)
            .field("url", &self.url)
            .field("headers", &names)
            .field("body_bytes", &self.body.as_ref().map(String::len))
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AiError {
    /// 401 or 403: the key is wrong, expired, or lacks access to the model.
    KeyRejected,
    /// 429: too many requests, or no credit left.
    RateLimited,
    /// 5xx: the provider is having trouble.
    Unavailable,
    /// No answer: offline, wrong address, timeout.
    Network,
    /// An answer that is not what was asked for.
    BadResponse(String),
}

/// Sends a request. Implemented by the app over its HTTP client, and by tests.
pub trait Http {
    fn send(&self, request: &HttpRequest) -> Result<HttpResponse, AiError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderConfig {
    pub kind: ProviderKind,
    pub model: String,
    /// For OpenAI-compatible endpoints, and to point any provider at a proxy.
    pub base_url: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Provider {
    pub config: ProviderConfig,
    key: Secret,
}

pub(crate) fn status_error(response: &HttpResponse) -> Option<AiError> {
    match response.status {
        200..=299 => None,
        401 | 403 => Some(AiError::KeyRejected),
        429 => Some(AiError::RateLimited),
        500..=599 => Some(AiError::Unavailable),
        status => Some(AiError::BadResponse(format!("status {status}"))),
    }
}

impl Provider {
    pub fn new(config: ProviderConfig, key: Secret) -> Self {
        Self { config, key }
    }

    fn base(&self) -> String {
        let default = match self.config.kind {
            ProviderKind::OpenAi | ProviderKind::OpenAiCompatible => OPENAI_URL,
            ProviderKind::Anthropic => ANTHROPIC_URL,
            ProviderKind::Gemini => GEMINI_URL,
        };
        self.config
            .base_url
            .as_deref()
            .filter(|url| !url.trim().is_empty())
            .unwrap_or(default)
            .trim_end_matches('/')
            .to_string()
    }

    fn headers(&self) -> Vec<(String, String)> {
        let key = self.key.expose().to_string();
        let mut headers = vec![("content-type".into(), "application/json".into())];
        match self.config.kind {
            ProviderKind::OpenAi | ProviderKind::OpenAiCompatible => {
                headers.push(("authorization".into(), format!("Bearer {key}")));
            }
            ProviderKind::Anthropic => {
                headers.push(("x-api-key".into(), key));
                headers.push(("anthropic-version".into(), ANTHROPIC_VERSION.into()));
            }
            ProviderKind::Gemini => headers.push(("x-goog-api-key".into(), key)),
        }
        headers
    }

    /// Lists models: cheap, and enough to know the key and the address work.
    pub fn test_request(&self) -> HttpRequest {
        let path = match self.config.kind {
            ProviderKind::OpenAi | ProviderKind::OpenAiCompatible | ProviderKind::Anthropic => {
                "/v1/models"
            }
            ProviderKind::Gemini => "/v1beta/models",
        };
        HttpRequest {
            method: Method::Get,
            url: format!("{}{path}", self.base()),
            headers: self.headers(),
            body: None,
        }
    }

    pub fn test_connection(&self, http: &dyn Http) -> Result<(), AiError> {
        let response = http.send(&self.test_request())?;
        status_error(&response).map_or(Ok(()), Err)
    }

    /// The request asking for JSON following `schema`, through each provider's own structured
    /// output: a JSON schema format, a forced tool, or a JSON response type.
    pub fn completion_request(&self, system: &str, user: &str, schema: &Value) -> HttpRequest {
        let model = &self.config.model;
        let (path, body) = match self.config.kind {
            ProviderKind::OpenAi | ProviderKind::OpenAiCompatible => (
                "/v1/chat/completions".to_string(),
                json!({
                    "model": model,
                    "messages": [
                        {"role": "system", "content": system},
                        {"role": "user", "content": user}
                    ],
                    "response_format": {
                        "type": "json_schema",
                        "json_schema": {"name": PLAN_TOOL, "schema": schema}
                    }
                }),
            ),
            ProviderKind::Anthropic => (
                "/v1/messages".to_string(),
                json!({
                    "model": model,
                    "max_tokens": MAX_OUTPUT_TOKENS,
                    "system": system,
                    "messages": [{"role": "user", "content": user}],
                    "tools": [{
                        "name": PLAN_TOOL,
                        "description": "Return the edit plan.",
                        "input_schema": schema
                    }],
                    "tool_choice": {"type": "tool", "name": PLAN_TOOL}
                }),
            ),
            ProviderKind::Gemini => (
                format!("/v1beta/models/{model}:generateContent"),
                json!({
                    "systemInstruction": {"parts": [{"text": system}]},
                    "contents": [{"role": "user", "parts": [{"text": user}]}],
                    "generationConfig": {"responseMimeType": "application/json"}
                }),
            ),
        };
        HttpRequest {
            method: Method::Post,
            url: format!("{}{path}", self.base()),
            headers: self.headers(),
            body: Some(body.to_string()),
        }
    }

    /// Sends a prepared completion, retrying once when the provider is down or unreachable
    /// (docs/07 §3), and returns the JSON it produced.
    pub fn complete(&self, http: &dyn Http, request: &HttpRequest) -> Result<Value, AiError> {
        let response = match http.send(request) {
            Err(AiError::Network | AiError::Unavailable) => http.send(request)?,
            Ok(response) if response.status >= 500 => http.send(request)?,
            other => other?,
        };
        if let Some(error) = status_error(&response) {
            return Err(error);
        }
        self.read_completion(&response.body)
    }

    fn read_completion(&self, body: &str) -> Result<Value, AiError> {
        let bad = |what: &str| AiError::BadResponse(what.into());
        let answer: Value = serde_json::from_str(body).map_err(|_| bad("not JSON"))?;
        let text = match self.config.kind {
            ProviderKind::OpenAi | ProviderKind::OpenAiCompatible => {
                answer["choices"][0]["message"]["content"].as_str()
            }
            ProviderKind::Anthropic => {
                let tool = answer["content"]
                    .as_array()
                    .and_then(|parts| parts.iter().find(|part| part["type"] == "tool_use"));
                return tool
                    .map(|part| part["input"].clone())
                    .ok_or_else(|| bad("no tool use"));
            }
            ProviderKind::Gemini => answer["candidates"][0]["content"]["parts"][0]["text"].as_str(),
        };
        let text = text.ok_or_else(|| bad("no content"))?;
        serde_json::from_str(text).map_err(|_| bad("content is not JSON"))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    const KEY: &str = "sk-test-1234567890";

    struct Script {
        answers: RefCell<Vec<Result<HttpResponse, AiError>>>,
        sent: RefCell<Vec<HttpRequest>>,
    }

    impl Script {
        fn new(answers: Vec<Result<HttpResponse, AiError>>) -> Self {
            Self {
                answers: RefCell::new(answers),
                sent: RefCell::new(Vec::new()),
            }
        }
    }

    impl Http for Script {
        fn send(&self, request: &HttpRequest) -> Result<HttpResponse, AiError> {
            self.sent.borrow_mut().push(request.clone());
            self.answers.borrow_mut().remove(0)
        }
    }

    fn reply(status: u16, body: &str) -> Result<HttpResponse, AiError> {
        Ok(HttpResponse {
            status,
            body: body.into(),
        })
    }

    fn provider(kind: ProviderKind) -> Provider {
        Provider::new(
            ProviderConfig {
                kind,
                model: "m".into(),
                base_url: None,
            },
            Secret::new(KEY.into()),
        )
    }

    #[test]
    fn test_connection_tells_the_kinds_of_failure_apart() {
        let openai = provider(ProviderKind::OpenAi);
        for (answer, expected) in [
            (reply(200, "{}"), Ok(())),
            (reply(401, ""), Err(AiError::KeyRejected)),
            (reply(403, ""), Err(AiError::KeyRejected)),
            (reply(429, ""), Err(AiError::RateLimited)),
            (reply(503, ""), Err(AiError::Unavailable)),
            (Err(AiError::Network), Err(AiError::Network)),
        ] {
            let script = Script::new(vec![answer]);
            assert_eq!(openai.test_connection(&script), expected);
        }
    }

    #[test]
    fn each_provider_is_addressed_its_own_way() {
        let script = Script::new(vec![reply(200, "{}"), reply(200, "{}"), reply(200, "{}")]);
        for kind in [
            ProviderKind::OpenAi,
            ProviderKind::Anthropic,
            ProviderKind::Gemini,
        ] {
            provider(kind).test_connection(&script).unwrap();
        }
        let sent = script.sent.borrow();
        assert_eq!(sent[0].url, "https://api.openai.com/v1/models");
        assert!(
            sent[0]
                .headers
                .contains(&("authorization".into(), format!("Bearer {KEY}")))
        );
        assert_eq!(sent[1].url, "https://api.anthropic.com/v1/models");
        assert!(sent[1].headers.contains(&("x-api-key".into(), KEY.into())));
        assert_eq!(
            sent[2].url,
            "https://generativelanguage.googleapis.com/v1beta/models"
        );
        assert!(
            sent[2]
                .headers
                .contains(&("x-goog-api-key".into(), KEY.into()))
        );
    }

    #[test]
    fn a_compatible_endpoint_uses_its_own_address() {
        let local = Provider::new(
            ProviderConfig {
                kind: ProviderKind::OpenAiCompatible,
                model: "llama".into(),
                base_url: Some("http://localhost:11434/".into()),
            },
            Secret::new(KEY.into()),
        );
        assert_eq!(local.test_request().url, "http://localhost:11434/v1/models");
    }

    #[test]
    fn the_answer_is_read_from_each_provider_shape() {
        let schema = json!({"type": "object"});
        let cases = [
            (
                ProviderKind::OpenAi,
                r#"{"choices":[{"message":{"content":"{\"version\":1}"}}]}"#,
            ),
            (
                ProviderKind::Anthropic,
                r#"{"content":[{"type":"text","text":"x"},{"type":"tool_use","input":{"version":1}}]}"#,
            ),
            (
                ProviderKind::Gemini,
                r#"{"candidates":[{"content":{"parts":[{"text":"{\"version\":1}"}]}}]}"#,
            ),
        ];
        for (kind, body) in cases {
            let provider = provider(kind);
            let request = provider.completion_request("system", "user", &schema);
            let script = Script::new(vec![reply(200, body)]);
            assert_eq!(
                provider.complete(&script, &request).unwrap(),
                json!({"version": 1}),
                "{kind:?}"
            );
        }
    }

    #[test]
    fn a_failing_provider_is_tried_once_more() {
        let provider = provider(ProviderKind::OpenAi);
        let request = provider.completion_request("s", "u", &json!({}));
        let body = r#"{"choices":[{"message":{"content":"{}"}}]}"#;
        let script = Script::new(vec![reply(503, ""), reply(200, body)]);
        assert!(provider.complete(&script, &request).is_ok());
        assert_eq!(script.sent.borrow().len(), 2);
        let twice = Script::new(vec![Err(AiError::Network), Err(AiError::Network)]);
        assert_eq!(provider.complete(&twice, &request), Err(AiError::Network));
        let rejected = Script::new(vec![reply(401, "")]);
        assert_eq!(
            provider.complete(&rejected, &request),
            Err(AiError::KeyRejected)
        );
        assert_eq!(rejected.sent.borrow().len(), 1);
    }

    #[test]
    fn the_key_never_shows_when_printed() {
        let provider = provider(ProviderKind::Anthropic);
        let request = provider.completion_request("s", "u", &json!({}));
        for printed in [
            format!("{provider:?}"),
            format!("{request:?}"),
            format!("{:?}", Secret::new(KEY.into())),
        ] {
            assert!(!printed.contains(KEY), "{printed}");
        }
        assert!(!request.body.unwrap().contains(KEY));
    }
}
