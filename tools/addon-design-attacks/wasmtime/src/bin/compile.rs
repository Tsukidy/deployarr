//! Finding 16: compiling a small hostile component can take seconds and gigabytes.
//!
//!   compile <blocks> <locals>
//!
//! The component holds one core module with one function. The function declares <locals>
//! i32 locals and gives each a distinct value from its parameter. It then has <blocks>
//! blocks in a row. Each block may branch out early, and otherwise adds 1 to one local, so
//! every block end merges two values. At the end the function adds up every local, so
//! every local stays live through every block. The probe compiles the component with
//! `Component::new` and the default `Config`, then prints the binary size, the time, and
//! the process's peak resident memory.

use attack_wasmtime::peak_rss_mib;
use std::fmt::Write as _;
use std::time::Instant;
use wasmtime::component::Component;
use wasmtime::{Engine, Result};

fn component_wat(blocks: usize, locals: usize) -> String {
    let mut f = String::with_capacity(blocks * 90 + locals * 60);
    f.push_str("(component (core module (func (export \"f\") (param i32) (result i32)");
    for _ in 0..locals {
        f.push_str(" (local i32)");
    }
    for j in 1..=locals {
        write!(f, " local.get 0 i32.const {j} i32.add local.set {j}").unwrap();
    }
    for i in 0..blocks {
        let x = 1 + i % locals;
        write!(f, " block local.get 0 br_if 0 local.get {x} i32.const 1 i32.add local.set {x} end").unwrap();
    }
    f.push_str(" local.get 1");
    for j in 2..=locals {
        write!(f, " local.get {j} i32.add").unwrap();
    }
    f.push_str(")))");
    f
}

fn main() -> Result<()> {
    let args: Vec<usize> = std::env::args().skip(1).filter_map(|a| a.parse().ok()).collect();
    let (blocks, locals) = match args[..] {
        [b, l] if l > 0 => (b, l),
        _ => {
            eprintln!("usage: compile <blocks> <locals>");
            std::process::exit(2);
        }
    };
    let binary = wat::parse_str(component_wat(blocks, locals))?;
    let engine = Engine::default();
    let t = Instant::now();
    let r = Component::new(&engine, &binary);
    let secs = t.elapsed().as_secs_f64();
    println!(
        "n={blocks} blocks, k={locals} locals: binary {} bytes: compile {} in {secs:.1} s, peak RSS {} MiB",
        binary.len(),
        match r { Ok(_) => "ok".to_string(), Err(e) => format!("failed ({})", e.root_cause()) },
        peak_rss_mib()
    );
    Ok(())
}
