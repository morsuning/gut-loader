//! 通过 LLM 解析数据库连接字符串/自然语言描述为结构化连接配置。
//!
//! 本模块提供与 OpenAI 兼容 API 的客户端实现，可用于从自然语言文本中
//! 抽取结构化的数据库连接参数。除 OpenAI 外，亦兼容 DeepSeek、通义千问
//! 等以 OpenAI Chat Completions 为契约的服务。

use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, error, info, warn};

use crate::models::{DatabaseConfig, LlmConfig};

// ---------------------------------------------------------------------------
// 与 OpenAI 兼容 API 交互的请求/响应结构
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<ResponseFormat>,
}

#[derive(Debug, Serialize)]
struct ResponseFormat {
    #[serde(rename = "type")]
    format_type: String,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: MessageContent,
}

#[derive(Debug, Deserialize)]
struct MessageContent {
    content: String,
}

// ---------------------------------------------------------------------------
// LLM 解析结果
// ---------------------------------------------------------------------------

/// 从文本中提取出的数据库信息。所有字段均为可选；当 LLM 无法识别时，
/// 字段为 `None`。`confidence` 表示模型对本次提取整体可靠度的估计。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ParsedDbInfo {
    pub db_type: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub database: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub schema: Option<String>,
    /// 0~1 之间的置信度。
    pub confidence: f64,
}

impl ParsedDbInfo {
    /// 将解析结果转换为 [`DatabaseConfig`]，缺失字段以默认值填充。
    ///
    /// 默认值：
    /// - `db_type`: `mysql`
    /// - `host`: `localhost`
    /// - `port`: `3306`
    /// - `database`/`username`/`password`: 空字符串
    /// - `schema`: 保留 `Option`
    pub fn to_database_config(&self) -> DatabaseConfig {
        DatabaseConfig {
            db_type: self.db_type.clone().unwrap_or_else(|| "mysql".to_string()),
            host: self.host.clone().unwrap_or_else(|| "localhost".to_string()),
            port: self.port.unwrap_or(3306),
            database: self.database.clone().unwrap_or_default(),
            username: self.username.clone().unwrap_or_default(),
            password: self.password.clone().unwrap_or_default(),
            schema: self.schema.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// LLM 客户端
// ---------------------------------------------------------------------------

/// 系统提示词：约束模型仅返回 JSON 结构化结果。
const SYSTEM_PROMPT: &str = r#"你是一个数据库连接信息提取助手。从用户提供的文本中识别数据库连接参数。

请从文本中提取以下信息并以JSON格式返回：
- db_type: 数据库类型（mysql/postgresql/opengauss/txsql/tdsql/gaussdb/oracle/dameng之一）
- host: 主机地址
- port: 端口号
- database: 数据库名称
- username: 用户名
- password: 密码
- schema: Schema名称（如有）
- confidence: 你对提取结果的置信度（0-1之间的小数）

如果某个字段无法从文本中识别，返回null。
如果文本中包含多种可能的解释，选择最合理的一个。

仅返回JSON，不要添加任何解释。"#;

/// 默认 HTTP 超时（秒）。
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// LLM 客户端。
pub struct LlmClient {
    client: Client,
    config: LlmConfig,
}

impl LlmClient {
    /// 使用给定的配置构建客户端。
    pub fn new(config: LlmConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .build()
            .unwrap_or_else(|_| Client::new());
        Self { client, config }
    }

    /// 拼接 chat completions 接口地址。
    fn endpoint(&self) -> String {
        let base = self.config.api_url.trim_end_matches('/');
        format!("{}/chat/completions", base)
    }

    /// 发送一次聊天补全请求并返回模型返回的纯文本内容。
    async fn chat(&self, messages: Vec<ChatMessage>, json_mode: bool) -> Result<String> {
        let url = self.endpoint();
        let request = ChatRequest {
            model: self.config.model.clone(),
            messages,
            temperature: 0.0,
            response_format: if json_mode {
                Some(ResponseFormat {
                    format_type: "json_object".to_string(),
                })
            } else {
                None
            },
        };

        debug!(target: "llm", url = %url, model = %self.config.model, "sending chat request");

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.config.api_key)
            .json(&request)
            .send()
            .await
            .with_context(|| format!("调用 LLM 接口失败: {}", url))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            error!(target: "llm", %status, body = %body, "LLM 接口返回错误状态");
            if status == reqwest::StatusCode::UNAUTHORIZED
                || status == reqwest::StatusCode::FORBIDDEN
            {
                return Err(anyhow!("LLM 认证失败 (HTTP {}): {}", status, body));
            }
            return Err(anyhow!("LLM 接口异常 (HTTP {}): {}", status, body));
        }

        let parsed: ChatResponse = resp.json().await.context("解析 LLM 响应 JSON 失败")?;

        let content = parsed
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or_else(|| anyhow!("LLM 响应不包含任何 choices"))?;

        debug!(target: "llm", content = %content, "received chat response");
        Ok(content)
    }

    /// 从文本中解析数据库连接信息。
    ///
    /// 当 LLM 调用失败时返回错误；当响应内容无法解析为期望结构时，
    /// 退化为尝试从原文中抽取 JSON 片段，仍失败则返回全空的
    /// [`ParsedDbInfo`]（置信度为 0）。
    pub async fn parse_database_info(&self, text: &str) -> Result<ParsedDbInfo> {
        if text.trim().is_empty() {
            warn!(target: "llm", "input text is empty, skip LLM call");
            return Ok(ParsedDbInfo::default());
        }

        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: SYSTEM_PROMPT.to_string(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: text.to_string(),
            },
        ];

        let raw = self.chat(messages, true).await?;
        info!(target: "llm", "LLM 解析完成，开始反序列化响应");

        match parse_json_payload(&raw) {
            Some(info) => Ok(info),
            None => {
                warn!(target: "llm", raw = %raw, "无法从 LLM 响应中提取 JSON，返回空结果");
                Ok(ParsedDbInfo::default())
            }
        }
    }

