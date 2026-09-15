#!/usr/bin/env bash
set -euo pipefail

ROOT="${1:-$PWD}"
cd "$ROOT"

PAGE="apps/leptos-dashboard/src/pages/proxy_checks.rs"
CSS="apps/leptos-dashboard/style.css"

test -f "$PAGE"
test -f "$CSS"

STAMP="$(date +%Y%m%d-%H%M%S)"
BACKUP=".proxy-ui-backup-$STAMP"

mkdir -p "$BACKUP"

cp "$PAGE" "$BACKUP/proxy_checks.rs"
cp "$CSS" "$BACKUP/style.css"

python3 - <<'PY'
from pathlib import Path
import re

page = Path("apps/leptos-dashboard/src/pages/proxy_checks.rs")
s = page.read_text()

# ------------------------------------------------------------
# 1. Keep uploaded filename in frontend state
# ------------------------------------------------------------

old = '''    let input = RwSignal::new(String::new());
    let input_mode = RwSignal::new("paste".to_string());
    let job = RwSignal::new(None::<ProxyJob>);
'''

new = '''    let input = RwSignal::new(String::new());
    let input_mode = RwSignal::new("paste".to_string());
    let uploaded_filename = RwSignal::new(None::<String>);
    let uploaded_proxy_count = RwSignal::new(0usize);
    let job = RwSignal::new(None::<ProxyJob>);
'''

if old not in s:
    raise SystemExit("ERROR: frontend state anchor not found")

s = s.replace(old, new, 1)

# ------------------------------------------------------------
# 2. Save filename and detected proxy count after file parsing
# ------------------------------------------------------------

old = '''                            Ok(items) => {
                                // submit_items owns the busy lifecycle while the
                                // job is being created and polled.
                                submit_items.run(items)
                            }
'''

new = '''                            Ok(items) => {
                                uploaded_filename.set(Some(file.name()));
                                uploaded_proxy_count.set(items.len());

                                // submit_items owns the busy lifecycle while the
                                // job is being created and polled.
                                submit_items.run(items)
                            }
'''

if old not in s:
    raise SystemExit("ERROR: file success block not found")

s = s.replace(old, new, 1)

# ------------------------------------------------------------
# 3. Replace the input card with premium upload presentation
# ------------------------------------------------------------

old = '''            <section class="card proxy-input-card">
                <div class="proxy-input-tabs" role="tablist" aria-label="Proxy input format">
                    <button type="button" class:active=move || input_mode.get() == "paste">"Paste"</button>
                    <label class="btn proxy-file-button">
                        "CSV Upload"
                        <input type="file" accept=".csv,text/csv" on:change=on_file_change />
                    </label>
                    <label class="btn proxy-file-button">
                        "JSON"
                        <input type="file" accept=".json,application/json" on:change=on_file_change />
                    </label>
                </div>

                <form on:submit=submit class="stack-form">
                    <label for="proxy-list">"Paste proxies"</label>
                    <textarea
                        id="proxy-list"
                        rows="10"
                        spellcheck="false"
                        autocomplete="off"
                        prop:value=input
                        placeholder="http://1.2.3.4:8080&#10;socks5://2.3.4.5:1080"
                        on:input=move |e| {
                            input.set(event_target_value(&e));
                            input_mode.set("paste".into());
                            input_error.set(None);
                        }
                    ></textarea>

                    {move || {
                        let (count, preview_error) = parse_input_for_preview(&input.get());
                        view! {
                            <div class="proxy-input-meta">
                                <span>{format!("{} proxies detected", count)}</span>
                                {preview_error.map(|message| view! {
                                    <span class="proxy-inline-error">{message}</span>
                                })}
                            </div>
                        }
                    }}

                    <button class="btn btn-primary" type="submit" disabled=move || busy.get()>
                        {move || if busy.get() { "Submitting…" } else { "Check Proxies" }}
                    </button>
                </form>

                {move || input_error.get().map(|message| view! {
                    <div class="form-error" role="alert">{message}</div>
                })}
            </section>
'''

