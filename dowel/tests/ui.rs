//! UI tests pinning the derive's behavior. The missing-dependency case is the
//! load-bearing one: its compile error is the repair signal documented in
//! CLAUDE.md, so its stderr is checked exactly.

#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/pass.rs");
    t.pass("tests/ui/skip.rs");
    t.pass("tests/ui/default.rs");
    t.pass("tests/ui/with.rs");
    t.pass("tests/ui/tuple.rs");
    t.pass("tests/ui/array.rs");
    t.pass("tests/ui/context.rs");
    t.compile_fail("tests/ui/missing_dependency.rs");
    t.compile_fail("tests/ui/on_enum.rs");
    t.compile_fail("tests/ui/context_dup.rs");
}