    /// 验证 LLM 配置是否可用。
    ///
    /// 通过发送一条 "Hello" 消息进行连通性测试。请求成功返回 `Ok(true)`，
    /// 网络/认证失败时返回 `Ok(false)`，仅在配置本身明显非法时返回 `Err`。
    pub async fn validate_config(&self) -> Result<bool> {
        if self.config.api_url.trim().is_empty() {
            return Err(anyhow!("api_url 不能为空"));
        }
        if self.config.api_key.trim().is_empty() {
            return Err(anyhow!("api_key 不能为空"));
        }
        if self.config.model.trim().is_empty() {
            return Err(anyhow!("model 不能为空"));
        }

        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: "Hello".to_string(),
        }];

        match self.chat(messages, false).await {
            Ok(_) => Ok(true),
            Err(err) => {
                warn!(target: "llm", error = %err, "LLM 配置验证失败");
                Ok(false)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 辅助函数
// ---------------------------------------------------------------------------

/// 尝试将 LLM 返回的字符串解析为 [`ParsedDbInfo`]。
///
/// 处理流程：
/// 1. 直接尝试反序列化整段文本；
/// 2. 失败则提取首个 `{...}` JSON 片段再行解析；
/// 3. 仍失败则返回 `None`。
fn parse_json_payload(raw: &str) -> Option<ParsedDbInfo> {
    let trimmed = raw.trim();
    if let Ok(info) = serde_json::from_str::<ParsedDbInfo>(trimmed) {
        return Some(info);
    }
    if let Some(snippet) = extract_json_object(trimmed) {
        if let Ok(info) = serde_json::from_str::<ParsedDbInfo>(&snippet) {
            return Some(info);
        }
    }
    None
}

/// 从任意文本中提取首个完整的 JSON 对象字符串（基于花括号配平）。
fn extract_json_object(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut start: Option<usize> = None;
    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut escape = false;

    for (i, &b) in bytes.iter().enumerate() {
        if in_string {
            if escape {
                escape = false;
            } else if b == b'\\' {
                escape = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'{' => {
                if depth == 0 {
                    start = Some(i);
                }
                depth += 1;
            }
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    if let Some(s) = start {
                        return Some(text[s..=i].to_string());
                    }
                }
            }
            _ => {}
        }
    }
    None
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_pure_json_response() {
        let raw = r#"{
            "db_type": "mysql",
            "host": "10.0.0.1",
            "port": 3306,
            "database": "orders",
            "username": "root",
            "password": "secret",
            "schema": null,
            "confidence": 0.92
        }"#;
        let info = parse_json_payload(raw).expect("should parse");
        assert_eq!(info.db_type.as_deref(), Some("mysql"));
        assert_eq!(info.host.as_deref(), Some("10.0.0.1"));
        assert_eq!(info.port, Some(3306));
        assert_eq!(info.database.as_deref(), Some("orders"));
        assert_eq!(info.username.as_deref(), Some("root"));
        assert_eq!(info.password.as_deref(), Some("secret"));
        assert!(info.schema.is_none());
        assert!((info.confidence - 0.92).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_json_wrapped_in_markdown_fence() {
        let raw = "```json\n{\"db_type\":\"postgresql\",\"port\":5432,\"confidence\":0.8}\n```";
        let info = parse_json_payload(raw).expect("should parse from fenced block");
        assert_eq!(info.db_type.as_deref(), Some("postgresql"));
        assert_eq!(info.port, Some(5432));
        assert!((info.confidence - 0.8).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_json_with_leading_explanation() {
        let raw = "解析结果如下：\n{\"db_type\":\"oracle\",\"host\":\"db.example.com\",\"port\":1521,\"confidence\":0.7}\n额外说明。";
        let info = parse_json_payload(raw).expect("should extract embedded json");
        assert_eq!(info.db_type.as_deref(), Some("oracle"));
        assert_eq!(info.host.as_deref(), Some("db.example.com"));
        assert_eq!(info.port, Some(1521));
    }

    #[test]
    fn parse_invalid_response_returns_none() {
        let raw = "完全无法识别的纯文本响应。";
        assert!(parse_json_payload(raw).is_none());
    }

    #[test]
    fn parse_partial_response_yields_defaults() {
        let raw = r#"{"db_type":"dameng","confidence":0.5}"#;
        let info = parse_json_payload(raw).expect("should parse");
        assert_eq!(info.db_type.as_deref(), Some("dameng"));
        assert!(info.host.is_none());
        assert!(info.port.is_none());
        assert_eq!(info.confidence, 0.5);
    }

    #[test]
    fn extract_json_handles_nested_objects() {
        let raw = "前言 {\"a\":{\"b\":1},\"c\":\"}\"} 收尾";
        let snippet = extract_json_object(raw).expect("should extract");
        assert_eq!(snippet, "{\"a\":{\"b\":1},\"c\":\"}\"}");
    }

    #[test]
    fn parsed_to_database_config_uses_defaults() {
        let info = ParsedDbInfo::default();
        let cfg = info.to_database_config();
        assert_eq!(cfg.db_type, "mysql");
        assert_eq!(cfg.host, "localhost");
        assert_eq!(cfg.port, 3306);
        assert_eq!(cfg.database, "");
        assert_eq!(cfg.username, "");
        assert_eq!(cfg.password, "");
        assert!(cfg.schema.is_none());
    }

    #[test]
    fn parsed_to_database_config_preserves_provided_values() {
        let info = ParsedDbInfo {
            db_type: Some("postgresql".to_string()),
            host: Some("192.168.1.10".to_string()),
            port: Some(5432),
            database: Some("app".to_string()),
            username: Some("alice".to_string()),
            password: Some("p@ss".to_string()),
            schema: Some("public".to_string()),
            confidence: 0.9,
        };
        let cfg = info.to_database_config();
        assert_eq!(cfg.db_type, "postgresql");
        assert_eq!(cfg.host, "192.168.1.10");
        assert_eq!(cfg.port, 5432);
        assert_eq!(cfg.database, "app");
        assert_eq!(cfg.username, "alice");
        assert_eq!(cfg.password, "p@ss");
        assert_eq!(cfg.schema.as_deref(), Some("public"));
    }

    #[test]
    fn endpoint_strips_trailing_slash() {
        let client = LlmClient::new(LlmConfig {
            api_url: "https://api.openai.com/v1/".to_string(),
            api_key: "sk-test".to_string(),
            model: "gpt-4o-mini".to_string(),
        });
        assert_eq!(
            client.endpoint(),
            "https://api.openai.com/v1/chat/completions"
        );
    }

    #[test]
    fn endpoint_keeps_path_without_trailing_slash() {
        let client = LlmClient::new(LlmConfig {
            api_url: "https://api.deepseek.com/v1".to_string(),
            api_key: "sk-test".to_string(),
            model: "deepseek-chat".to_string(),
        });
        assert_eq!(
            client.endpoint(),
            "https://api.deepseek.com/v1/chat/completions"
        );
    }

    #[tokio::test]
    async fn validate_config_rejects_empty_api_url() {
        let client = LlmClient::new(LlmConfig {
            api_url: "".to_string(),
            api_key: "sk".to_string(),
            model: "m".to_string(),
        });
        assert!(client.validate_config().await.is_err());
    }

    #[tokio::test]
    async fn validate_config_rejects_empty_api_key() {
        let client = LlmClient::new(LlmConfig {
            api_url: "https://api.openai.com/v1".to_string(),
            api_key: "   ".to_string(),
            model: "m".to_string(),
        });
        assert!(client.validate_config().await.is_err());
    }

    #[tokio::test]
    async fn parse_database_info_with_empty_text_returns_default() {
        let client = LlmClient::new(LlmConfig {
            api_url: "https://api.openai.com/v1".to_string(),
            api_key: "sk-test".to_string(),
            model: "gpt-4o-mini".to_string(),
        });
        let info = client.parse_database_info("   ").await.expect("ok");
        assert_eq!(info, ParsedDbInfo::default());
    }
}
