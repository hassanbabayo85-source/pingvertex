// Tells Cargo to re-run this crate's build script (and therefore to
// recompile `crates/infra`, re-embedding the migrations) whenever any
// file under `migrations/` changes.
//
// Why this is needed: `sqlx::migrate!("../../migrations")` is a macro
// that reads the migration files at COMPILE time and embeds their
// contents into the binary. Cargo has no idea those `.sql` files are
// inputs to the macro, so without this hint a change to a migration
// leaves `cargo test`/`cargo build` using the OLD embedded copy.
// That is exactly what happened during PART 4: migration 0121 was
// edited, but `cargo test -p infra` kept running the previous version
// (which contained a nested dollar-quote that sqlx's parser skipped),
// so the new triggers never appeared on test schemas.
//
// See: https://doc.rust-lang.org/cargo/reference/build-scripts.html
fn main() {
    println!("cargo:rerun-if-changed=../../migrations");
}
