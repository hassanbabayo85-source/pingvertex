use crate::auth::context::provide_auth_context;
use crate::components::require_auth::RequireAuth;
use crate::pages::account::AccountPage;
use crate::pages::api_flows::ApiFlowsPage;
use crate::pages::api_keys::ApiKeysPage;
use crate::pages::audit::AuditPage;
use crate::pages::billing::BillingPage;
use crate::pages::bulk_import::BulkImportPage;
use crate::pages::dashboard_overview::DashboardOverviewPage;
use crate::pages::escalation::EscalationPage;
use crate::pages::forgot_password::ForgotPasswordPage;
use crate::pages::help_center::HelpCenterPage;
use crate::pages::incident_detail::IncidentDetailPage;
use crate::pages::incidents::IncidentsPage;
use crate::pages::invitations::InvitationsPage;
use crate::pages::invoices::InvoicesPage;
use crate::pages::login::LoginPage;
use crate::pages::logout::LogoutPage;
use crate::pages::maintenance::MaintenancePage;
use crate::pages::monitor_detail::MonitorDetailPage;
use crate::pages::monitor_form::MonitorFormPage;
use crate::pages::monitors::MonitorsPage;
use crate::pages::not_found::NotFoundPage;
use crate::pages::notifications::NotificationsPage;
use crate::pages::platform_admin::PlatformAdminPage;
use crate::pages::proxy_checks::ProxyChecksPage;
use crate::pages::register::RegisterPage;
use crate::pages::reset_password::ResetPasswordPage;
use crate::pages::rum_dashboard::RumDashboardPage;
use crate::pages::rum_sites::RumSitesPage;
use crate::pages::security::SecurityPage;
use crate::pages::status::{StatusPage, SubdomainStatusPage};
use crate::pages::status_pages::StatusPagesPage;
use crate::pages::support::SupportPage;
use crate::pages::team::TeamPage;
use crate::pages::transactions::TransactionsPage;
use crate::pages::usage::UsagePage;
use crate::pages::verify_email::VerifyEmailPage;
use crate::pages::wallet::WalletPage;
use leptos::prelude::*;
use leptos_router::components::{ParentRoute, Route, Router, Routes};
use leptos_router::path;

#[component]
pub fn App() -> impl IntoView {
    provide_auth_context();

    view! {
        <Router>
            <Routes fallback=|| view! { <NotFoundPage/> }>
                <Route path=path!("/login") view=LoginPage/>
                <Route path=path!("/register") view=RegisterPage/>
                <Route path=path!("/forgot-password") view=ForgotPasswordPage/>
                <Route path=path!("/reset-password") view=ResetPasswordPage/>
                <Route path=path!("/logout") view=LogoutPage/>
                <Route path=path!("/status") view=StatusPage/>
                <Route path=path!("/status/:subdomain") view=SubdomainStatusPage/>
                <Route path=path!("/verify-email") view=VerifyEmailPage/>
                <ParentRoute path=path!("/") view=RequireAuth>
                    <Route path=path!("") view=DashboardOverviewPage/>
                    <Route path=path!("monitors") view=MonitorsPage/>
                    <Route path=path!("monitors/new") view=MonitorFormPage/>
                    <Route path=path!("monitors/import") view=BulkImportPage/>
                    <Route path=path!("monitors/:id") view=MonitorDetailPage/>
                    <Route path=path!("monitors/:id/edit") view=MonitorFormPage/>
                    <Route path=path!("incidents") view=IncidentsPage/>
                    <Route path=path!("incidents/:id") view=IncidentDetailPage/>
                    <Route path=path!("proxy-check") view=ProxyChecksPage/>
                    <Route path=path!("proxy-checks") view=ProxyChecksPage/>
                    <Route path=path!("rum") view=RumDashboardPage/>
                    <Route path=path!("rum/sites") view=RumSitesPage/>
                    <Route path=path!("status-pages") view=StatusPagesPage/>
                    <Route path=path!("team") view=TeamPage/>
                    <Route path=path!("team/invitations") view=InvitationsPage/>
                    <Route path=path!("developer/api-keys") view=ApiKeysPage/>
                    <Route path=path!("developer/api-flows") view=ApiFlowsPage/>
                    <Route path=path!("billing") view=BillingPage/>
                    <Route path=path!("billing/wallet") view=WalletPage/>
                    <Route path=path!("billing/usage") view=UsagePage/>
                    <Route path=path!("billing/invoices") view=InvoicesPage/>
                    <Route path=path!("billing/transactions") view=TransactionsPage/>
                    <Route path=path!("audit") view=AuditPage/>
                    <Route path=path!("admin") view=PlatformAdminPage/>
                    <Route path=path!("help") view=HelpCenterPage/>
                    <Route path=path!("support") view=SupportPage/>
                    <Route path=path!("maintenance") view=MaintenancePage/>
                    <Route path=path!("escalation-policies") view=EscalationPage/>
                    <Route path=path!("settings/account") view=AccountPage/>
                    <Route path=path!("settings/security") view=SecurityPage/>
                    <Route path=path!("settings/notifications") view=NotificationsPage/>
                </ParentRoute>
            </Routes>
        </Router>
    }
}
