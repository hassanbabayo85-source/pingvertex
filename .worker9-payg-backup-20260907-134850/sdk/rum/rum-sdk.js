/*!
 * PingVertex RUM (Real User Monitoring) SDK
 *
 * Framework-agnostic, dependency-free. Designed to be embedded via a
 * plain <script> tag on ANY customer site (not bundled into
 * apps/dashboard, which is a separate, unrelated Next.js app).
 *
 * PRIVACY / SECURITY, read before changing anything in this file:
 *
 * - NEVER collect form input values, cookies, or full URLs with query
 *   strings/fragments. Only `location.pathname` is ever sent — see
 *   `sanitizePath()`. This mirrors the server-side rejection of any path
 *   containing "?" or "#" in `RumPageView::new` (crates/domain).
 * - NEVER send a secret credential. `siteKey` is a PUBLIC, non-secret
 *   identifier (see `RumSite::site_key` server-side) — safe to embed in
 *   page source, safe to see in devtools network tab.
 * - Beacons are sent with `navigator.sendBeacon` using a Blob of type
 *   "text/plain;charset=UTF-8" — deliberately NOT "application/json".
 *   This is what lets the browser treat the request as a CORS "simple
 *   request" (no preflight), so beacons can be delivered to the
 *   ingestion API from any customer domain without that domain needing
 *   to be granted CORS access server-side. The ingestion endpoint
 *   parses the body as JSON regardless of the Content-Type header it
 *   receives — see crates/api/src/handlers/rum_ingest.rs. If you ever
 *   change the transport here, you MUST also change that handler, or
 *   cross-origin delivery will silently break.
 * - This file must NEVER throw an uncaught exception into the host
 *   page. Every browser API used here (PerformanceObserver, Beacon,
 *   etc.) is feature-detected and wrapped in try/catch — a RUM SDK
 *   bug must not be able to break the site it's installed on.
 */
