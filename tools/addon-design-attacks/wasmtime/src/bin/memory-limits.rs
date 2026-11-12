//! Findings 1 and 9: `StoreLimits::memory_size` caps each linear memory on its own, and it
//! does not count tables. Every case runs one component in one store with a 64 MiB limit.
//!
//!   memory-limits multi-instance <n>   n core instances, each with a 62.5 MiB memory it touches
//!   memory-limits one-big-memory       one core instance with a 68.75 MiB memory (the control)
//!   memory-limits grow-1gib            one memory.grow of 1 GiB
//!   memory-limits table-grow <n>       one table.grow of n funcref elements
//!   memory-limits table-bomb <n>       n tables of 30,000,000 funcref elements each, at instantiation

use attack_wasmtime::rss_mib;
use wasmtime::component::{Component, Linker};
use wasmtime::{Engine, Result, Store, StoreLimits, StoreLimitsBuilder};

const LIMIT_BYTES: usize = 64 << 20;
const PAGE: u64 = 65536;

fn store(engine: &Engine) -> Store<StoreLimits> {
    let mut s = Store::new(engine, StoreLimitsBuilder::new().memory_size(LIMIT_BYTES).build());
    s.limiter(|limits| limits);
    s
}

/// A core module with one memory of `pages` pages. Its start function writes one byte to
/// every 4 KiB page, so the memory is resident.
fn touching_module(pages: u64) -> String {
    let end = pages * PAGE;
    format!(
        r#"(core module $m
             (memory {pages})
             (func $touch (local $a i32)
               (loop $l
                 (i32.store8 (local.get $a) (i32.const 1))
                 (local.set $a (i32.add (local.get $a) (i32.const 4096)))
                 (br_if $l (i32.lt_u (local.get $a) (i32.const {end})))))
             (start $touch))"#
    )
}

fn instantiate(engine: &Engine, wat: &str) -> (Store<StoreLimits>, Result<()>, u64, u64) {
    let component = Component::new(engine, wat).expect("component compiles");
    let linker: Linker<StoreLimits> = Linker::new(engine);
    let mut s = store(engine);
    let before = rss_mib();
    let r = linker.instantiate(&mut s, &component).map(|_| ());
    let after = rss_mib();
    (s, r, before, after)
}

fn show(r: &Result<()>) -> String {
    match r {
        Ok(()) => "instantiate OK".to_string(),
        Err(e) => format!("instantiate refused: {}", e.root_cause()),
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let engine = Engine::default();
    let n: u64 = args.get(2).and_then(|a| a.parse().ok()).unwrap_or(1);
    match args.get(1).map(String::as_str) {
        Some("multi-instance") => {
            let mut wat = format!("(component {}", touching_module(1000));
            for _ in 0..n {
                wat.push_str(" (core instance (instantiate $m))");
            }
            wat.push(')');
            let (_store, r, before, after) = instantiate(&engine, &wat);
            println!(
                "multi-instance: {n} core instances x 62.5 MiB under a 64 MiB StoreLimits: {}; RSS {before} MiB -> {after} MiB (+{} MiB held by one store)",
                show(&r),
                after.saturating_sub(before)
            );
        }
        Some("one-big-memory") => {
            let wat = format!("(component {} (core instance (instantiate $m)))", touching_module(1100));
            let (_store, r, before, after) = instantiate(&engine, &wat);
            println!(
                "one-big-memory: 1 core instance x 68.75 MiB under a 64 MiB StoreLimits: {}; RSS {before} MiB -> {after} MiB",
                show(&r)
            );
        }
        Some("grow-1gib") => {
            let wat = r#"(component
                (core module $m
                  (memory 1)
                  (func (export "grow") (param i32) (result i32) (memory.grow (local.get 0))))
                (core instance $i (instantiate $m))
                (func (export "grow") (param "pages" u32) (result s32) (canon lift (core func $i "grow"))))"#;
            let component = Component::new(&engine, wat)?;
            let linker: Linker<StoreLimits> = Linker::new(&engine);
            let mut s = store(&engine);
            let instance = linker.instantiate(&mut s, &component)?;
            let grow = instance.get_typed_func::<(u32,), (i32,)>(&mut s, "grow")?;
            let (r,) = grow.call(&mut s, (16384,))?;
            println!("grow-1gib: one memory.grow of 16384 pages (1 GiB) under a 64 MiB StoreLimits returned {r}");
        }
        Some("table-grow") => {
            let wat = r#"(component
                (core module $m
                  (table 1 funcref)
                  (func (export "grow") (param i32) (result i32) (table.grow (ref.null func) (local.get 0))))
                (core instance $i (instantiate $m))
                (func (export "grow") (param "n" u32) (result s32) (canon lift (core func $i "grow"))))"#;
            let component = Component::new(&engine, wat)?;
            let linker: Linker<StoreLimits> = Linker::new(&engine);
            let mut s = store(&engine);
            let instance = linker.instantiate(&mut s, &component)?;
            let grow = instance.get_typed_func::<(u32,), (i32,)>(&mut s, "grow")?;
            let before = rss_mib();
            let (r,) = grow.call(&mut s, (n as u32,))?;
            let after = rss_mib();
            let how = if r < 0 { "was refused".to_string() } else { format!("grew to {} funcref elements", n + 1) };
            println!(
                "table-grow: table.grow by {n} elements under a 64 MiB StoreLimits {how} (returned {r}); RSS {before} MiB -> {after} MiB (+{} MiB)",
                after.saturating_sub(before)
            );
        }
        Some("table-bomb") => {
            // Two runs. In the first, nothing touches the tables. In the second, a start
            // function fills every element of every table with a null reference.
            for fill in [false, true] {
                let mut tables = String::new();
                let mut start = String::new();
                for t in 0..n {
                    tables.push_str(" (table 30000000 funcref)");
                    if fill {
                        start.push_str(&format!(" (table.fill {t} (i32.const 0) (ref.null func) (i32.const 30000000))"));
                    }
                }
                let start = if fill { format!(" (func $fill {start}) (start $fill)") } else { String::new() };
                let wat = format!("(component (core module $m {tables}{start}) (core instance (instantiate $m)))");
                let (store, r, before, after) = instantiate(&engine, &wat);
                println!(
                    "table-bomb: {n} tables x 30,000,000 funcref under a 64 MiB StoreLimits, {}: {}; RSS {before} MiB -> {after} MiB (+{} MiB)",
                    if fill { "filled with null by a start function" } else { "untouched" },
                    show(&r),
                    after.saturating_sub(before)
                );
                drop(store);
            }
        }
        _ => {
            eprintln!("usage: memory-limits multi-instance <n> | one-big-memory | grow-1gib | table-grow <n> | table-bomb <n>");
            std::process::exit(2);
        }
    }
    Ok(())
}
