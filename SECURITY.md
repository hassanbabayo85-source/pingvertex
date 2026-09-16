# Security Policy

## Supported versions

PingVertex is pre-1.0. Only the latest release on the `main` branch receives security fixes.

| Version | Supported |
|---|---|
| `main` (latest) | Yes |
| Older tags | No |

## Reporting a vulnerability

**Do not open a public GitHub issue for a security problem.**

Report privately through [GitHub Security Advisories](https://github.com/hassanbabayo85-source/pingvertex/security/advisories/new), or email **hassanbabayo85@gmail.com**.

Please include:

- What the issue is and roughly how bad you think it is
- Steps to reproduce, or a proof of concept
- Affected version or commit
- Any suggested fix

**What to expect:** acknowledgement within 72 hours, an initial assessment within 7 days, and updates as the fix progresses. Once a fix ships, you will be credited in the advisory unless you prefer to stay anonymous.

Please give us reasonable time to ship a fix before disclosing publicly.

## Areas of particular interest

If you are looking for somewhere to start, these are the parts of PingVertex where a bug hurts most:

- **SSRF and DNS rebinding** — `crates/shared/src/ssrf.rs`. Users control the URLs being checked; anything that gets a request to a private address is serious.
- **Tenant isolation** — any path where data from one tenant reaches another.
- **Authentication and session handling** — JWT validation, cookie flags, OAuth audience checks.
- **Secret handling at rest** — proxy credential encryption, API key storage.
- **Agent mTLS** — certificate validation on the internal agent boundary.
- **Public ingest endpoints** — the RUM collector accepts unauthenticated traffic by design.

## Out of scope

- Findings that require the operator to have deliberately set a documented development-only flag (for example `PINGVERTEX_SSRF_ALLOW_PRIVATE=true`)
- Weak defaults in `docker-compose.yml`, which is documented as local-only and is not the production path
- Missing security headers on endpoints that serve no sensitive content
- Automated scanner output with no demonstrated impact
- Denial of service through raw traffic volume

## For operators

Before running PingVertex in production, read [`SECURITY_HARDENING.md`](SECURITY_HARDENING.md) and deploy with the production Compose overlay. Rotate the database role passwords and `METRICS_TOKEN` on a schedule, and keep the metrics endpoints off the public internet.
