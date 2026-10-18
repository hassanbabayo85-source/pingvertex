use std::{fs, path::PathBuf};

fn api_source(path: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join(path);

    fs::read_to_string(path).unwrap()
}

#[test]
fn billing_invoice_handler_requires_view_billing_before_invoice_lookup() {
    let source = api_source("handlers/billing_finalization.rs");

    let auth = source
        .find("require(Permission::ViewBilling)")
        .expect("billing invoice handler must require ViewBilling");

    let lookup = source
        .find("find_by_id_for_tenant")
        .expect("invoice lookup must be tenant-scoped");

    assert!(
        auth < lookup,
        "billing authorization must happen before invoice lookup"
    );
}

#[test]
fn billing_invoice_handlers_use_authenticated_tenant_context() {
    let billing = api_source("handlers/billing_finalization.rs");
    let invoices = api_source("handlers/invoices.rs");

    assert!(billing.contains("tenant_context().tenant_id()"));
    assert!(invoices.contains("tenant_context().tenant_id()"));
}
