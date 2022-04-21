#[test]
fn tests() {
    let t = trybuild::TestCases::new();
    //FIXME: figure out how we're going to do variable sized arrays
    //t.pass("tests/smoke-test.rs");
    t.compile_fail("tests/must-include-lifetime.rs");
    t.compile_fail("tests/single-lifetime-only.rs");
    t.compile_fail("tests/bad-attrs.rs");
    t.pass("tests/enums.rs");
    t.compile_fail("tests/enum-bad-attrs.rs");
    t.compile_fail("tests/count-all-guard.rs");
    t.pass("tests/offset_host.rs");
    t.pass("tests/bitflags.rs");
    t.compile_fail("tests/enum-across-decls.rs");
}
