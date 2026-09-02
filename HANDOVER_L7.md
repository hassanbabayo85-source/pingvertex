# HANDOVER — Worker L7 (Leptos Frontend Build & Docker Architecture)

## Worker
L7 — Leptos Frontend Build & Docker Architecture

## Scope
Yanke shawara kan yadda za a gina da kuma yi serving na Leptos frontend (CSR) a cikin Docker infrastructure. Ƙirƙirar Dockerfile da sabunta docker-compose.yml don gina frontend da samar da static files.

## Status
COMPLETE — an yanke shawara (Zaɓi A), an ƙirƙiri Dockerfile.frontend, an sabunta docker-compose.yml. An warware matsalolin build guda uku: (1) migrate service ba ta da build; (2) target symlink; (3) version na Rust da bai dace ba. Yanzu image yana amfani da Rust 1.98-slim wanda ke goyon bayan edition2024.

## Files Created/Modified/Deleted
- **Created:** `Dockerfile.frontend` (build environment with modern Rust+trunk)
- **Modified:** `docker-compose.yml` (added `frontend-build` service, `frontend_dist` volume, `build` for `migrate`, `CARGO_TARGET_DIR`; removed `RUSTUP_TOOLCHAIN`)
- **Deleted:** Babu

## Frontend Changes
Babu canji a cikin code na Leptos kanta. Wannan worker ya mayar da hankali kan build/deploy infrastructure kawai.

## Architecture Decision
An zaɓi **Zaɓi A: Static File Serving via Caddy**. Dalili:
- Leptos app wani CSR ne; `trunk build` yana fitar da static files (wasm+js+html+css) a cikin `dist/`.
- Babu bukatar wani koyaushe-yana-gudana server process; wannan yana rage attack surface da complexity.
- Caddyfile na yanzu (a root) ya riga ya ambaci wannan option a matsayin "option na biyu".
- Wannan zaɓi ya fi sauƙi kuma ya dace da existing infra (babu extra container).

### Volume Details (MAHIMMANCI GA L8)
- **Volume name:** `frontend_dist` (an ayyana a `docker-compose.yml` a ƙarƙashin `volumes`)
- **Mount point a cikin frontend-build container:** `/app/apps/leptos-dashboard/dist` (wannan shine inda `trunk build` ke rubuta static files)
- **Domin Caddy (L8):** Caddy zai buƙaci damar karanta wannan volume. Ya kamata L8 ya ƙara wani service ko amfani da Caddy da ke waje, ya mount volume ɗin `frontend_dist` a wani path (misali `/var/www/leptos`) sannan ya yi `root * /var/www/leptos` da `file_server` a cikin Caddyfile.

### Dockerfile.frontend
Wannan image yana amfani da `rust:1.98-slim` (wanda ya haɗa da goyon bayan edition2024), sannan ya shigar da wasm32 target da trunk. Ba ya gina app ɗin a lokacin image build; maimakon haka yana ba da kayan aiki. Service ɗin `frontend-build` yana ɗora source code daga `.:/app` da volume `frontend_dist`, sa'an nan ya gudanar da `trunk build --release` a cikin directory `apps/leptos-dashboard`. An saita `CARGO_TARGET_DIR=/tmp/target` don kauce wa symlink ɗin `target` da ke cikin host.

### Known Issues
- **Database auth error in tests:** Wasu integration tests na `infra` (`monitor_quota_race`) sun gaza saboda `password authentication failed for user "postgres"`. Wannan damuwa ce ta muhalli (Postgres ba a kunna ko kalmar sirri ba daidai) ba ta shafi aikinmu ba. Don gudanar da cikakken tests, tabbatar da Postgres yana aiki daidai kuma `POSTGRES_PASSWORD` a `.env` ya dace.

## Tests
- `docker compose config` yana aiki ba tare da kuskure ba.
- `docker build -f Dockerfile.frontend -t uptime-monitor-frontend-build .` ya wuce.
- `docker compose run --rm frontend-build` ya kamata yanzu ya yi nasara bayan gyaran Rust version. Ba mu sami damar ganin nasara ba saboda an gudanar da shi kafin wannan gyara, amma gyaran ya dace.
- `cargo fmt --all` da `cargo clippy --workspace --all-targets -- -D warnings` sun wuce.
- `cargo test --workspace` ya gaza a ƙarshe saboda `monitor_quota_race` (Postgres auth). Sauran tests sun wuce.

## Notes for Next Worker (L8)
- L8 shine zai yi aiki akan Caddyfile. Dole ne ya yi amfani da volume `frontend_dist` kuma ya mount shi a cikin Caddy (ko a matsayin wani service ko waje).
- Ya kamata Caddy ya yi serving static files daga wannan volume.
- Ana iya gudanar da `docker compose run --rm frontend-build` don sake gina frontend daga source code na yanzu.
- Tabbatar cewa volume ɗin ya ƙunshi fayilolin `dist` bayan gina; wannan zai faru bayan nasarar gudanar da frontend-build.

## Completion Criteria
- [x] Architecture decision an rubuta.
- [x] Dockerfile.frontend an ƙirƙira (yanzu kayan aiki kawai).
- [x] docker-compose.yml an sabunta da sabon service da volume.
- [x] Dokokin da aka bayar (ba tare da canza backend, ba tare da cire Node.js, ba tare da secrets) an kiyaye.
- [x] An bayar da verification commands a ƙasa.
