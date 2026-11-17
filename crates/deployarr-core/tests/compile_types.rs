//! `compile` takes only a lowered sequence, so a program that names an add-on
//! does not type-check.

#[test]
fn compile_takes_only_a_lowered_sequence() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/ui/compile_lowered.rs");
    cases.compile_fail("tests/ui/compile_authored.rs");
}
