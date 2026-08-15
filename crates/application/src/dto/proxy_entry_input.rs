/// Input data for a single proxy entry submitted as part of a bulk
/// "Proxy Check" request.
///
/// `username`/`password` hold plaintext for only as long as it takes to
/// travel from the HTTP request boundary into the use case that encrypts
/// them via `ProxyCredentialCipher` before a `ProxyEntry` is persisted.
/// Once encrypted, this plaintext must not be retained, logged, or
/// stored anywhere else.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ProxyEntryInput {
    pub host: String,
    pub port: u16,
    pub scheme: domain::ProxyScheme,
    pub username: Option<String>,
    pub password: Option<String>,
}
