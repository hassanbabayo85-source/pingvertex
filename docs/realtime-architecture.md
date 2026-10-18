# PingVertex Realtime Architecture Decision

Decision: use authenticated, visibility-aware polling rather than introducing SSE/WebSocket infrastructure. The current backend exposes tenant-scoped HTTP read endpoints but has no existing event broker/pub-sub channel. Polling is therefore the smallest reliable change and preserves the current architecture.

## Event model represented by polling

- monitor status changed → dashboard/monitor refresh observes the new status
- incident opened → incident list/aggregate changes
- incident acknowledged → incident acknowledgement changes
- incident resolved → resolved timestamp/status changes
- check result updated → latest check/status/latency changes

## Reconnect / backoff

- normal cadence: 15 seconds
- failures: 30s → 60s → 120s → 240s → 300s maximum
- success resets cadence to 15 seconds
- hidden tabs make no API requests; the loop wakes on a 60s visibility check
- unmount cancels the polling loop

## Authentication / tenant isolation

Polling uses the existing `/api/v1` client with browser credentials included. It therefore uses the existing authenticated cookie/session and JWT middleware. Tenant selection remains server-side from the authenticated membership; polling adds no tenant selector.

## Backpressure

Each polling surface has an in-flight guard. A new poll cannot start while the previous request is running.

## Reliability states

The existing API client already distinguishes unauthorized, forbidden, rate-limited, server, and network errors. The UI preserves successful stale data when refresh fails and shows a compact retry/degraded banner. A global browser online/offline indicator is shown when offline.

## Performance

- dashboard keeps the existing aggregate endpoint
- monitor detail polls only volatile monitor/check/incident data
- hidden tabs stop telemetry requests
- monitor list already paginates rendered rows; no virtualization was added because current pages are small
- no SSE/WebSocket connection manager was introduced
