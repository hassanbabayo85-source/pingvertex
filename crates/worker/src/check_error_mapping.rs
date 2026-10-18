use domain::{CheckErrorKind, ErrorType};

/// Maps the canonical `CheckErrorKind` to the persisted `ErrorType`
/// used by the `checks` table. This single mapping is shared by every
/// active check executor (HTTP, TCP, DNS) so error classification never
/// drifts out of sync across check types.
pub fn map_check_error_kind_to_error_type(
    kind: CheckErrorKind,
    status_code: Option<u16>,
) -> ErrorType {
    match kind {
        CheckErrorKind::DnsError => ErrorType::Dns,
        CheckErrorKind::ConnectionError => ErrorType::Connection,
        CheckErrorKind::TlsError => ErrorType::Ssl,
        CheckErrorKind::Timeout => ErrorType::Timeout,
        CheckErrorKind::HttpError => match status_code {
            Some(status) if (400..500).contains(&status) => ErrorType::Http4xx,
            Some(status) if status >= 500 => ErrorType::Http5xx,
            _ => ErrorType::Unknown,
        },
        CheckErrorKind::AssertionFailed
        | CheckErrorKind::InternalError
        | CheckErrorKind::Unknown => ErrorType::Unknown,
    }
}
