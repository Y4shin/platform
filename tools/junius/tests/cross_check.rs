//! Integration tests for the M09 cross-plugin `junius check` rules. Each builds a
//! tiny repo with a producer (`prov`) and a consumer (`cons`), runs
//! `junius check --manifest platform.toml --format json`, and asserts the
//! expected rule fires (or, for the clean fixture, that none do).

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::fs;
use std::path::Path;

use common::{cmd_in, make_repo, tempdir};
use predicates::prelude::*;

/// Write the producer plugin `prov`: a `prov` schema/table, a `Widget` component,
/// and a `ProvService.DoThing` RPC.
fn write_prov(repo: &Path) {
    let dir = repo.join("plugins/prov");
    fs::write(
        dir.join("plugin.toml"),
        "[plugin]\nname = \"prov\"\ndisplay_name = \"Prov\"\nmanifest_schema = 1\n\
         [exposes.tables.thing]\nschema = \"prov\"\n\
         [exposes.components.Widget]\nmodule = \"./lib/Widget\"\n",
    )
    .unwrap();
    let proto = dir.join("proto/prov/v1");
    fs::create_dir_all(&proto).unwrap();
    fs::write(
        proto.join("prov.proto"),
        "syntax = \"proto3\";\npackage prov.v1;\n\
         service ProvService {\n  rpc DoThing(M) returns (M);\n}\nmessage M {}\n",
    )
    .unwrap();
    let migrations = dir.join("migrations");
    fs::create_dir_all(&migrations).unwrap();
    fs::write(
        migrations.join("0001_thing.up.sql"),
        "CREATE SCHEMA prov;\nCREATE TABLE prov.thing (id UUID PRIMARY KEY);\n",
    )
    .unwrap();
}

/// Overwrite `cons`'s `plugin.toml`.
fn cons_manifest(repo: &Path, body: &str) {
    fs::write(
        repo.join("plugins/cons/plugin.toml"),
        format!("[plugin]\nname = \"cons\"\ndisplay_name = \"Cons\"\nmanifest_schema = 1\n{body}"),
    )
    .unwrap();
}

/// Write a `cons` migration.
fn cons_migration(repo: &Path, sql: &str) {
    let dir = repo.join("plugins/cons/migrations");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("0001_x.up.sql"), sql).unwrap();
}

/// Write a `cons` frontend source file.
fn cons_frontend(repo: &Path, ts: &str) {
    let dir = repo.join("plugins/cons/frontend/src");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("page.tsx"), ts).unwrap();
}

fn check(repo: &Path) -> assert_cmd::assert::Assert {
    cmd_in(repo)
        .args(["check", "--manifest", "platform.toml", "--format", "json"])
        .assert()
}

#[test]
fn dep_undeclared_flags_undeclared_plugin_import() {
    let tmp = tempdir();
    make_repo(tmp.path(), &["prov", "cons"]);
    write_prov(tmp.path());
    // cons declares NO dependency on prov, but imports its component.
    cons_frontend(
        tmp.path(),
        "import { Widget } from '@junius/plugin-prov';\nexport const W = Widget;\n",
    );

    check(tmp.path())
        .failure()
        .stdout(predicate::str::contains("DEP.UNDECLARED"));
}

#[test]
fn rpc_undeclared_flags_nonexistent_method() {
    let tmp = tempdir();
    make_repo(tmp.path(), &["prov", "cons"]);
    write_prov(tmp.path());
    // prov is declared, but rpc_methods names a method that doesn't exist.
    cons_manifest(
        tmp.path(),
        "[dependencies.prov]\nrpc_methods = [\"ProvService.Missing\"]\n",
    );

    check(tmp.path())
        .failure()
        .stdout(predicate::str::contains("RPC.UNDECLARED"));
}

#[test]
fn sql_requires_missing_flags_cross_schema_fk_without_requires() {
    let tmp = tempdir();
    make_repo(tmp.path(), &["prov", "cons"]);
    write_prov(tmp.path());
    cons_manifest(tmp.path(), "[dependencies.prov]\n");
    // FK into prov.thing but no `-- @requires prov:...`.
    cons_migration(
        tmp.path(),
        "CREATE SCHEMA cons;\nCREATE TABLE cons.x (\n  id UUID PRIMARY KEY,\n  t UUID NOT NULL REFERENCES prov.thing (id)\n);\n",
    );

    check(tmp.path())
        .failure()
        .stdout(predicate::str::contains("SQL.REQUIRES.MISSING"));
}

#[test]
fn fk_cross_cascade_flags_cascade_across_plugins() {
    let tmp = tempdir();
    make_repo(tmp.path(), &["prov", "cons"]);
    write_prov(tmp.path());
    cons_manifest(tmp.path(), "[dependencies.prov]\n");
    cons_migration(
        tmp.path(),
        "-- @requires prov:0001_thing\nCREATE SCHEMA cons;\nCREATE TABLE cons.x (\n  id UUID PRIMARY KEY,\n  t UUID NOT NULL REFERENCES prov.thing (id) ON DELETE CASCADE\n);\n",
    );

    check(tmp.path())
        .failure()
        .stdout(predicate::str::contains("FK.CROSS.CASCADE"));
}

#[test]
fn fk_optional_nullable_flags_not_null_fk_into_optional_dep() {
    let tmp = tempdir();
    make_repo(tmp.path(), &["prov", "cons"]);
    write_prov(tmp.path());
    cons_manifest(tmp.path(), "[dependencies.prov]\noptional = true\n");
    cons_migration(
        tmp.path(),
        "-- @requires prov:0001_thing\nCREATE SCHEMA cons;\nCREATE TABLE cons.x (\n  id UUID PRIMARY KEY,\n  t UUID NOT NULL REFERENCES prov.thing (id)\n);\n",
    );

    check(tmp.path())
        .failure()
        .stdout(predicate::str::contains("FK.OPTIONAL.NULLABLE"));
}

#[test]
fn clean_cross_plugin_setup_passes() {
    let tmp = tempdir();
    make_repo(tmp.path(), &["prov", "cons"]);
    write_prov(tmp.path());
    // Required dep, declared, valid RPC, FK with @requires, no cascade, NOT NULL
    // ok (required dep), import declared.
    cons_manifest(
        tmp.path(),
        "[dependencies.prov]\nrpc_methods = [\"ProvService.DoThing\"]\n",
    );
    cons_frontend(
        tmp.path(),
        "import { Widget } from '@junius/plugin-prov';\nexport const W = Widget;\n",
    );
    cons_migration(
        tmp.path(),
        "-- @requires prov:0001_thing\nCREATE SCHEMA cons;\nCREATE TABLE cons.x (\n  id UUID PRIMARY KEY,\n  t UUID NOT NULL REFERENCES prov.thing (id)\n);\n",
    );

    check(tmp.path()).success().stdout(
        predicate::str::contains("\"ok\": true").or(predicate::str::contains("\"ok\":true")),
    );
}