new = '''            <section class="card proxy-input-card proxy-input-card-premium">
                <div class="proxy-input-hero">
                    <div>
                        <span class="proxy-section-kicker">"PROXY SOURCE"</span>
                        <h2>"Add proxies to inspect"</h2>
                        <p class="muted">
                            "Upload a CSV/JSON list or paste proxy endpoints. Up to 500 proxies per job."
                        </p>
                    </div>
                    <div class="proxy-source-orb" aria-hidden="true">
                        <span>"↗"</span>
                    </div>
                </div>

                <div class="proxy-input-tabs" role="tablist" aria-label="Proxy input format">
                    <button
                        type="button"
                        class:active=move || input_mode.get() == "paste"
                        on:click=move |_| input_mode.set("paste".into())
                    >
                        <span class="proxy-tab-icon">"⌨"</span>
                        " Paste"
                    </button>

                    <label class="btn proxy-file-button">
                        <span class="proxy-tab-icon">"CSV"</span>
                        " Upload CSV"
                        <input type="file" accept=".csv,text/csv" on:change=on_file_change />
                    </label>

                    <label class="btn proxy-file-button">
                        <span class="proxy-tab-icon">"JSON"</span>
                        " Upload JSON"
                        <input type="file" accept=".json,application/json" on:change=on_file_change />
                    </label>
                </div>

                {move || uploaded_filename.get().map(|filename| view! {
                    <div class="proxy-uploaded-file">
                        <div class="proxy-file-icon">"▤"</div>
                        <div class="proxy-file-details">
                            <strong>{filename.clone()}</strong>
                            <span>
                                {format!(
                                    "{} file · {} proxies detected",
                                    if filename.to_ascii_lowercase().ends_with(".json") { "JSON" } else { "CSV" },
                                    uploaded_proxy_count.get()
                                )}
                            </span>
                        </div>
                        <div class="proxy-file-state">
                            <span class="proxy-file-dot"></span>
                            " Ready"
                        </div>
                    </div>
                })}

                <form on:submit=submit class="stack-form">
                    <label for="proxy-list">"Paste proxies"</label>
                    <div class="proxy-editor-shell">
                        <div class="proxy-editor-bar">
                            <span></span><span></span><span></span>
                            <small>"PROXY LIST"</small>
                        </div>
                        <textarea
                            id="proxy-list"
                            rows="10"
                            spellcheck="false"
                            autocomplete="off"
                            prop:value=input
                            placeholder="http://1.2.3.4:8080&#10;socks5://2.3.4.5:1080"
                            on:input=move |e| {
                                input.set(event_target_value(&e));
                                input_mode.set("paste".into());
                                uploaded_filename.set(None);
                                uploaded_proxy_count.set(0);
                                input_error.set(None);
                            }
                        ></textarea>
                    </div>

                    {move || {
                        let (count, preview_error) = parse_input_for_preview(&input.get());
                        view! {
                            <div class="proxy-input-meta proxy-input-meta-premium">
                                <span>
                                    <strong>{count}</strong>
                                    " proxies detected"
                                </span>
                                <span class="proxy-input-limit">"MAX 500"</span>
                                {preview_error.map(|message| view! {
                                    <span class="proxy-inline-error">{message}</span>
                                })}
                            </div>
                        }
                    }}

                    <button class="btn btn-primary proxy-check-button" type="submit" disabled=move || busy.get()>
                        <span>"◉"</span>
                        {move || if busy.get() { "Checking…" } else { "Check Proxies" }}
                    </button>
                </form>

                {move || input_error.get().map(|message| view! {
                    <div class="form-error" role="alert">{message}</div>
                })}
            </section>
'''

if old not in s:
    raise SystemExit("ERROR: input card block not found")

s = s.replace(old, new, 1)

