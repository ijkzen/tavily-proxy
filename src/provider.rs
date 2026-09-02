//! 提供商抽象：Tavily 与 Exa 的 search/extract 接口共性。
//!
//! 一个 [`Provider`] 描述「一类上游密钥」的全部差异点：
//! - REST 基址与工具端点路径（exa 的 extract 叫 /contents，与 tavily 不同）
//! - 请求体参数翻译（tavily 是下划线风格，exa 是驼峰风格）
//! - 鉴权头（tavily 用 Bearer，exa 用 x-api-key，两者各自原生）
//! - 成功响应的成本提取（tavily 是 credits，exa 是美元 costDollars）
//! - 错误状态判定（耗尽：tavily 432/433，exa 402）
//!
//! 具体 Provider 由 `AppState.providers` 按 kind 分发，避免每次调用都做字符串匹配。

use serde_json::{Value, json};

/// 单次工具调用的「提供商 + 该组内选中的密钥」。
pub struct Target<'a> {
    pub provider: &'a Provider,
    pub key_id: i64,
    pub api_key: String,
}

impl<'a> Target<'a> {
    pub fn new(provider: &'a Provider, key_id: i64, api_key: String) -> Self {
        Self {
            provider,
            key_id,
            api_key,
        }
    }
}

/// 一次工具调用实际产生的成本（用于簿记与展示）。
pub struct Usage {
    /// 成本数值；tavily 为 credits，exa 为美元（通常是个很小的浮点）。
    pub amount: f64,
    /// 单位说明：tavily = "credits"，exa = "usd"。
    pub unit: &'static str,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Tavily,
    Exa,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Tavily => "tavily",
            Kind::Exa => "exa",
        }
    }

    /// 从密钥前缀判定类型：exa- 开头为 Exa，其余按 Tavily 处理（存量兼容）。
    pub fn from_key(key: &str) -> Self {
        if key.starts_with("exa-") {
            Kind::Exa
        } else {
            Kind::Tavily
        }
    }
}

#[derive(Clone)]
pub struct Provider {
    pub kind: Kind,
    pub name: &'static str,
    /// REST 基址（不含尾部斜杠）。
    pub base_url: String,
    /// 搜索端点路径。
    pub search_path: &'static str,
    /// 提取端点路径。
    pub extract_path: &'static str,
}

impl Provider {
    pub fn new(kind: Kind, base_url: String) -> Self {
        match kind {
            Kind::Tavily => Self {
                kind,
                name: "tavily",
                base_url,
                search_path: "/search",
                extract_path: "/extract",
            },
            Kind::Exa => Self {
                kind,
                name: "exa",
                base_url,
                search_path: "/search",
                extract_path: "/contents",
            },
        }
    }

    /// 工具调用（search 或 extract）目标端点路径。
    pub fn tool_path(&self, tool: Tool) -> &'static str {
        match (self.kind, tool) {
            (_, Tool::Search) => self.search_path,
            (Kind::Tavily, Tool::Extract) => self.extract_path,
            (Kind::Exa, Tool::Extract) => self.extract_path,
        }
    }

    /// 应用请求鉴权头；返回 None 表示无法携带（按策略跳过该 key）。
    pub fn auth(
        &self,
        req: reqwest::RequestBuilder,
        api_key: &str,
    ) -> Option<reqwest::RequestBuilder> {
        match self.kind {
            Kind::Tavily => Some(req.bearer_auth(api_key)),
            Kind::Exa => {
                // x-api-key 必须为 ASCII；exa- 前缀密钥本来就是 ASCII
                if !api_key.is_ascii() {
                    return None;
                }
                Some(req.header("x-api-key", api_key))
            }
        }
    }

    /// 请求体参数翻译：tavily 直通（调用方参数已是其原生风格），exa 做驼峰映射。
    /// 返回 None 表示参数对 provider 无效（调用方错误）。
    pub fn translate(&self, tool: Tool, args: &Value) -> Option<Value> {
        match self.kind {
            Kind::Tavily => Some(args.clone()),
            Kind::Exa => Some(match tool {
                Tool::Search => json!({
                    "query": args["query"].as_str()?,
                    "numResults": args["numResults"].as_i64().unwrap_or(5),
                    "type": args["type"].as_str().unwrap_or("auto"),
                    "category": args.get("category"),
                    "userLocation": args.get("userLocation"),
                    "includeDomains": args.get("includeDomains"),
                    "excludeDomains": args.get("excludeDomains"),
                    "startCrawlDate": args.get("startCrawlDate"),
                    "endCrawlDate": args.get("endCrawlDate"),
                    "startPublishedDate": args.get("startPublishedDate"),
                    "endPublishedDate": args.get("endPublishedDate"),
                    "text": args.get("text").or(args.get("contents")),
                }),
                Tool::Extract => json!({
                    "urls": args["urls"].as_array()?.iter().map(Value::as_str)
                        .collect::<Option<Vec<&str>>>()?,
                    "text": args.get("text").unwrap_or(&json!(true)),
                    "highlights": args.get("highlights"),
                    "summary": args.get("summary"),
                }),
            }),
        }
    }

    /// 成功响应里提取本次成本；解析不了按 0（不影响选路，只影响记账）。
    pub fn usage_of(&self, payload: &Value) -> Usage {
        match self.kind {
            Kind::Tavily => Usage {
                amount: payload["usage"]["credits"].as_f64().unwrap_or(0.0),
                unit: "credits",
            },
            Kind::Exa => Usage {
                amount: payload["costDollars"]["total"].as_f64().unwrap_or(0.0),
                unit: "usd",
            },
        }
    }

    /// 该状态码是否表示「额度耗尽」：tavily 432/433，exa 402。
    pub fn is_exhausted(&self, status: u16) -> bool {
        match self.kind {
            Kind::Tavily => status == 432 || status == 433,
            Kind::Exa => status == 402,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Search,
    Extract,
}
