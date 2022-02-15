#[test]
fn tests() {
    let t = trybuild::TestCases::new();
    t.pass("tests/smoke-test.rs");
    t.compile_fail("tests/must-include-lifetime.rs");
    t.compile_fail("tests/single-lifetime-only.rs");
    t.compile_fail("tests/bad-attrs.rs");
    t.pass("tests/enums.rs");
    t.compile_fail("tests/enum-bad-attrs.rs");
    t.compile_fail("tests/count-all-guard.rs");
}