# ------------------------------------------------------------
# 4. Replace results table with individual proxy cards
# ------------------------------------------------------------

old = '''                        <div class="table-responsive">
                            <table class="proxy-results-table">
                                <thead>
                                    <tr>
                                        <th>"Host"</th>
                                        <th>"Port"</th>
                                        <th>"Scheme"</th>
                                        <th>"Status"</th>
                                        <th>"Latency"</th>
                                        <th>"HTTPS"</th>
                                        <th>"Country"</th>
                                        <th>"Error"</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {move || results.get().into_iter()
                                        .filter(|row| matches_filter(row, active_filter.get()))
                                        .map(|row| {
                                            let body = row.result.clone();
                                            let status = body.as_ref().map(|r| if r.reachable { "Working" } else { "Failed" }).unwrap_or("Pending");
                                            let status_class = if status == "Working" { "up" } else if status == "Failed" { "down" } else { "" };
                                            let latency = body.as_ref().and_then(|r| r.latency_ms).map(|v| format!("{} ms", v)).unwrap_or_else(|| "—".into());
                                            let https = body.as_ref().is_some_and(|r| r.supports_https);
                                            let country = body.as_ref().and_then(|r| r.country_code.clone()).unwrap_or_else(|| "—".into());
                                            let error_text = body.as_ref().filter(|r| !r.reachable).map(result_error).unwrap_or_else(|| "—".into());
                                            let error_title = error_text.clone();
                                            view! {
                                                <tr>
                                                    <td>{row.host}</td>
                                                    <td>{row.port}</td>
                                                    <td>{row.scheme}</td>
                                                    <td><span class=status_class>{status}</span></td>
                                                    <td>{latency}</td>
                                                    <td>{if https { "Yes" } else { "No" }}</td>
                                                    <td>{country}</td>
                                                    <td title=error_title>{error_text}</td>
                                                </tr>
                                            }
                                        }).collect_view()}
                                </tbody>
                            </table>
                        </div>
'''

new = '''                        <div class="proxy-result-grid">
                            {move || results.get().into_iter()
                                .filter(|row| matches_filter(row, active_filter.get()))
                                .map(|row| {
                                    let body = row.result.clone();

                                    let status = body
                                        .as_ref()
                                        .map(|r| if r.reachable { "WORKING" } else { "FAILED" })
                                        .unwrap_or("PENDING");

                                    let status_class = match status {
                                        "WORKING" => "working",
                                        "FAILED" => "failed",
                                        _ => "pending",
                                    };

                                    let latency = body
                                        .as_ref()
                                        .and_then(|r| r.latency_ms)
                                        .map(|v| format!("{} ms", v))
                                        .unwrap_or_else(|| "—".into());

                                    let https = body.as_ref().is_some_and(|r| r.supports_https);
                                    let country = body
                                        .as_ref()
                                        .and_then(|r| r.country_code.clone())
                                        .unwrap_or_else(|| "—".into());

                                    let anonymity = body
                                        .as_ref()
                                        .and_then(|r| r.anonymity.clone())
                                        .unwrap_or_else(|| "—".into());

                                    let error_text = body
                                        .as_ref()
                                        .filter(|r| !r.reachable)
                                        .map(result_error)
                                        .unwrap_or_else(|| "Connection healthy".into());

                                    let endpoint = format!(
                                        "{}://{}:{}",
                                        row.scheme,
                                        row.host,
                                        row.port
                                    );

                                    view! {
                                        <article class=format!("proxy-result-card proxy-result-{}", status_class)>
                                            <div class="proxy-result-top">
                                                <div class="proxy-result-status">
                                                    <span class="proxy-status-dot"></span>
                                                    <strong>{status}</strong>
                                                </div>
                                                <span class="proxy-result-scheme">{row.scheme.clone()}</span>
                                            </div>

                                            <div class="proxy-endpoint">
                                                <span class="proxy-endpoint-prefix">"PROXY"</span>
                                                <code>{endpoint}</code>
                                            </div>

                                            <div class="proxy-result-metrics">
                                                <div>
                                                    <span>"LATENCY"</span>
                                                    <strong>{latency}</strong>
                                                </div>
                                                <div>
                                                    <span>"HTTPS"</span>
                                                    <strong class:proxy-positive=https>
                                                        {if https { "Supported" } else { "No" }}
                                                    </strong>
                                                </div>
                                                <div>
                                                    <span>"COUNTRY"</span>
                                                    <strong>{country}</strong>
                                                </div>
                                                <div>
                                                    <span>"ANONYMITY"</span>
                                                    <strong>{anonymity}</strong>
                                                </div>
                                            </div>

                                            <div class=if status == "WORKING" {
                                                "proxy-result-message proxy-result-message-success"
                                            } else {
                                                "proxy-result-message proxy-result-message-error"
                                            }>
                                                <span>{if status == "WORKING" { "✓" } else { "!" }}</span>
                                                <span>{error_text}</span>
                                            </div>
                                        </article>
                                    }
                                })
                                .collect_view()}
                        </div>
'''

