//! Compile-fail tests for the typed permission system (M07 Stage A). Each
//! `tests/ui/*.rs` file must fail to compile with the matching `*.stderr`.
//! Regenerate the expected output with `TRYBUILD=overwrite cargo test -p
//! hello-plugin --test ui` after an intentional change (pinned to rustc 1.88).

#[test]
fn permission_compile_fail() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
