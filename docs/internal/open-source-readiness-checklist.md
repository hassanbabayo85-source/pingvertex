# PingVertex — Open Source Readiness Checklist

This is an internal working document. Remove it once the work is complete.

---

## 1. Must be fixed before the repository is made public

- [ ] **`README.md` is empty (0 bytes).** Fill it in, and replace every `OWNER` placeholder with the real GitHub account name.
- [ ] **`LICENSE` is empty (0 bytes).** `Cargo.toml` declares MIT, but an empty licence file legally means *nobody has any permission*. Add the full MIT text.
- [ ] **`.cargo/config.toml` contains a machine-specific path.**
  `target-dir = "/run/media/<user>/2e9125.../cargo-target"`
  This breaks the build for every contributor and leaks a local username and device ID. Replace it with a portable configuration.
- [ ] **`DATABASE_URL` in the `[env]` section of `.cargo/config.toml`** — remove it; document it as an environment variable in `CONTRIBUTING.md` instead.
- [ ] **Audit the git history.** The repository has been on GitHub for some time, so even if no secrets remain in the working tree, they may still exist in older commits:
  ```bash
  pipx install detect-secrets && detect-secrets scan --all-files
  # or:
  docker run --rm -v "$PWD:/repo" zricethezav/gitleaks:latest detect -s /repo -v
  ```
  If anything is found: purge it with `git filter-repo`, **then rotate every exposed credential** (the exposure has already happened).

## 2. Documentation cleanup

These make the repository look like an internal workspace rather than an open source project:

- [ ] **`HANDOVER.md`** — an internal handover document. It means nothing to outside contributors. Move it to `docs/internal/` or delete it outright.
- [ ] **The closure documents and contract audit under `docs/`** — internal working notes. Move them to `docs/internal/` or fold the important parts into `docs/ARCHITECTURE.md`.
- [ ] **`docs/ROADMAP.md` is empty.** Either fill it in (people check the roadmap before contributing) or delete it.
- [ ] **Two ADRs share the number `0001`** (`0001-rust.md` and `0001-feature-based-architecture.md`). Renumber one of them.
- [ ] **Add `docs/README.md`** — a short index describing what each file contains.

Worth keeping as-is: `docs/ARCHITECTURE.md`, `docs/VISION.md`, `docs/adr/`, `SECURITY_HARDENING.md`, `docs/ARCHITECTURE_DECISIONS.md`, `sdk/rum/README.md`.

## 3. Language in the codebase

- [ ] `.gitignore` and `.repomixignore` previously carried Hausa comments. Not a problem in itself, but English is a better default for an international project. (A Hausa README is a separate matter — see section 7.)
- [ ] Review document filenames for non-English words and rename any that remain.

## 4. Community files

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
- [ ] `CHANGELOG.md` — seed it with the first release (following [Keep a Changelog](https://keepachangelog.com))
- [ ] `.github/CODEOWNERS` — a single line is enough: `* @your-username`

**In all of these files, replace:**
- `OWNER` with the GitHub account name
- `security@example.com` with a real, monitored email address
- `conduct@example.com` likewise

## 5. Cargo metadata

In `[workspace.package]` in `Cargo.toml`, add:

```toml
repository = "https://github.com/OWNER/pingvertex"
homepage = "https://pingvertex.com"
documentation = "https://github.com/OWNER/pingvertex/tree/main/docs"
keywords = ["monitoring", "uptime", "observability", "sre"]
categories = ["web-programming", "command-line-utilities"]
readme = "README.md"
rust-version = "1.85"
```

Then add `repository.workspace = true` and `description = "..."` to each `crates/*/Cargo.toml`.

- [ ] `apps/leptos-dashboard/Cargo.toml` uses `edition = "2021"` while the rest of the workspace uses `2024`. Align them, or document why not.

## 6. GitHub settings (in the web UI)

- [ ] **About:** write a short description and add topics: `rust`, `monitoring`, `uptime`, `observability`, `self-hosted`, `axum`, `leptos`, `postgresql`
- [ ] Enable **Discussions** (for questions, so they do not fill up the issue tracker)
- [ ] Enable **Private vulnerability reporting** (Settings then Security)
- [ ] Enable **Dependabot alerts** and **secret scanning**
- [ ] Branch protection on `main`: require a PR, require CI to pass
- [ ] Create labels: `good first issue`, `help wanted`, `bug`, `enhancement`, `documentation`
- [ ] Tag the first release: `git tag -a v0.1.0 -m "First public release" && git push --tags`
- [ ] Open a few approachable issues and label them `good first issue` — without them, nobody has an entry point

## 7. A Hausa README (optional extra)

The main README should stay in English so it reaches the widest audience. But consider adding `README.ha.md` with a link at the top of the main README:

```md
*[Hausa](README.ha.md)*
```

This is rarely done and helps the project stand out.

## 8. Licence considerations — MIT vs AGPL

This is a commercial decision rather than a technical one, so it is yours to make:

**MIT** (what `Cargo.toml` currently declares) — anyone can take it, modify it, and sell it as their own SaaS without contributing anything back. It maximises adoption.

**AGPL-3.0** — anyone who runs a modified version as a network service must publish their source. This is what Grafana, Sentry, and Plausible chose, for a reason that applies here too: the repository contains `billing`, `wallet`, and `pricing` — that is, work that could be turned into a directly competing SaaS.

**Open core** — MIT/Apache-2.0 for the core, with commercial components (billing, SSO, multi-agent) in a separate commercially licensed repository.

If you intend to sell PingVertex as a service one day, **AGPL-3.0 is the safer choice**. Decide now — changing the licence after people have contributed requires permission from every contributor.

## 9. After going public

- [ ] Write an announcement post (dev.to / Hashnode) explaining why you built it
- [ ] Share it on r/rust, r/selfhosted, and Hacker News (Show HN)
- [ ] Submit it to [awesome-selfhosted](https://github.com/awesome-selfhosted/awesome-selfhosted)
- [ ] Add CI and licence badges to the README (already prepared — just fix `OWNER`)

---

## Summary

The engineering is already at open source standard: a Rust workspace with a clean architecture, over a thousand `.rs` files, 124 migrations, CI with `cargo audit` and `cargo deny`, a fully documented `.env.example`, a thorough `.gitignore`, separate Docker Compose files for development and production, SSRF protection, and mTLS. That is stronger than most open source repositories.

What remains is the finishing work — README, LICENSE, and the community files. A day or two, not more.
