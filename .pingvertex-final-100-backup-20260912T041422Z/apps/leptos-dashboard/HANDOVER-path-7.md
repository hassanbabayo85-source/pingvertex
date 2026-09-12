# HANDOVER — Path 7: Account Security & Authentication Completion

## Status: COMPLETE for password change, TOTP 2FA, and email verification.
## OAuth (Google/Apple) is explicitly NOT built — see "Deliberately deferred" below.

## What changed
- **Modified** `src/auth/api.rs` (extended, not restructured — all existing
  functions untouched): added `verify_email`, `resend_verification`,
  `change_password`, `enable_totp`, `confirm_totp`, `disable_totp`. Same
  conventions as the existing functions: thin wrappers over `api::client`,
  errors flattened to `String`, no token/secret ever stored outside the
  server's HttpOnly cookie.
- **New file** `src/api/account.rs` — thin re-export of
  `fetch_current_user` as `fetch_account_profile`, documented as
  intentionally thin because the backend has no dedicated account-profile
  endpoints yet.
- **New file** `src/pages/security.rs` — authenticated Security page:
  email verification status + resend, password change form, TOTP 2FA
  setup/confirm/disable.
- **New file** `src/pages/verify_email.rs` — public page at (intended)
  `/verify-email?token=...`, calls `POST /auth/verify-email` on mount.
- **Modified** `src/api/mod.rs` — added `pub mod account;` (see OWNERSHIP
  OVERRIDE below).
- **Modified** `style.css` — added `.muted`, `.totp-*`, `.backup-code-list`
  rules only, under an explicit `/* Path 7 additions */` block.

## OWNERSHIP OVERRIDE
`src/api/mod.rs` is not in Path 7's OWNED FILES, and (like `style.css` for
Path 1) no Path in the plan was assigned ownership of it. Without adding
`pub mod account;` the new `api/account.rs` file cannot compile into the
crate. This is a one-line, additive, non-conflicting change. Flagging here
per protocol; pending orchestrator reconciliation if it wants a different
owner for shared `mod.rs` registration files going forward (Path 10 is the
natural candidate alongside its `pages/mod.rs` ownership).

## Explicitly NOT touched
- `routes/mod.rs`, `pages/mod.rs` — not touched. `/settings/security` and
  `/verify-email` are not routable yet; that is Path 10's job. The Path 1
  nav already has a "Security" entry under Settings marked `ComingSoon`
  pointing at `/settings/security` — Path 10 should register
  `security.rs` there and flip that one nav item, and register
  `verify_email.rs` as a PUBLIC route (no `RequireAuth`), matching how
  `/status` and `/reset-password` are public today.

## Deliberately deferred: Google / Apple OAuth
The backend's `oauth_google` / `oauth_apple` handlers
(`POST /auth/oauth/google`, `POST /auth/oauth/apple`) expect an already-
verified provider `id_token` in the request body — the frontend is
expected to obtain that token from Google Identity Services' JS SDK
(`credential` field) or Apple JS (`authorization.id_token`), NOT from a
server-side redirect. That is compatible with this SPA's architecture in
principle, but wiring it safely requires:
1. A Google OAuth **Client ID** and an Apple **Services ID** + configured
   return URL — neither exists in any file I could read; these are
   product/infra configuration values, not something to invent.
2. Loading a third-party script (`accounts.google.com/gsi/client`,
   `appleid.cdn-apple.com/.../appisapi.js`) into the page and bridging its
   JS callback into WASM — a real, non-trivial integration surface that I
   cannot verify or test without a running browser and real credentials in
   this environment.

Per the plan's own instruction ("if it isn't compatible... document that
limitation explicitly rather than forcing it"), I'm stopping here rather
than wiring a guessed Client ID or an untested JS↔WASM callback bridge for
a security-sensitive login path. **This needs the human to provide the
Google Client ID / Apple Services ID before it's safe to build.**

## Confirmed backend gap (relevant to whoever eventually revisits this page)
`crates/api/src/handlers/current_session.rs`'s `CurrentSessionResponse`
does not include a `totp_enabled` field. The Security page therefore
cannot show a persistent "2FA is currently ON/OFF" state on page load —
only within the same browser session, right after a successful
enable/disable action. `security.rs` documents this in the UI copy itself
rather than guessing. Recommend the orchestrator file this as a backend
follow-up (`current_session.rs` + `CurrentSessionResponse` need a
`totp_enabled: bool` field) — I did NOT add it, since Path 7 has no
backend ownership.

## Verification NOT yet done (no cargo toolchain / no network in the
sandbox this was built in — must be run on your machine)
```
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p leptos-dashboard
```
Manual pass once Path 10 wires the routes:
- Change password with wrong current password → see the 401 surfaced as a
  form error, not a crash.
- Enable 2FA → scan/paste the `otpauth://` URI into an authenticator app
  → confirm with a real code → see backup codes exactly once → reload the
  page → confirm the "we can't tell" copy shows (this is the documented
  gap, not a bug).
- Disable 2FA with the wrong password → see it rejected.
- Visit `/verify-email` with no token, with a garbage token, and with a
  real token from a real verification email.
