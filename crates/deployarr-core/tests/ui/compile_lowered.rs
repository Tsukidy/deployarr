use deployarr_core::compile::{CompileErrors, Program, compile};
use deployarr_core::sequence::LoweredSequence;

fn plan(sequence: &LoweredSequence) -> Result<Program, CompileErrors> {
    compile(sequence)
}

fn main() {
    let _ = plan;
}