(function (window, document) {
  "use strict";

  var DEFAULT_ENDPOINT = "https://ingest.pingvertex.com/api/v1/public/rum/collect";

  // Mirrors MAX_JS_ERRORS_PER_BEACON / MAX_RESOURCE_ERRORS_PER_BEACON in
  // crates/application/src/use_cases/ingest_rum_event.rs. Capping
  // client-side too keeps beacons small and avoids relying solely on the
  // server to enforce it.
  var MAX_ERRORS_PER_BEACON = 25;

  var config = {
    siteKey: null,
    endpoint: DEFAULT_ENDPOINT,
  };

  var state = {
    initialized: false,
    sessionKey: null,
    jsErrors: [],
    resourceErrors: [],
    sent: false,
  };

  // ---------------------------------------------------------------------
  // Utilities
  // ---------------------------------------------------------------------

  function safe(fn) {
    // Runs `fn` and swallows any exception. Used everywhere a browser API
    // might be missing or behave unexpectedly — this SDK must never be
    // the reason a host page breaks.
    try {
      return fn();
    } catch (err) {
      return undefined;
    }
  }

  /**
   * Returns `location.pathname` only — no scheme, host, query string, or
   * fragment. Query strings and fragments frequently carry PII or
   * secrets (tokens, emails, session identifiers) as a common web
   * pattern, so they are never even read here, let alone sent.
   */
  function sanitizePath(pathname) {
    if (typeof pathname !== "string" || pathname.length === 0) {
      return "/";
    }
    // Defensive: some SPA routers put "?"/"#" INSIDE what they treat as
    // the "path" (hash-based routing). Strip anything from the first
    // "?" or "#" onward, in addition to only ever reading
    // `location.pathname` (which itself never includes the query
    // string) in the first place.
    var cut = pathname.length;
    var q = pathname.indexOf("?");
    var h = pathname.indexOf("#");
    if (q !== -1) cut = Math.min(cut, q);
    if (h !== -1) cut = Math.min(cut, h);
    var path = pathname.slice(0, cut);
    return path.length > 0 ? path : "/";
  }

  /**
   * Same treatment as `sanitizePath`, applied to `document.referrer`
   * (also same-document-relative-safe: only the path component is kept).
   */
  function sanitizeReferrerPath(referrer) {
    if (typeof referrer !== "string" || referrer.length === 0) {
      return null;
    }
    return safe(function () {
      var url = new URL(referrer);
      return sanitizePath(url.pathname);
    }) || null;
  }

  function randomHex(byteLength) {
    return safe(function () {
      var bytes = new Uint8Array(byteLength);
      window.crypto.getRandomValues(bytes);
      var hex = "";
      for (var i = 0; i < bytes.length; i++) {
        hex += bytes[i].toString(16).padStart(2, "0");
      }
      return hex;
    });
  }

  /**
   * Session identifier: a random, opaque, non-PII value grouping this
   * tab's events. Stored in `sessionStorage` (NOT `localStorage`, NOT a
   * cookie) so it's automatically scoped to one tab and cleared when the
   * tab closes — there is no reason for a session key to outlive the tab
   * it was generated for, and using longer-lived storage would make this
   * look and behave like a persistent visitor identifier, which it is
   * explicitly NOT meant to be.
   */
  function getOrCreateSessionKey() {
    var storageKey = "__pv_rum_session__";
    var existing = safe(function () {
      return window.sessionStorage.getItem(storageKey);
    });
    if (existing) {
      return existing;
    }
    var fresh = "sess_" + (randomHex(16) || String(Date.now()) + String(Math.random()));
    safe(function () {
      window.sessionStorage.setItem(storageKey, fresh);
    });
    return fresh;
  }

  // ---------------------------------------------------------------------
  // Browser / OS / device detection
  // ---------------------------------------------------------------------
  //
  // Deliberately simple, best-effort heuristics — NOT a full UA-parsing
  // library. These fields are informational (dashboard breakdowns), not
  // security- or billing-relevant, so an occasional misclassification on
  // an unusual browser is an acceptable trade-off for keeping this file
  // small and dependency-free.

  function detectBrowser(ua) {
    var browsers = [
      ["Edg", "Edge"],
      ["OPR", "Opera"],
      ["Firefox", "Firefox"],
      ["Chrome", "Chrome"],
      ["Safari", "Safari"],
    ];
    for (var i = 0; i < browsers.length; i++) {
      var token = browsers[i][0];
      var name = browsers[i][1];
      var idx = ua.indexOf(token);
      if (idx !== -1) {
        var versionMatch = ua.slice(idx).match(/[\w]+\/([\d.]+)/);
        return {
          name: name,
          version: versionMatch ? versionMatch[1] : null,
        };
      }
    }
    return { name: null, version: null };
  }

  function detectOs(ua) {
    if (ua.indexOf("Windows") !== -1) return "Windows";
    if (ua.indexOf("Mac OS X") !== -1) return "macOS";
    if (ua.indexOf("Android") !== -1) return "Android";
    if (ua.indexOf("iPhone") !== -1 || ua.indexOf("iPad") !== -1) return "iOS";
    if (ua.indexOf("Linux") !== -1) return "Linux";
    return null;
  }

  function detectDeviceType(ua) {
    if (/iPad|Tablet/i.test(ua)) return "tablet";
    if (/Mobi|Android(?!.*Tablet)|iPhone/i.test(ua)) return "mobile";
    return "desktop";
  }

  // ---------------------------------------------------------------------
  // Timing / Core Web Vitals
  // ---------------------------------------------------------------------

  function round(value) {
    return typeof value === "number" && isFinite(value) ? Math.round(value) : null;
  }

  function collectNavigationTiming() {
    return safe(function () {
      var entries = performance.getEntriesByType("navigation");
      var nav = entries && entries[0];
      if (!nav) return {};
      return {
        ttfb_ms: round(nav.responseStart),
        load_time_ms: round(nav.loadEventEnd > 0 ? nav.loadEventEnd : nav.duration),
      };
    }) || {};
  }

  function collectFcp() {
    return safe(function () {
      var entries = performance.getEntriesByType("paint");
      for (var i = 0; i < entries.length; i++) {
        if (entries[i].name === "first-contentful-paint") {
          return round(entries[i].startTime);
        }
      }
      return null;
    });
  }

  /**
   * LCP, INP, and CLS all require a `PerformanceObserver` that stays
   * subscribed for the page's lifetime (the "largest" contentful paint,
   * the worst interaction latency, and the cumulative layout shift are
   * all only known once the page is about to be left). `startVitalsObservers`
   * wires these up once at init; `readVitals()` reads back whatever's
   * been observed so far when it's time to send a beacon.
   */
  var vitals = {
    lcp: null,
    inpMax: null, // simplified INP approximation — see note below.
    clsSum: 0,
  };

  function startVitalsObservers() {
    if (typeof PerformanceObserver === "undefined") {
      return;
    }

    safe(function () {
      var po = new PerformanceObserver(function (list) {
        var entries = list.getEntries();
        var last = entries[entries.length - 1];
        if (last) {
          vitals.lcp = round(last.renderTime || last.loadTime || last.startTime);
        }
      });
      po.observe({ type: "largest-contentful-paint", buffered: true });
    });

    // Simplified INP: tracks the single largest event-processing
    // duration seen so far, rather than the full "98th percentile across
    // all interactions" the formal INP metric specifies. Chosen
    // deliberately for simplicity/robustness across browsers — a true
    // INP implementation needs sorted-array bookkeeping across the whole
    // page lifetime, which is unnecessary complexity for a first cut.
    // This is documented in HANDOVER.md as an approximation, not a
    // spec-compliant INP value.
    safe(function () {
      var po = new PerformanceObserver(function (list) {
        var entries = list.getEntries();
        for (var i = 0; i < entries.length; i++) {
          var duration = entries[i].duration;
          if (vitals.inpMax === null || duration > vitals.inpMax) {
            vitals.inpMax = round(duration);
          }
        }
      });
      po.observe({ type: "event", durationThreshold: 40, buffered: true });
    });

    safe(function () {
      var po = new PerformanceObserver(function (list) {
        var entries = list.getEntries();
        for (var i = 0; i < entries.length; i++) {
          var entry = entries[i];
          // Standard CLS rule: ignore shifts that happened shortly after
          // user input (they're expected, not a UX problem).
          if (!entry.hadRecentInput) {
            vitals.clsSum += entry.value;
          }
        }
      });
      po.observe({ type: "layout-shift", buffered: true });
    });
  }

  function readVitals() {
    return {
      lcp_ms: vitals.lcp,
      inp_ms: vitals.inpMax,
      cls: vitals.clsSum > 0 ? Math.round(vitals.clsSum * 1000) / 1000 : null,
    };
  }

  // ---------------------------------------------------------------------
  // Error collection
  // ---------------------------------------------------------------------

  var RESOURCE_TAG_TO_TYPE = {
    SCRIPT: "script",
    LINK: "stylesheet",
    IMG: "image",
  };

  function recordJsError(message, source, lineNumber, columnNumber) {
    if (state.jsErrors.length >= MAX_ERRORS_PER_BEACON) return;
    state.jsErrors.push({
      message: String(message || "Unknown error").slice(0, 2000),
      source: source ? sanitizeResourceUrl(source) : null,
      line_number: typeof lineNumber === "number" ? lineNumber : null,
      column_number: typeof columnNumber === "number" ? columnNumber : null,
    });
  }

  function recordResourceError(resourceUrl, resourceType, statusCode) {
    if (state.resourceErrors.length >= MAX_ERRORS_PER_BEACON) return;
    state.resourceErrors.push({
      resource_url: sanitizeResourceUrl(resourceUrl),
      resource_type: resourceType || "other",
      status_code: typeof statusCode === "number" ? statusCode : null,
    });
  }

  /**
   * Strips query strings from a resource/fetch URL before it's ever
   * stored in memory, for the same reason `sanitizePath` strips them
   * from the page path: query params are a common place for tokens/PII
   * to leak, and this data is attacker/visitor-controlled content the
   * dashboard will later render as plain text (never trusted HTML) —
   * see the domain-layer notes on `RumJsError`/`RumResourceError`.
   */
  function sanitizeResourceUrl(url) {
    return safe(function () {
      var parsed = new URL(url, location.href);
      return parsed.origin + parsed.pathname;
    }) || String(url).split("?")[0].slice(0, 2048);
  }

  function installErrorListeners() {
    // Real script errors. `capture: true` is required to ALSO catch
    // resource-load failures dispatched on the elements themselves
    // (img/script/link `error` events don't bubble), which we route to
    // resourceErrors instead of jsErrors below.
    window.addEventListener(
      "error",
      function (event) {
        safe(function () {
          if (event.target && event.target !== window && event.target.tagName) {
            var tag = event.target.tagName.toUpperCase();
            var type = RESOURCE_TAG_TO_TYPE[tag];
            if (type) {
              var src = event.target.src || event.target.href;
              if (src) {
                recordResourceError(src, type, null);
              }
              return;
            }
          }
          // A real script error.
          recordJsError(event.message, event.filename, event.lineno, event.colno);
        });
      },
      true
    );

    window.addEventListener("unhandledrejection", function (event) {
      safe(function () {
        var reason = event.reason;
        var message =
          reason && reason.message ? reason.message : "Unhandled promise rejection";
        recordJsError(message, null, null, null);
      });
    });
  }

  /**
   * Optional: wraps `window.fetch` to report failed/erroring API calls
   * as resource errors. Only reads method/URL/status — NEVER inspects
   * request or response bodies or headers, which could contain
   * authentication tokens or user data.
   */
  function installFetchMonitor() {
    if (typeof window.fetch !== "function") return;

    var originalFetch = window.fetch;
    window.fetch = function () {
      var args = arguments;
      var url = safe(function () {
        return args[0] && args[0].url ? args[0].url : String(args[0]);
      });

      return originalFetch.apply(window, args).then(
        function (response) {
          safe(function () {
            if (!response.ok) {
              recordResourceError(url, "fetch", response.status);
            }
          });
          return response;
        },
        function (err) {
          safe(function () {
            recordResourceError(url, "fetch", null);
          });
          throw err;
        }
      );
    };
  }

  // ---------------------------------------------------------------------
  // Sending
  // ---------------------------------------------------------------------

  function buildPayload() {
    var ua = navigator.userAgent || "";
    var browser = detectBrowser(ua);
    var nav = collectNavigationTiming();

    var pageView = {
      path: sanitizePath(location.pathname),
      referrer_path: sanitizeReferrerPath(document.referrer),
      ttfb_ms: nav.ttfb_ms != null ? nav.ttfb_ms : null,
      fcp_ms: collectFcp(),
      load_time_ms: nav.load_time_ms != null ? nav.load_time_ms : null,
    };
    var v = readVitals();
    pageView.lcp_ms = v.lcp_ms;
    pageView.inp_ms = v.inp_ms;
    pageView.cls = v.cls;

    return {
      site_key: config.siteKey,
      session_key: state.sessionKey,
      browser_name: browser.name,
      browser_version: browser.version,
      os_name: detectOs(ua),
      device_type: detectDeviceType(ua),
      page_view: pageView,
      js_errors: state.jsErrors.slice(0, MAX_ERRORS_PER_BEACON),
      resource_errors: state.resourceErrors.slice(0, MAX_ERRORS_PER_BEACON),
    };
  }

  function send() {
    if (state.sent || !state.initialized) return;
    state.sent = true;

    safe(function () {
      var payload = buildPayload();
      var body = JSON.stringify(payload);

      // "text/plain" (NOT "application/json") is what keeps this a
      // CORS-simple request — see the file-level comment.
      var blob = new Blob([body], { type: "text/plain;charset=UTF-8" });

      var delivered =
        typeof navigator.sendBeacon === "function" &&
        navigator.sendBeacon(config.endpoint, blob);

      if (!delivered) {
        // Fallback for browsers/situations where sendBeacon is
        // unavailable or refuses the payload (e.g. over its internal
        // size limit). `keepalive: true` asks the browser to let the
        // request outlive page unload, matching sendBeacon's guarantee
        // as closely as fetch can. `mode: "no-cors"` matches the same
        // "don't require CORS headers" reasoning as the Content-Type
        // choice above; the response is opaque and never read.
        safe(function () {
          fetch(config.endpoint, {
            method: "POST",
            mode: "no-cors",
            keepalive: true,
            headers: { "Content-Type": "text/plain;charset=UTF-8" },
            body: body,
          });
        });
      }
    });
  }

  function installSendTriggers() {
    // `visibilitychange` -> "hidden" is the modern, reliable signal that
    // the user is leaving/backgrounding the page (works consistently on
    // mobile, unlike `unload`/`beforeunload`). `pagehide` is a fallback
    // for older engines. Either firing first sends the beacon; `send()`
    // is idempotent (state.sent guard) so a double-fire is harmless.
    document.addEventListener("visibilitychange", function () {
      if (document.visibilityState === "hidden") {
        send();
      }
    });
    window.addEventListener("pagehide", send);
  }

  // ---------------------------------------------------------------------
  // Public API
  // ---------------------------------------------------------------------

  var PingVertexRUM = {
    /**
     * init({ siteKey, endpoint? })
     *
     * `siteKey` (required): the PUBLIC site identifier from the
     * PingVertex dashboard (looks like "rum_<32 hex chars>"). This is
     * NOT a secret — it's fine for it to be visible in page source or
     * devtools.
     *
     * `endpoint` (optional): override the ingestion URL. Only needed for
     * self-hosted/dev setups; defaults to PingVertex's production
     * ingestion endpoint.
     */
    init: function (options) {
      if (state.initialized) return;
      options = options || {};

      if (!options.siteKey || typeof options.siteKey !== "string") {
        // Fails loudly to the CONSOLE (developer-facing), never to the
        // page itself — a misconfigured SDK must not visibly break the
        // site. No network request is made without a site key.
        safe(function () {
          console.warn("[PingVertexRUM] init() requires a `siteKey`; RUM collection disabled.");
        });
        return;
      }

      config.siteKey = options.siteKey;
      if (options.endpoint) {
        config.endpoint = options.endpoint;
      }

      state.sessionKey = getOrCreateSessionKey();
      state.initialized = true;

      startVitalsObservers();
      installErrorListeners();
      if (options.monitorFetch !== false) {
        installFetchMonitor();
      }
      installSendTriggers();
    },
  };

  window.PingVertexRUM = PingVertexRUM;
})(window, document);
