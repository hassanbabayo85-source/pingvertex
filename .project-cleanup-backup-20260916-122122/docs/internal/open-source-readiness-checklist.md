# PingVertex — Jerin Aikin Maida Aikin Open Source

Wannan fayil ɗin naka ne kaɗai (kada ka tura shi GitHub). Cire shi bayan ka gama.

---

## 1. Abubuwan da DOLE ne a gyara kafin a buɗe repo

- [ ] **`README.md` ba komai a ciki (0 bytes).** Yi amfani da wanda aka rubuta maka. Sauya kowane `OWNER` da ainihin sunan GitHub ɗinka.
- [ ] **`LICENSE` ba komai a ciki (0 bytes).** `Cargo.toml` na cewa MIT amma fayil ɗin babu komai — a doka, hakan yana nufin *babu izini ga kowa*. Sanya fayil ɗin MIT da aka shirya maka.
- [ ] **`.cargo/config.toml` yana ɗauke da hanyar kwamfutarka.**
  `target-dir = "/run/media/abubakar-hassan-babayo/2e9125.../cargo-target"`
  Wannan zai sa ginin kowane mai ba da gudummawa ya gaza, kuma yana fallasa sunan mai amfani da ID ɗin na'urarka. Yi amfani da sabon fayil ɗin da aka shirya.
- [ ] **`DATABASE_URL` a cikin `[env]` na `.cargo/config.toml`** — cire shi; a saka shi a matsayin environment variable a `CONTRIBUTING.md`.
- [ ] **Duba tarihin git (git history).** Repo ɗin yana kan GitHub tun kafin yanzu, don haka ko da yanzu babu sirri a cikin fayilolin, yana yiwuwa akwai a cikin tsofaffin commits:
  ```bash
  pipx install detect-secrets && detect-secrets scan --all-files
  # ko:
  docker run --rm -v "$PWD:/repo" zricethezav/gitleaks:latest detect -s /repo -v
  ```
  Idan an sami wani abu: yi amfani da `git filter-repo` ka goge shi, **sannan ka canza duk kalmomin sirrin da aka fallasa** (fallasa ta riga ta faru).

## 2. Docs — tsaftacewa

Waɗannan suna sa repo ya yi kama da wurin aiki na ciki, ba aikin open source ba:

- [ ] **`HANDOVER.md`** — bayanin mika aiki na ciki ("Worker: L9", "Worker 4"). Ba shi da ma'ana ga baƙi. Matsar zuwa `docs/internal/` ko ka goge shi gaba ɗaya.
- [ ] **`docs/GAP_*_CLOSURE.md`, `docs/PAYG_GAP_1_CLOSURE.md`, `docs/WORKER4_PAYG.md`, `docs/FRONTEND_BACKEND_CONTRACT_AUDIT_2026-09-03.md`** — takardun aiki na ciki. Ka matsar da su zuwa `docs/internal/` ko ka haɗa muhimman abubuwan cikin `docs/ARCHITECTURE.md`.
- [ ] **`docs/ROADMAP.md` babu komai a ciki.** Ko ka cike shi (mutane suna duba roadmap kafin su ba da gudummawa) ko ka goge shi.
- [ ] **ADR guda biyu suna da lamba `0001`** (`0001-rust.md` da `0001-feature-based-architecture.md`). Sake yi wa ɗaya lamba.
- [ ] **Ƙara `docs/README.md`** — ɗan ƙaramin index na abin da ke cikin kowane fayil.

Abin da ke da kyau kuma ya kamata ya zauna: `docs/ARCHITECTURE.md`, `docs/VISION.md`, `docs/adr/`, `SECURITY_HARDENING.md`, `docs/ARCHITECTURE_DECISIONS.md`, `sdk/rum/README.md`.

## 3. Harshe a cikin lambar

- [ ] `.gitignore` da `.repomixignore` suna da sharhi cikin Hausa ("Babban abu da ba a bukata", "Stray files da suka taɓa fitowa", "asalin dalilin cleanup ɗin da muka yi"). Ba laifi ba ne, amma ga aikin ƙasa da ƙasa ya fi kyau a canza zuwa Turanci. (Hausa a cikin README na daban — duba §7.)
- [ ] Bincika sunayen docs: `GAP_3_KASHI_A_CLOSURE.md` da sauransu suna amfani da "KASHI" — sake suna idan sun rage.

## 4. Fayilolin al'umma (an shirya maka su)

