# Archived Documentation

This directory preserves documentation for features that have been
removed from PingVertex, kept here for historical context.

The files in this directory describe the **pre-Phase-3 commercial
billing system** (wallet, PAYG metering, subscriptions, plans, pricing,
invoices, checkout, and the related entitlement enforcement). That
system was fully removed from the codebase across Phases 1-3. It is
not part of the current open-source product.

The current product enforces **instance-wide resource limits**
configured via the `PV_MAX_*` environment variables. See:

- [`README.md`](../../README.md) — feature overview
- [`docs/SCHEDULER_BACKPRESSURE.md`](../SCHEDULER_BACKPRESSURE.md) — scheduler limits
- [`docs/ARCHITECTURE.md`](../ARCHITECTURE.md) — overall architecture

If you are looking for the current billing story: there is none.
PingVertex does not ship a payment gateway. Anyone wishing to run
PingVertex as a SaaS can re-implement billing as a downstream extension.

## Contents

- [`PAYG_RUNTIME_CONTRACT.md`](PAYG_RUNTIME_CONTRACT.md)
- [`payg-domain-and-pricing.md`](payg-domain-and-pricing.md)
- [`payg-entitlement-closure.md`](payg-entitlement-closure.md)
- [`payg-hardening-closure.md`](payg-hardening-closure.md)
- [`payg-metering-closure.md`](payg-metering-closure.md)
- [`payg-reporting-closure.md`](payg-reporting-closure.md)
- [`payg-reservation-closure.md`](payg-reservation-closure.md)
- [`payg-settlement-closure.md`](payg-settlement-closure.md)
