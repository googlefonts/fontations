#[test]
fn tests() {
    let t = trybuild::TestCases::new();
    t.pass("tests/01-parse.rs");
    t.pass("tests/02-avar.rs");
    t.pass("tests/03-count-fn.rs");
    t.pass("tests/04-array-all.rs");
    t.pass("tests/05-enum.rs");
}
