use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponsePart {
    StatusCode,
    Headers,
    Body,
    Json,
    Latency,
    ContentType,
    ContentLength,
    // Future expansion points:
    Xml,
    XPath,
    CssSelector,
    GraphQL,
    GrpcStatusCode,
    SslCertificate,
    DnsRecord,
    TcpPort,
    ProxyHealth,
}
