use deployarr_core::compile::{CompileErrors, Program, compile};
use deployarr_core::sequence::AuthoredSequence;

fn plan(sequence: &AuthoredSequence) -> Result<Program, CompileErrors> {
    compile(sequence)
}

fn main() {
    let _ = plan;
}