if old not in s:
    raise SystemExit("ERROR: result table block not found")

s = s.replace(old, new, 1)

page.write_text(s)
PY

cat >> "$CSS" <<'CSS'

/* ============================================================
   PingVertex — Proxy Checker Premium UI
   UI-only overrides. Existing backend/API contracts untouched.
   ============================================================ */

.proxy-check-page {
  position: relative;
  max-width: 1280px;
  isolation: isolate;
}

.proxy-check-page::before {
  content: "";
  position: absolute;
  width: 420px;
  height: 420px;
  top: -120px;
  right: -160px;
  border-radius: 50%;
  background: radial-gradient(circle, rgba(99, 102, 241, .13), transparent 68%);
  filter: blur(8px);
  pointer-events: none;
  z-index: -1;
}

.proxy-input-card-premium,
.proxy-progress-card,
.proxy-results-card {
  position: relative;
  overflow: hidden;
  border: 1px solid color-mix(in srgb, var(--border) 75%, #7c3aed 25%);
  background:
    linear-gradient(145deg, rgba(255,255,255,.075), rgba(255,255,255,.025)),
    var(--surface);
  box-shadow:
    0 20px 60px rgba(15, 23, 42, .08),
    inset 0 1px 0 rgba(255,255,255,.08);
  backdrop-filter: blur(18px);
}

.proxy-input-card-premium::after,
.proxy-progress-card::after,
.proxy-results-card::after {
  content: "";
  position: absolute;
  inset: 0;
  pointer-events: none;
  background: linear-gradient(
    120deg,
    transparent 0%,
    rgba(255,255,255,.045) 42%,
    transparent 58%
  );
}

.proxy-input-hero {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 24px;
  margin-bottom: 20px;
}

.proxy-section-kicker {
  display: inline-block;
  margin-bottom: 6px;
  font-size: .68rem;
  font-weight: 800;
  letter-spacing: .16em;
  opacity: .62;
}

.proxy-input-hero h2 {
  margin: 0;
  font-size: 1.45rem;
  letter-spacing: -.025em;
}

.proxy-input-hero p {
  margin: 6px 0 0;
}

.proxy-source-orb {
  display: grid;
  width: 58px;
  height: 58px;
  place-items: center;
  border-radius: 18px;
  background: linear-gradient(135deg, rgba(99,102,241,.20), rgba(168,85,247,.10));
  border: 1px solid rgba(139,92,246,.30);
  box-shadow: 0 0 34px rgba(99,102,241,.16);
  font-size: 1.45rem;
}

.proxy-input-tabs {
  padding: 5px;
  border-radius: 15px;
  background: rgba(15,23,42,.045);
  border: 1px solid var(--border);
  gap: 5px;
}

.proxy-input-tabs > button,
.proxy-input-tabs .proxy-file-button {
  min-height: 42px;
  border-radius: 11px;
  border: 1px solid transparent;
  transition:
    transform .2s ease,
    box-shadow .2s ease,
    background .2s ease,
    border-color .2s ease;
}

.proxy-input-tabs > button:hover,
.proxy-input-tabs .proxy-file-button:hover {
  transform: translateY(-1px);
  border-color: rgba(99,102,241,.24);
  box-shadow: 0 8px 25px rgba(99,102,241,.10);
}

.proxy-input-tabs > button.active {
  background: linear-gradient(135deg, rgba(99,102,241,.16), rgba(168,85,247,.10));
  border-color: rgba(99,102,241,.28);
  box-shadow: 0 7px 24px rgba(99,102,241,.11);
}

.proxy-tab-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  margin-right: 5px;
  font-size: .72rem;
  font-weight: 800;
  letter-spacing: .04em;
}

