use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProxyScheme {
    #[serde(rename = "http", alias = "Http", alias = "HTTP")]
    Http,
    #[serde(rename = "https", alias = "Https", alias = "HTTPS")]
    Https,
    #[serde(rename = "socks4", alias = "Socks4", alias = "SOCKS4")]
    Socks4,
    #[serde(rename = "socks5", alias = "Socks5", alias = "SOCKS5")]
    Socks5,
}
