use serde_json::Value;

#[derive(Debug, Clone)]
pub enum Body {
    Text(String),
    Json(Value),
    Binary(Vec<u8>),
}
