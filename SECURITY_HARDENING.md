# Security hardening applied

## Fixed in this revision

1. **JWT logout revocation**
   - `/api/v1/auth/logout` is now authentication-protected.
   - Logout writes a per-user Redis invalidation marker.
   - Revocation uses `iat < now + 1` because JWT `iat` is second-granularity.
   - Redis failure makes logout fail closed instead of claiming success.
   - Added an end-to-end regression test proving a copied bearer token is rejected after logout.

2. **Production Redis requirement for security-sensitive auth state**
   - API startup now fails in production if `REDIS_URL` is missing.
   - This prevents silent no-op session invalidation/TOTP behavior in production.

3. **Dependency attack-surface reduction**
   - `rust_decimal` now disables default features and enables only `serde`.
   - `sqlx` now disables default features and explicitly enables only PostgreSQL/Rustls functionality.
   - This removes unused optional dependency families such as `rkyv` and `sqlx-mysql` from the resolved production graph after lockfile regeneration.

4. **Dependency advisory policy**
   - Removed all `cargo-audit` advisory ignores.
   - CI now regenerates and requires a committed `Cargo.lock`, then runs `cargo audit` without ignores.

5. **Maintenance-window repository foot-guns**
   - Removed unscoped `find_by_id` and `delete` methods from the maintenance-window repository port and implementations.
   - The remaining deletion primitive is tenant-scoped at the SQL layer.

6. **Build-context/source hygiene**
   - `.dockerignore` excludes historical security backups, generated source aggregates, SBOM files, and local metrics-token material.
   - Historical backup directories and `overview.xml` were removed from this hardened source package.

## Not silently changed

The following remain deployment tasks rather than risky automatic rewrites:

- splitting PostgreSQL runtime and migration users/roles;
- trusted reverse-proxy configuration for client IP/rate limiting;
- adding PostgreSQL Row-Level Security across every tenant-owned table.

These require knowledge of the real production topology and database operational policy. The verification commands below should be run before deployment.
