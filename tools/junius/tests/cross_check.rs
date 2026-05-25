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
    // The exposed `Widget` component must be a named export of index.ts
    // (FE.EXPORTS.MATCH_MANIFEST).
    let fe = dir.join("frontend/src");
    fs::create_dir_all(&fe).unwrap();
    fs::write(
        fe.join("index.ts"),
        "export { Widget } from './lib/Widget.js';\n",
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

/// Write `cons`'s `frontend/src/index.ts` (the exposed-component barrel).
fn cons_frontend_index(repo: &Path, ts: &str) {
    let dir = repo.join("plugins/cons/frontend/src");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("index.ts"), ts).unwrap();
}

/// Write a `cons` Rust source file (scanned for `sqlx::query*!` table refs).
fn cons_rust(repo: &Path, rs: &str) {
    let dir = repo.join("plugins/cons/src");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("repo.rs"), rs).unwrap();
}

/// Write `plugins/<plugin>/migrations/<file>` with `sql`.
fn write_migration(repo: &Path, plugin: &str, file: &str, sql: &str) {
    let dir = repo.join(format!("plugins/{plugin}/migrations"));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(file), sql).unwrap();
}

fn git(repo: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args(args)
        .current_dir(repo)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed");
}

/// Init a git repo on branch `main` and commit the current tree as the baseline.
fn git_init_commit(repo: &Path) {
    git(repo, &["init", "-q", "-b", "main"]);
    git(repo, &["add", "."]);
    git(
        repo,
        &[
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-q",
            "-m",
            "baseline",
        ],
    );
}

fn check_base(repo: &Path, base: &str) -> assert_cmd::assert::Assert {
    cmd_in(repo)
        .args([
            "check",
            "--manifest",
            "platform.toml",
            "--format",
            "json",
            "--base",
            base,
        ])
        .assert()
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
        "[dependencies.prov]\ntables = [\"thing\"]\nrpc_methods = [\"ProvService.DoThing\"]\n",
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

#[test]
fn private_table_access_flags_undeclared_exposed_table_in_query() {
    let tmp = tempdir();
    make_repo(tmp.path(), &["prov", "cons"]);
    write_prov(tmp.path());
    // prov.thing IS exposed, but cons doesn't list it in [dependencies.prov].tables.
    cons_manifest(tmp.path(), "[dependencies.prov]\n");
    cons_rust(
        tmp.path(),
        "fn f() { let _ = sqlx::query!(\"SELECT id FROM prov.thing\"); }\n",
    );

    check(tmp.path())
        .failure()
        .stdout(predicate::str::contains("SQL.PRIVATE_TABLE_ACCESS"));
}

#[test]
fn private_table_access_flags_unexposed_table_in_migration() {
    let tmp = tempdir();
    make_repo(tmp.path(), &["prov", "cons"]);
    write_prov(tmp.path());
    // prov.secret is not in prov's [exposes.tables], even though cons lists it.
    cons_manifest(tmp.path(), "[dependencies.prov]\ntables = [\"secret\"]\n");
    cons_migration(
        tmp.path(),
        "-- @requires prov:0001_thing\nCREATE SCHEMA cons;\nCREATE TABLE cons.x (\n  id UUID PRIMARY KEY,\n  s UUID REFERENCES prov.secret (id)\n);\n",
    );

    check(tmp.path())
        .failure()
        .stdout(predicate::str::contains("SQL.PRIVATE_TABLE_ACCESS"));
}

#[test]
fn private_table_access_passes_for_exposed_and_declared_table() {
    let tmp = tempdir();
    make_repo(tmp.path(), &["prov", "cons"]);
    write_prov(tmp.path());
    cons_manifest(tmp.path(), "[dependencies.prov]\ntables = [\"thing\"]\n");
    // A raw, multiline query referencing the exposed+declared table.
    cons_rust(
        tmp.path(),
        "fn f() {\n  let _ = sqlx::query_scalar!(\n    r#\"SELECT count(*)\n       FROM prov.thing\"#\n  );\n}\n",
    );

    check(tmp.path()).success().stdout(
        predicate::str::contains("\"ok\": true").or(predicate::str::contains("\"ok\":true")),
    );
}

#[test]
fn exposed_no_breaking_flags_uncoordinated_breaking_change() {
    let tmp = tempdir();
    make_repo(tmp.path(), &["prov", "cons"]);
    write_prov(tmp.path());
    cons_manifest(tmp.path(), "[dependencies.prov]\ntables = [\"thing\"]\n");
    git_init_commit(tmp.path());
    // A new (untracked) breaking change to prov's exposed table; cons declares it
    // and ships no coordinating migration.
    write_migration(
        tmp.path(),
        "prov",
        "0002_drop.up.sql",
        "ALTER TABLE prov.thing DROP COLUMN id;\n",
    );

    check_base(tmp.path(), "main")
        .failure()
        .stdout(predicate::str::contains("SQL.EXPOSED.NO_BREAKING"));
}

#[test]
fn exposed_no_breaking_allows_coordinated_change() {
    let tmp = tempdir();
    make_repo(tmp.path(), &["prov", "cons"]);
    write_prov(tmp.path());
    cons_manifest(tmp.path(), "[dependencies.prov]\ntables = [\"thing\"]\n");
    git_init_commit(tmp.path());
    write_migration(
        tmp.path(),
        "prov",
        "0002_drop.up.sql",
        "ALTER TABLE prov.thing DROP COLUMN id;\n",
    );
    // cons coordinates with a migration in the same change set.
    write_migration(tmp.path(), "cons", "0002_adapt.up.sql", "SELECT 1;\n");

    check_base(tmp.path(), "main").success();
}

#[test]
fn exposed_no_breaking_is_noop_without_git() {
    let tmp = tempdir();
    make_repo(tmp.path(), &["prov", "cons"]);
    write_prov(tmp.path());
    cons_manifest(tmp.path(), "[dependencies.prov]\ntables = [\"thing\"]\n");
    // No git repo → the rule cannot diff and must no-op (so the breaking change
    // is not flagged here).
    write_migration(
        tmp.path(),
        "prov",
        "0002_drop.up.sql",
        "ALTER TABLE prov.thing DROP COLUMN id;\n",
    );

    check_base(tmp.path(), "main").success();
}

#[test]
fn fe_exports_flags_declared_component_not_exported() {
    let tmp = tempdir();
    make_repo(tmp.path(), &["prov", "cons"]);
    write_prov(tmp.path());
    // cons declares a component but its index.ts exports something else.
    cons_manifest(
        tmp.path(),
        "[exposes.components.Card]\nmodule = \"./lib/Card\"\n",
    );
    cons_frontend_index(tmp.path(), "export { Other } from './lib/Other.js';\n");

    check(tmp.path())
        .failure()
        .stdout(predicate::str::contains("FE.EXPORTS.MATCH_MANIFEST"));
}

#[test]
fn fe_exports_passes_when_component_is_exported() {
    let tmp = tempdir();
    make_repo(tmp.path(), &["prov", "cons"]);
    write_prov(tmp.path());
    cons_manifest(
        tmp.path(),
        "[exposes.components.Card]\nmodule = \"./lib/Card\"\n",
    );
    // Multiline export list with an alias and a type export to exercise the scan.
    cons_frontend_index(
        tmp.path(),
        "export type { CardProps } from './lib/Card.js';\nexport {\n  Inner as Card,\n  helper,\n} from './lib/Card.js';\n",
    );

    check(tmp.path()).success().stdout(
        predicate::str::contains("\"ok\": true").or(predicate::str::contains("\"ok\":true")),
    );
}