- [ ] `README.md`
- [ ] `LICENSE`
- [ ] `CONTRIBUTING.md`
- [ ] `SECURITY.md`
- [ ] `CODE_OF_CONDUCT.md`
- [ ] `.github/ISSUE_TEMPLATE/bug_report.yml`
- [ ] `.github/ISSUE_TEMPLATE/feature_request.yml`
- [ ] `.github/ISSUE_TEMPLATE/config.yml`
- [ ] `.github/PULL_REQUEST_TEMPLATE.md`
- [ ] `.github/dependabot.yml`
- [ ] `CHANGELOG.md` — ƙara shi da sakin farko (tsarin [Keep a Changelog](https://keepachangelog.com))
- [ ] `.github/CODEOWNERS` — layi ɗaya ya isa: `* @sunanka`

**A cikin duk fayilolin da aka shirya maka, sauya:**
- `OWNER` → sunan GitHub ɗinka
- `security@example.com` → adireshin imel na gaskiya da kake duba
- `conduct@example.com` → haka ma

## 5. Metadata na Cargo

A cikin `[workspace.package]` na `Cargo.toml`, ƙara:

```toml
repository = "https://github.com/OWNER/pingvertex"
homepage = "https://pingvertex.com"
documentation = "https://github.com/OWNER/pingvertex/tree/main/docs"
keywords = ["monitoring", "uptime", "observability", "sre"]
categories = ["web-programming", "command-line-utilities"]
readme = "README.md"
rust-version = "1.85"
```

Sannan a kowane `crates/*/Cargo.toml` ƙara `repository.workspace = true` da `description = "..."`.

- [ ] `apps/leptos-dashboard/Cargo.toml` na amfani da `edition = "2021"` amma sauran workspace ɗin `2024`. Daidaita ko rubuta dalili.

## 6. Saitin GitHub (a cikin gidan yanar gizon)

- [ ] **About:** rubuta bayani ɗan gajere + topics: `rust`, `monitoring`, `uptime`, `observability`, `self-hosted`, `axum`, `leptos`, `postgresql`
- [ ] Kunna **Discussions** (don tambayoyi, don kada issues ya cika da su)
- [ ] Kunna **Private vulnerability reporting** (Settings → Security)
- [ ] Kunna **Dependabot alerts** da **secret scanning**
- [ ] Kariyar branch akan `main`: buƙatar PR, buƙatar CI ya wuce
- [ ] Ƙirƙiri labels: `good first issue`, `help wanted`, `bug`, `enhancement`, `documentation`
- [ ] Yi tag ɗin sakin farko: `git tag -a v0.1.0 -m "First public release" && git push --tags`
- [ ] Ƙara wasu issues masu sauƙi kuma ka yi musu alamar `good first issue` — ba tare da su ba, babu mai ba da gudummawa da zai fara

## 7. README na Hausa (ƙarin daraja)

Babban README ya kasance Turanci don isa ga duniya. Amma ƙara `README.ha.md` da hanya a saman babban README:

```md
*[Hausa](README.ha.md)*
```

Wannan ba kasafai ake ganin sa ba kuma yana sa aikin ya fito.

## 8. Tunani kan lasisi — MIT vs AGPL

Wannan shawara ce ta kasuwanci, ba ta fasaha ba, don haka kai ne ka yanke:

**MIT** (abin da `Cargo.toml` ke cewa yanzu) — kowa zai iya ɗauka, ya canza, ya sayar da shi a matsayin SaaS na kansa, ba tare da ya mayar maka da komai ba. Yana sa karɓuwa ta fi sauƙi.

**AGPL-3.0** — duk wanda ya gudanar da canjin da ya yi a matsayin sabis akan yanar gizo dole ya buga lambar sa. Wannan shi ne abin da Grafana, Sentry, da Plausible suka zaɓa saboda dalili ɗaya da ya shafe ka: kana da `billing`, `wallet`, da `pricing` a cikin repo — wato aikin da za a iya yin gogayya da shi a matsayin SaaS.

**Open core** — babban abu MIT/Apache-2.0, sassan kasuwanci (billing, SSO, agents masu yawa) a repo daban mai lasisin kasuwanci.

Idan ka yi niyyar sayar da PingVertex a matsayin sabis wata rana, **AGPL-3.0 ya fi maka aminci**. Ka yanke shawara YANZU — canza lasisi bayan mutane sun ba da gudummawa yana buƙatar izinin kowannensu.

## 9. Bayan buɗewa

- [ ] Rubuta post ɗin sanarwa (dev.to / Hashnode) kan dalilin da ya sa ka gina shi
- [ ] Aika zuwa r/rust, r/selfhosted, Hacker News (Show HN)
- [ ] Ƙara zuwa [awesome-selfhosted](https://github.com/awesome-selfhosted/awesome-selfhosted)
- [ ] Ƙara badge ɗin CI da lasisi a README (an riga an sanya, sai dai gyara `OWNER`)

---

## Taƙaitaccen hukunci

Injin aikin ya riga ya kai matsayin open source: Rust workspace mai tsarin clean architecture, 1,018 fayilolin `.rs`, 124 migrations, CI da `cargo audit` + `cargo deny`, `.env.example` mai cikakken bayani, `.gitignore` mai ƙarfi, Docker Compose na dev da na production daban, tsaron SSRF, da mTLS. Wannan ya fi yawancin repos ɗin open source ƙarfi.

Abin da ya rage shi ne **rufin gida kawai** — README, LICENSE, da fayilolin al'umma. Aiki na kwana ɗaya ko biyu, ba fiye ba.
