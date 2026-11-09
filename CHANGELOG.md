# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

### Changed

### Removed

- Commercial billing surface: wallet, PAYG metering, subscriptions, plans,
  pricing, invoices, checkout, and all related API endpoints, use cases,
  repositories, DB tables, and migrations (0132–0134). Replaced by
  instance-wide resource limits configured via `PV_MAX_*` environment
  variables.

### Fixed

## [0.1.0] — 2026-09-16

First public release.

### Added

- HTTP/HTTPS monitoring with configurable intervals, retries, custom headers and bodies, keyword and status-code assertions
- Multi-step API flows with value passing between steps
- Automatic incident open/close with flap protection and a durable notification outbox
- Public per-tenant status pages
- Real User Monitoring browser SDK (`sdk/rum`)
- Distributed check agents authenticated with mTLS
- Multi-tenancy with members, invitations, and role-based access enforced at the repository layer
- Scoped API keys
- Prometheus metrics, Grafana dashboards, and Alertmanager rules in the Compose stack
- Append-only tenant audit log
- SSRF and DNS-rebinding protection on every user-supplied URL

[Unreleased]: https://github.com/hassanbabayo85-source/pingvertex/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/hassanbabayo85-source/pingvertex/releases/tag/v0.1.0
