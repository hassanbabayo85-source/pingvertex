# PingVertex Frontend Phase 0 → Phase 1 — Handover (Ƙarshe, an Tabbatar)

**Matsayi**: Dukkan hujjojin da ke ƙasa an tabbatar da su ta hanyar bincike na kai-tsaye
a cikin codebase (grep, cat, cargo check) — babu hasashe. Ranar bincike: 2026-09-03.

---

## 0. Build/Verification Command (an tabbatar)

Kowanne worker, kafin ya fara aiki kuma bayan kowanne canji, ya gudanar da:

```bash
cd apps/leptos-dashboard && cargo check
```

Idan ana son daidaita da abin da CI ke bincika (wasm target):

```bash
cd apps/leptos-dashboard && cargo check --target wasm32-unknown-unknown
```

**Bayani**: `apps/leptos-dashboard` memba ne na babban Cargo workspace (root
`Cargo.toml`), amma yana da nasa `[package]` (BABU `[workspace]` a cikinsa).
`cargo check` (BABU `--workspace` flag) yana bincikar `leptos-dashboard` da
abubuwan da ta dogara da su KAWAI — leptos, leptos_router, wasm-bindgen, gloo-net,
da sauransu. Ba ya taɓa gina crates na backend (domain/application/infra/api/
worker/agent) balle sqlx/tokio/axum. An tabbata: cargo check na farko ya gama a
cikin 21.91s (real build), sannan wasm target ya kammala shi ma.

**GARGADI**: CI (`leptos-frontend.yml`) yana amfani da `cargo clippy --workspace
--all-targets` a cikin wannan directory ɗin. Flag ɗin `--workspace` zai haɗa
DUKKAN members na babban workspace (har da backend crates) kuma ya yi ƙoƙarin
gina su duka don `wasm32-unknown-unknown` — wannan matsala ce ta CI-level da ba
lallai workers su warware ba yayin gwaji na gida, amma za a bukaci a duba ta
kafin PR na ƙarshe ya wuce CI. **Workers KADA su yi amfani da `--workspace`
flag lokacin gwajin gida.**

---

## 1. Gyare-gyare a kan Risks na Asali (bisa hujja)

