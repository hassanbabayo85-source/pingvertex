use domain::{Check, DiagnosticReport};

/// A `Check` paired with its optional diagnostic report.
///
/// The diagnostic report is stored separately (keyed by `check_id`), so
/// the application layer fetches both and returns this composite type to
/// the API layer, which then shapes it into the final JSON response.
pub struct CheckWithDiagnosticReport {
    pub check: Check,
    pub diagnostic_report: Option<DiagnosticReport>,
}
