# PingVertex RUM SDK

Lightweight, dependency-free browser SDK for Real User Monitoring. Collects
page load timing, Core Web Vitals, JS errors, and failed resource/API
requests, and reports them to your PingVertex dashboard.

## Installation

Add this near the top of `<head>`, on every page you want monitored:

<script src="https://cdn.pingvertex.com/rum/rum-sdk.js"></script>
<script>
  PingVertexRUM.init({
    siteKey: "rum_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
  });
</script>

`siteKey` is a **public** identifier — it is safe to expose in browser
source code and devtools. It is not a secret.

### Options

PingVertexRUM.init({ siteKey: "rum_...", endpoint: "https://...", monitorFetch: true });

## What it collects

- Page path (without query string or fragment), referrer path, and page load timing (TTFB, load time)
- Core Web Vitals: FCP, LCP, CLS, and approximate INP
- Uncaught JavaScript errors and unhandled promise rejections
- Failed resource loads and failed/erroring fetch() calls when monitorFetch is enabled
- Coarse browser name/version, OS, and device type

**INP note**: the SDK reports the single largest interaction-processing duration observed on the page as `inp_ms`, not the formally-specified 98th percentile INP metric.

## What it deliberately never collects

- Form input values
- Cookies or persistent visitor identifiers; session identifier uses sessionStorage only
- Query strings or URL fragments from page URL, referrer, error URLs, or resource URLs
- Request or response bodies/headers from monitored fetch() calls

## How it sends data

Beacons are sent via `navigator.sendBeacon`, falling back to `fetch(..., {keepalive: true, mode: "no-cors"})` when unavailable, when the page becomes hidden or is navigated away from. One beacon is sent per page view.

The request body is JSON but uses `Content-Type: text/plain` intentionally to avoid a CORS preflight. The ingestion API parses the body as JSON regardless of that header.

## Failure behavior

The SDK is designed never to break the host page. Browser APIs are feature-detected and internal operations are protected. If `siteKey` is missing or invalid, it logs a warning and makes no network requests.

## Ingestion API reference

`POST /api/v1/public/rum/collect`

Request body:

{
  "site_key": "rum_...",
  "request_id": "rum_...",
  "session_key": "sess_...",
  "browser_name": "Chrome",
  "browser_version": "128.0.0.0",
  "os_name": "Windows",
  "device_type": "desktop",
  "page_view": {
    "path": "/pricing",
    "referrer_path": "/home",
    "ttfb_ms": 120,
    "fcp_ms": 800,
    "lcp_ms": 1400,
    "inp_ms": 90,
    "cls": 0.02,
    "load_time_ms": 1600
  },
  "js_errors": [
    { "message": "...", "source": "...", "line_number": 10, "column_number": 5 }
  ],
  "resource_errors": [
    { "resource_url": "...", "resource_type": "script", "status_code": 404 }
  ]
}

Responses are always `{"status": "..."}` with one of: `ok` (202), `rejected` (400/404), `rate_limited` (429), `unavailable` (503), or `error` (500).

The endpoint never returns request content back to the caller and never requires or accepts a secret credential — only the public `site_key`.

## Dashboard API reference

Everything below requires authentication using the same session/JWT as the PingVertex dashboard and is scoped to the caller tenant. A site belonging to another tenant returns `404`.

### Sites

`POST /api/v1/dashboard/rum/sites`

{ "name": "Marketing site", "domain": "example.com" }

Returns `201` with the created site, including `site_key`.

`GET /api/v1/dashboard/rum/sites` → list of the tenant sites.

{
  "id": "…",
  "name": "Marketing site",
  "domain": "example.com",
  "site_key": "rum_…",
  "created_at": "2026-08-31T12:00:00Z"
}

### Analytics

All analytics endpoints are `GET /api/v1/dashboard/rum/sites/{site_id}/...` and accept `from` and `to` RFC 3339 timestamps. Both are optional and default to the last 24 hours. Combined span is capped at 90 days.

`limit` is available on ranked-list endpoints, default 10 and maximum 100.

| Path suffix | Returns |
|---|---|
| `/overview` | Page views, sessions, avg/p95 load time, p75 LCP/INP/CLS, JS + resource error counts, error rate |
| `/pages?limit=` | Per-path breakdown with view count, avg/p95 load time, avg LCP, slowest first |
| `/errors?limit=` | JS/resource error counts and error rate, plus top JS error messages and resource URLs |
| `/browsers-devices` | Session counts grouped by browser and device type |
| `/geo?limit=` | Session counts grouped by country |
| `/trend` | Hourly page-view count, avg load time, avg LCP |
| `/sessions` | Session count, avg page views per session, avg session duration |

**Rendering note**: `message` and `resource_url` values originate in visitors browsers and are attacker-controllable. Render them as plain text and never pass them through `dangerouslySetInnerHTML`, `innerHTML`, or equivalent.

**Operator note on geo data**: `/geo` returns `country_code: null` until `GEOIP_DB_PATH` points to a readable GeoLite2 country database (default `data/geoip/GeoLite2-Country.mmdb`) and the API can see visitors real IP addresses. Reverse proxies/load balancers can cause the proxy IP to be used instead of the visitor IP.