.proxy-uploaded-file {
  display: flex;
  align-items: center;
  gap: 13px;
  margin-top: 16px;
  padding: 13px 15px;
  border: 1px solid rgba(99,102,241,.22);
  border-radius: 15px;
  background:
    linear-gradient(135deg, rgba(99,102,241,.09), rgba(168,85,247,.045));
  box-shadow: inset 0 1px 0 rgba(255,255,255,.06);
}

.proxy-file-icon {
  display: grid;
  width: 42px;
  height: 42px;
  flex: 0 0 42px;
  place-items: center;
  border-radius: 12px;
  background: rgba(99,102,241,.13);
  color: var(--accent);
  font-weight: 800;
}

.proxy-file-details {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.proxy-file-details strong {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.proxy-file-details span {
  font-size: .78rem;
  opacity: .65;
}

.proxy-file-state {
  margin-left: auto;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  white-space: nowrap;
  font-size: .76rem;
  font-weight: 700;
}

.proxy-file-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--up);
  box-shadow: 0 0 12px rgba(34,197,94,.65);
}

.proxy-editor-shell {
  overflow: hidden;
  border: 1px solid var(--border);
  border-radius: 15px;
  background: rgba(2,6,23,.035);
  transition: border-color .2s ease, box-shadow .2s ease;
}

.proxy-editor-shell:focus-within {
  border-color: rgba(99,102,241,.45);
  box-shadow:
    0 0 0 3px rgba(99,102,241,.08),
    0 16px 40px rgba(99,102,241,.07);
}

.proxy-editor-bar {
  display: flex;
  align-items: center;
  gap: 6px;
  height: 34px;
  padding: 0 12px;
  border-bottom: 1px solid var(--border);
  background: rgba(15,23,42,.045);
}

.proxy-editor-bar > span {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: currentColor;
  opacity: .25;
}

.proxy-editor-bar small {
  margin-left: 7px;
  font-size: .62rem;
  letter-spacing: .12em;
  opacity: .45;
}

.proxy-editor-shell textarea {
  display: block;
  width: 100%;
  min-height: 190px;
  border: 0;
  border-radius: 0;
  background: transparent;
  box-shadow: none;
}

.proxy-input-meta-premium {
  align-items: center;
}

.proxy-input-limit {
  margin-left: auto;
  font-size: .65rem;
  font-weight: 800;
  letter-spacing: .12em;
  opacity: .45;
}

.proxy-check-button {
  min-height: 46px;
  position: relative;
  overflow: hidden;
  box-shadow: 0 10px 30px rgba(99,102,241,.18);
}

.proxy-check-button::before {
  content: "";
  position: absolute;
  inset: 0;
  background: linear-gradient(110deg, transparent 25%, rgba(255,255,255,.16), transparent 75%);
  transform: translateX(-120%);
  transition: transform .55s ease;
}

.proxy-check-button:hover::before {
  transform: translateX(120%);
}

/* ---------- Progress ---------- */

