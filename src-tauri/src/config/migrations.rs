//! WAT-206: forward-compatible settings migrations.
//!
//! Config files carry a `schema_version` field. When a file has a lower
//! version than the current binary, the migrations in this module run in
//! order to bring it up to date. Each migration mutates `Settings` in
//! place and bumps `schema_version` — no separate "migrator" type is
//! needed because the data structure is small and migrations are rare.
//!
//! ## Adding a migration
//!
//! 1. Bump `CURRENT_SCHEMA_VERSION` in `config/mod.rs`.
//! 2. Add a branch below that runs if `settings.schema_version < N` where
//!    N is your target version. Do your mutation, then set
//!    `settings.schema_version = N`.
//! 3. Add a test that seeds a v(N-1) struct, runs `migrate`, and asserts
//!    the v(N) shape.
//!
//! The `migrate` function is idempotent: calling it on an already-current
//! `Settings` is a no-op.

use super::settings::Settings;
use super::CURRENT_SCHEMA_VERSION;

/// Bring `settings` up to the current schema version. Safe to call on any
/// `Settings` value — including one whose `schema_version` is already
/// `CURRENT_SCHEMA_VERSION` (no-op) or one written by a newer Watson (no-op;
/// the caller should have already rejected those via the schema check).
pub fn migrate(settings: &mut Settings) {
    // v0 → v1: the field didn't exist before WAT-206, so serde defaults
    // it to 0. There's no actual data transformation — the v1 layout is
    // identical to the last pre-WAT-206 layout. Bumping the version
    // simply marks the file as "known current".
    if settings.schema_version < 1 {
        settings.schema_version = 1;
    }

    // Future migrations insert here:
    //
    // if settings.schema_version < 2 {
    //     // ... rename a field, split a struct, etc.
    //     settings.schema_version = 2;
    // }

    // Belt-and-suspenders: pin to current even if somehow a version in
    // between the branches was skipped. Guarantees the caller can rely on
    // `settings.schema_version == CURRENT_SCHEMA_VERSION` after migrate.
    if settings.schema_version < CURRENT_SCHEMA_VERSION {
        settings.schema_version = CURRENT_SCHEMA_VERSION;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_bumps_v0_to_current() {
        let mut settings = Settings::default();
        settings.schema_version = 0;
        migrate(&mut settings);
        assert_eq!(settings.schema_version, CURRENT_SCHEMA_VERSION);
    }

    #[test]
    fn migrate_is_noop_on_current_version() {
        let mut settings = Settings::default();
        let before = settings.clone();
        migrate(&mut settings);
        // Byte-for-byte equality through the TOML round-trip is the
        // strongest "no-op" assertion we can make without implementing
        // PartialEq on Settings.
        let a = toml::to_string(&before).unwrap();
        let b = toml::to_string(&settings).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn migrate_does_not_downgrade_a_newer_file() {
        // If a config arrived with a version newer than we understand, we
        // should NOT attempt to "migrate" it downward. The caller is
        // expected to reject the file before calling migrate, but be
        // defensive: if they didn't, we must not silently rewrite the
        // version.
        let mut settings = Settings::default();
        settings.schema_version = CURRENT_SCHEMA_VERSION + 5;
        migrate(&mut settings);
        assert_eq!(
            settings.schema_version,
            CURRENT_SCHEMA_VERSION + 5,
            "migrate must not overwrite a higher version"
        );
    }
}