| # | Risk na asali | Ainihin Hujja da aka Tabbatar |
|---|---|---|
| 1 | RUM rewrite — "sauya call 1 zuwa 9 functions" | **Ba daidai ba ne cikakke.** `api/rum.rs` YA RIGA YA ƙunshi cikakkun functions 9 na typed (daidai da backend contract). Ainihin aikin shi ne: `pages/rum_dashboard.rs` yana amfani da su ta hanyar `fetch_rum_dashboard()` legacy wrapper wanda ke kiran functions 5 daga cikinsu **a JERE (sequential awaits), BA a layi ɗaya ba (parallel)**, sannan yana canza sunayen filaye zuwa `Legacy*` structs guda 9 domin daidaita da tsohon UI. Wannan har yanzu babban aiki ne (Path 1a/1b split na iya buƙata), amma ba "canza call guda zuwa 9" ba — su 9 ɗin sun riga sun kasance. |
| 2 | Dashboard incidents aggregate — "wajen scope, ana bukatar sabon backend endpoint" | **KURAKURE. AN WARWARE SHI TUNI a backend.** `GET /dashboard/overview` (handler + `PostgresDashboardRepository`) YA RIGA YA DAWO da `incident_summary.recent: Vec<IncidentSummary>` (5 incidents na baya-bayan nan) a cikin JSON guda ɗaya, ta hanyar query guda ɗaya (babu N+1, ana amfani da LATERAL joins). **Babu bukatar sabon backend endpoint.** Kawai frontend `Overview` struct (`api/monitoring.rs`) ba shi da filin `recent` — yana da `active_incidents: u64` kaɗai. Aikin da ya rage: (a) Path 1 ya ƙara `recent: Vec<IncidentSummary>` zuwa `Overview` struct, (b) Path 3 ya nuna cikakken list a maimakon aggregate count kawai a "Active Incidents" widget. |
| 3 | Heartbeat backend support — "bai tabbata ba tukuna" | **KURAKURE. Backend YA CIKA sosai.** `MonitorKind::Heartbeat`, `HeartbeatToken`, `HeartbeatSweeper` (worker), `GET /api/v1/public/heartbeat/{token}` endpoint, cikakkun tests (`crates/api/tests/heartbeat.rs`) duk suna aiki. **Babu backend work da ake bukata.** Abin da ya ɓace guda biyu ne, dukkansu frontend: (a) `MonitorPayload`/`Monitor` struct a `api/monitoring.rs` (Path 1) ba su da filayen `heartbeat_interval_seconds`/`heartbeat_grace_period_seconds`, (b) `monitor_form.rs` (Path 5) yana da sako na "not yet supported" a maimakon fields na gaske. |
| 4 | Design System (Path 2) — "babban dogaro, na iya jinkirtawa" | **AN GAMA SHI TUNI.** `components/` yana da fayiloli 16 gama-gari (button, table_shell, metric_card, status_badge, page_header, empty_state, error_state, confirm_dialog, search_input, filter_bar, layout, card, loading, require_auth, error_boundary, filter_bar). `style.css` yana da sashen "Path 2 additions" cikakke, har da `.data-table` convention na generic. Ana amfani da `TableShell` tuni a `monitor_detail.rs` (Path 5). **Sai dai**: `pages/rum_dashboard.rs` (duba #5 ƙasa) BAI amfani da `TableShell` ba tukuna — yana da raw `<table>` elements guda 6 daban-daban. |
| 5 | cargo check/clippy target — "bai bayyana ba" | **AN WARWARE.** Duba §0 a sama. |

---

## 2. SABUWAR MATSALA da aka Gano (BA TA KASANCE a Execution Plan na Asali)

### File Ownership Gap: `pages/rum_dashboard.rs`

**Wannan fayil BA A BA WA KOWANE PATH BA** a cikin tsohon plan (Paths 3, 4, 5
kawai aka bayar wa Batch 2 don pages/). Amma `pages/rum_dashboard.rs` yana
amfani KAI-TSAYE da `api/rum.rs` (Path 1's contract, mai ɗauke da Legacy
structs 9). Idan Path 1 ya sake rubuta `api/rum.rs` (misali: ya cire
`Legacy*` structs domin gyara jere-zuwa-layi-ɗaya rewrite na Risk 1),
`rum_dashboard.rs` zai KASA `cargo check` sai an canza shi lokaci guda —
babu hanyar rabuwa da wannan dogaro.

**Hukunci**: An ƙara `pages/rum_dashboard.rs` zuwa mallakar **Path 1**.

---

## 3. SABON Execution Plan (na Ƙarshe)

### Batch 1 (Independent — workers 2, a lokaci guda)

- **Path 1** (faɗaɗa daga na asali):
  - `api/monitoring.rs` — ƙara `recent: Vec<IncidentSummary>` zuwa `Overview`
    struct; ƙara `heartbeat_interval_seconds`/`heartbeat_grace_period_seconds`
    zuwa `Monitor`/`MonitorPayload` structs
  - `api/rum.rs` — cire `Legacy*` structs 9, mayar da functions typed 9
    zuwa amfani kai-tsaye
  - `auth/api.rs` (fanko a yanzu — babu aiki na musamman)
  - **`pages/rum_dashboard.rs`** (SABO — an ƙara saboda dogaro da `api/rum.rs`)
    — sake rubuta domin kiran functions 9 a LAYI ƊAYA (parallel, misali ta
    hanyar `futures::join!` ko makamancinsa), share `Legacy*` mapping, kuma
    (idan lokaci ya bari) amfani da `TableShell` component maimakon raw
    `<table>` guda 6

- **Path 2**: `components/`, `style.css` — an gama; a duba kawai idan akwai
  polish/consistency fix (misali: babu wani abu da za a kara)

### Batch 2 (DOLE Batch 1 ya gama duka)

- **Path 3**: `pages/dashboard_overview.rs` — canza "Active Incidents" widget
  daga aggregate count kaɗai zuwa cikakken list, ta amfani da sabon
  `overview.recent` field daga Path 1
- **Path 4**: `pages/monitors.rs` — babu canji daga tsohon plan
- **Path 5**: `pages/monitor_detail.rs`, `pages/monitor_form.rs` — gina
  heartbeat-specific form fields, ta amfani da sabbin filayen da Path 1
  ya ƙara zuwa `Monitor`/`MonitorPayload`

### Solo

Babu — babu shared central file (`routes/mod.rs`) da aka canza a wannan phase.

---

## 4. Sauran Buɗaɗɗun Abubuwa (za a lura da su, ba blockers ba)

- **CI `--workspace` flag** (§0): za a bukaci a duba kafin PR na ƙarshe ya
  wuce CI, amma ba ya hana aiki na gida.
- **RUM page → TableShell migration** (Risk 4 gap): ba dole ba ne domin
  Batch 2 ta fara, amma yakamata a rufe shi a matsayin ɓangare na Path 1's
  RUM rewrite domin consistency.

---

## 5. Takaitawa ga Orchestrator

Babban canji daga plan na asali: **Risks 2, 3, 4 ba blockers ba ne kwata-kwata**
— sun riga sun warware ko kuma ƙananan aiki ne na frontend kawai, BABU
bukatar sabon backend endpoint ko backend work. Risk 1 (RUM) na gaske ne
amma girmansa ya bambanta da yadda aka bayyana tun farko. An gano sabon
dogaro guda ɗaya (Path 1 ↔ `pages/rum_dashboard.rs`) wanda dole a warware
kafin Batch 1 ta fara, don guje wa merge conflict/build failure.