.proxy-progress-card {
  background:
    radial-gradient(circle at 100% 0%, rgba(99,102,241,.10), transparent 38%),
    var(--surface);
}

.proxy-progress-track {
  height: 12px;
  margin-top: 22px;
  overflow: hidden;
  border-radius: 999px;
  background: rgba(100,116,139,.13);
  border: 1px solid rgba(100,116,139,.12);
}

.proxy-progress-fill {
  position: relative;
  height: 100%;
  border-radius: inherit;
  background: linear-gradient(90deg, var(--accent), #8b5cf6, #06b6d4);
  box-shadow: 0 0 24px rgba(99,102,241,.36);
  transition: width .45s cubic-bezier(.22,1,.36,1);
}

.proxy-progress-fill::after {
  content: "";
  position: absolute;
  inset: 0;
  background: linear-gradient(90deg, transparent, rgba(255,255,255,.35), transparent);
  animation: proxy-progress-shimmer 1.8s linear infinite;
}

@keyframes proxy-progress-shimmer {
  from { transform: translateX(-100%); }
  to { transform: translateX(100%); }
}

.proxy-progress-stats {
  gap: 10px;
  margin-top: 15px;
}

.proxy-progress-stats span {
  padding: 10px 12px;
  border: 1px solid var(--border);
  border-radius: 11px;
  background: rgba(255,255,255,.025);
}

.proxy-progress-stats strong {
  margin-right: 4px;
}

/* ---------- Result summary ---------- */

.proxy-summary-card {
  position: relative;
  overflow: hidden;
  min-height: 100px;
  padding: 17px 18px;
  border: 1px solid var(--border);
  border-radius: 16px;
  background:
    linear-gradient(145deg, rgba(255,255,255,.06), rgba(255,255,255,.015)),
    var(--surface);
  box-shadow: 0 12px 30px rgba(15,23,42,.06);
}

.proxy-summary-card::before {
  content: "";
  position: absolute;
  width: 70px;
  height: 70px;
  top: -30px;
  right: -25px;
  border-radius: 50%;
  background: rgba(99,102,241,.12);
  filter: blur(15px);
}

.proxy-summary-card strong {
  position: relative;
  z-index: 1;
  font-size: 1.65rem;
}

/* ---------- Individual result cards ---------- */

.proxy-result-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 14px;
  margin-top: 18px;
}

.proxy-result-card {
  position: relative;
  overflow: hidden;
  padding: 17px;
  border: 1px solid var(--border);
  border-radius: 17px;
  background:
    linear-gradient(145deg, rgba(255,255,255,.065), rgba(255,255,255,.018)),
    var(--surface);
  box-shadow: 0 10px 30px rgba(15,23,42,.055);
  transition:
    transform .22s ease,
    border-color .22s ease,
    box-shadow .22s ease;
}

.proxy-result-card::before {
  content: "";
  position: absolute;
  left: 0;
  top: 0;
  bottom: 0;
  width: 3px;
  background: var(--accent);
  opacity: .75;
}

.proxy-result-card:hover {
  transform: translateY(-3px);
  border-color: rgba(99,102,241,.30);
  box-shadow:
    0 18px 42px rgba(15,23,42,.10),
    0 0 30px rgba(99,102,241,.06);
}

.proxy-result-failed::before {
  background: var(--down);
}

.proxy-result-working::before {
  background: var(--up);
}

.proxy-result-top {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
}

.proxy-result-status {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  font-size: .72rem;
  letter-spacing: .08em;
}

.proxy-status-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: #94a3b8;
}

.proxy-result-working .proxy-status-dot {
  background: var(--up);
  box-shadow: 0 0 13px rgba(34,197,94,.65);
}

.proxy-result-failed .proxy-status-dot {
  background: var(--down);
  box-shadow: 0 0 13px rgba(239,68,68,.55);
}

