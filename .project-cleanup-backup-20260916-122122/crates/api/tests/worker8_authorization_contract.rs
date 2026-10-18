use std::fs;

#[test]
fn verify_checkout_requires_manage_billing_before_session_lookup() {
    let source = fs::read_to_string("src/handlers/verify_checkout.rs").unwrap();
    let auth = source.find("require(Permission::ManageBilling)").unwrap();
    let lookup = source.find("VerifyCheckoutUseCase::new").unwrap();
    assert!(
        auth < lookup,
        "authorization must happen before checkout lookup"
    );
}

#[test]
fn billing_invoice_handlers_use_authenticated_tenant_context() {
    let billing = fs::read_to_string("src/handlers/billing_finalization.rs").unwrap();
    let invoices = fs::read_to_string("src/handlers/invoices.rs").unwrap();
    assert!(billing.contains("tenant_context().tenant_id()"));
    assert!(invoices.contains("tenant_context().tenant_id()"));
}
