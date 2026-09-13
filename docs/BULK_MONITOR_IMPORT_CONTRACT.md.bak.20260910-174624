# Bulk Monitor Import Contract

This document describes the server-enforced CSV contract. The parser and
validation code are authoritative; this document must not be treated as a
source of truth when the implementation differs.

## File limits

- Maximum data rows: 10,000.
- Maximum file size for the API upload: 1,000,000 bytes.
- Maximum field size: 2,048 bytes.
- CSV must be UTF-8.
- Blank data lines are ignored.
- The header must exactly match one of the supported schemas below.

## Modern schema

```text
name,url,kind,interval_seconds,timeout_seconds,method,expected_status,tcp_port,dns_record_type,dns_expected_value,heartbeat_interval_seconds,heartbeat_grace_period_seconds,is_public,ssl_check_enabled,ssl_expiry_warning_days,regions,auth_type
```

## Legacy schema

The original 15-column schema remains accepted for compatibility:

```text
name,url,kind,interval_seconds,timeout_seconds,method,expected_status,tcp_port,dns_record_type,dns_expected_value,heartbeat_interval_seconds,heartbeat_grace_period_seconds,is_public,ssl_check_enabled,ssl_expiry_warning_days
```

Legacy rows are normalized internally to `regions=<default region>` and
`auth_type=none`.

## Fields

- `name`: required monitor name.
- `url`: required HTTP/heartbeat URL or TCP/DNS host; flow targets must be non-empty.
- `kind`: `http`, `tcp`, `dns`, `heartbeat`, or `flow`.
- `interval_seconds`: positive scheduling interval, subject to entitlement and server policy.
- `timeout_seconds`: positive timeout.
- `method`: `GET` or `HEAD`.
- `expected_status`: positive HTTP status code.
- `tcp_port`: required for TCP monitors and must be non-zero.
- `dns_record_type`: required for DNS monitors; supported values are `A`, `AAAA`, `CNAME`, `TXT`, `MX`, and `NS`.
- `dns_expected_value`: optional.
- `heartbeat_interval_seconds`: required and positive for heartbeat monitors.
- `heartbeat_grace_period_seconds`: required and positive for heartbeat monitors.
- `is_public`: optional boolean, default `false`.
- `ssl_check_enabled`: optional boolean, default `true`.
- `ssl_expiry_warning_days`: optional, default `14`.
- `regions`: semicolon-delimited region codes. Multiple regions are supported.
- `auth_type`: defaults to `none`. Bulk import currently rejects every other value.

## Validation

Every invalid row is returned with its source row number and field-specific
messages where possible. Validation includes URL/host syntax, monitor kind,
authentication type, interval/timeout constraints, monitor-specific required
fields, supported regions, duplicate identities, duplicate names, duplicate
targets, entitlement limits, and pricing availability.

## Pricing

For a multi-region row, each region is priced independently using the trusted
server-side context:

- tenant account type
- monitor kind
- region
- forecast execution quantity
- active immutable pricing version

The estimate is the sum of all region quantities multiplied by their resolved
unit prices. The confirmation stores the exact region-level pricing snapshot,
including quantity, unit price, currency, and version.


## Confirmation contract

The preview response includes `pricing_snapshot_hash`. Confirmation must send that
value as `X-Bulk-Pricing-Snapshot`. The server recomputes the snapshot from the
current trusted pricing resolution and rejects confirmation if the hash differs.
An existing idempotency key is replayed using its already-persisted snapshot
after the source hash and forecast are verified.