.proxy-result-scheme {
  padding: 4px 8px;
  border-radius: 7px;
  background: rgba(100,116,139,.09);
  font-size: .64rem;
  font-weight: 800;
  letter-spacing: .08em;
}

.proxy-endpoint {
  display: flex;
  align-items: center;
  gap: 9px;
  margin-top: 14px;
  padding: 11px 12px;
  border: 1px solid var(--border);
  border-radius: 11px;
  background: rgba(2,6,23,.035);
}

.proxy-endpoint-prefix {
  flex: 0 0 auto;
  font-size: .58rem;
  font-weight: 800;
  letter-spacing: .1em;
  opacity: .42;
}

.proxy-endpoint code {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: .82rem;
}

.proxy-result-metrics {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 7px;
  margin-top: 11px;
}

.proxy-result-metrics > div {
  min-width: 0;
  padding: 9px 8px;
  border-radius: 10px;
  background: rgba(100,116,139,.045);
}

.proxy-result-metrics span {
  display: block;
  margin-bottom: 3px;
  font-size: .55rem;
  font-weight: 800;
  letter-spacing: .09em;
  opacity: .45;
}

.proxy-result-metrics strong {
  display: block;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: .78rem;
}

.proxy-positive {
  color: var(--up);
}

.proxy-result-message {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  margin-top: 11px;
  padding: 9px 10px;
  border-radius: 10px;
  font-size: .73rem;
  line-height: 1.45;
}

.proxy-result-message-success {
  background: rgba(34,197,94,.065);
  color: var(--up);
}

.proxy-result-message-error {
  background: rgba(239,68,68,.065);
  color: var(--down);
}

/* ---------- Dark mode ---------- */

html[data-theme="dark"] .proxy-input-card-premium,
html[data-theme="dark"] .proxy-progress-card,
html[data-theme="dark"] .proxy-results-card,
html[data-theme="dark"] .proxy-result-card,
html[data-theme="dark"] .proxy-summary-card {
  background:
    linear-gradient(145deg, rgba(30,41,59,.72), rgba(15,23,42,.70)),
    #0f172a;
  box-shadow:
    0 20px 60px rgba(0,0,0,.24),
    inset 0 1px 0 rgba(255,255,255,.035);
}

html[data-theme="dark"] .proxy-editor-shell,
html[data-theme="dark"] .proxy-endpoint {
  background: rgba(2,6,23,.28);
}

html[data-theme="dark"] .proxy-input-tabs {
  background: rgba(2,6,23,.25);
}

/* ---------- Responsive ---------- */

@media (max-width: 900px) {
  .proxy-result-grid {
    grid-template-columns: 1fr;
  }

  .proxy-result-metrics {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}

@media (max-width: 640px) {
  .proxy-input-hero {
    align-items: flex-start;
  }

  .proxy-source-orb {
    width: 46px;
    height: 46px;
    flex: 0 0 46px;
  }

  .proxy-input-tabs {
    display: grid;
    grid-template-columns: 1fr;
  }

  .proxy-input-tabs > button,
  .proxy-input-tabs .proxy-file-button {
    width: 100%;
    justify-content: center;
  }

  .proxy-uploaded-file {
    align-items: flex-start;
  }

  .proxy-file-state {
    display: none;
  }

  .proxy-progress-stats {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .proxy-result-card {
    padding: 14px;
  }
}

@media (prefers-reduced-motion: reduce) {
  .proxy-progress-fill::after,
  .proxy-result-card,
  .proxy-check-button::before {
    animation: none;
    transition: none;
  }
}

CSS

cargo fmt --all

echo
echo "============================================================"
echo " Proxy Checker Premium UI applied"
echo "============================================================"
echo "Backup: $BACKUP"
echo
echo "Changed:"
echo "  - $PAGE"
echo "  - $CSS"
echo
echo "Next:"
echo "  cargo check -p leptos_dashboard"
echo "  cargo test -p leptos_dashboard"
echo "  trunk build --release"
echo "============================================================"
