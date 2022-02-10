#[test]
fn tests() {
    let t = trybuild::TestCases::new();
    t.pass("tests/smoke-test.rs");
    t.compile_fail("tests/must-include-lifetime.rs");
    t.compile_fail("tests/single-lifetime-only.rs");
}
