use crate::auth::context::provide_auth_context;
use crate::components::require_auth::RequireAuth;
use crate::pages::dashboard_overview::DashboardOverviewPage;
use crate::pages::forgot_password::ForgotPasswordPage;
use crate::pages::login::LoginPage;
use crate::pages::logout::LogoutPage;
use crate::pages::monitor_detail::MonitorDetailPage;
use crate::pages::monitor_form::MonitorFormPage;
use crate::pages::monitors::MonitorsPage;
use crate::pages::not_found::NotFoundPage;
use crate::pages::register::RegisterPage;
use crate::pages::reset_password::ResetPasswordPage;
use crate::pages::rum_dashboard::RumDashboardPage;
use crate::pages::status::{StatusPage, SubdomainStatusPage};
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
                <ParentRoute path=path!("/") view=RequireAuth>
                    <Route path=path!("") view=DashboardOverviewPage/>
                    <Route path=path!("monitors") view=MonitorsPage/>
                    <Route path=path!("monitors/new") view=MonitorFormPage/>
                    <Route path=path!("monitors/:id") view=MonitorDetailPage/>
                    <Route path=path!("monitors/:id/edit") view=MonitorFormPage/>
                    <Route path=path!("rum") view=RumDashboardPage/>
                </ParentRoute>
            </Routes>
        </Router>
    }
}
