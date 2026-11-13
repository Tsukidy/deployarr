//! Finding 14 and three supported claims. Each mode runs the guest from `guest/` with WASI 0.2
//! linked through `wasmtime_wasi::p2::add_to_linker_async` and an empty `WasiCtx`: no
//! preopened folder, no environment, no sockets, and no name lookup. Only the guest's
//! arguments are set, and `ambient` also passes stdout through so the guest can print.
//!
//!   wasi ambient <guest.wasm>          two calls that print what the guest can see
//!   wasi pure <guest.wasm>             the same, with a fixed clock and seeded randomness
//!   wasi call-cost <guest.wasm> <n>    n times: new store, new WasiCtx, instantiate
//!   wasi trap-leak <guest.wasm> <n>    n calls that each touch 8 MiB and then abort

use attack_wasmtime::{rss_mib, WasiState};
use rand::SeedableRng;
use std::time::{Duration, Instant};
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Engine, Result, Store, StoreLimitsBuilder};
use wasmtime_wasi::p2::bindings::CommandPre;
use wasmtime_wasi::{HostMonotonicClock, HostWallClock, WasiCtxBuilder};

/// The design's change for a `Pure` call: clocks that always read the same time.
struct FixedWall;
impl HostWallClock for FixedWall {
    fn resolution(&self) -> Duration {
        Duration::from_secs(1)
    }
    fn now(&self) -> Duration {
        Duration::from_secs(1_000_000_000)
    }
}
struct FixedMonotonic;
impl HostMonotonicClock for FixedMonotonic {
    fn resolution(&self) -> u64 {
        1
    }
    fn now(&self) -> u64 {
        0
    }
}

fn store(engine: &Engine, mode: &str, stdout: bool) -> Store<WasiState> {
    store_with(engine, mode, stdout, false)
}

fn store_with(engine: &Engine, mode: &str, stdout: bool, pure: bool) -> Store<WasiState> {
    let mut b = WasiCtxBuilder::new();
    b.args(&["guest", mode]);
    if stdout {
        b.inherit_stdout();
    }
    if pure {
        b.wall_clock(FixedWall)
            .monotonic_clock(FixedMonotonic)
            .secure_random(rand::rngs::StdRng::seed_from_u64(7))
            .insecure_random(rand::rngs::StdRng::seed_from_u64(8))
            .insecure_random_seed(9);
    }
    let limits = StoreLimitsBuilder::new().memory_size(64 << 20).build();
    let mut s = Store::new(engine, WasiState { ctx: b.build(), table: ResourceTable::new(), limits });
    s.limiter(|s| &mut s.limits);
    s
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let (mode, guest) = match (args.get(1), args.get(2)) {
        (Some(m), Some(g)) => (m.clone(), g.clone()),
        _ => {
            eprintln!("usage: wasi ambient|call-cost|trap-leak <guest.wasm> [n]");
            std::process::exit(2);
        }
    };
    let n: usize = args.get(3).and_then(|a| a.parse().ok()).unwrap_or(1);
    let engine = Engine::default();
    let component = Component::from_file(&engine, &guest)?;
    let mut linker: Linker<WasiState> = Linker::new(&engine);
    wasmtime_wasi::p2::add_to_linker_async(&mut linker)?;
    let pre = CommandPre::new(linker.instantiate_pre(&component)?)?;

    match mode.as_str() {
        "ambient" | "pure" => {
            for _ in 0..2 {
                let mut st = store_with(&engine, "ambient", true, mode == "pure");
                let cmd = pre.instantiate_async(&mut st).await?;
                let _ = cmd.wasi_cli_run().call_run(&mut st).await?;
            }
        }
        "call-cost" => {
            let rss0 = rss_mib();
            let mut micros = Vec::with_capacity(n);
            for _ in 0..n {
                let t = Instant::now();
                let mut st = store(&engine, "noop", false);
                let _cmd = pre.instantiate_async(&mut st).await?;
                micros.push(t.elapsed().as_secs_f64() * 1e6);
            }
            micros.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let at = |q: f64| micros[((micros.len() - 1) as f64 * q).round() as usize];
            println!(
                "new store + WasiCtx + instantiate, {n} calls: median {:.0} us, p99 {:.0} us, max {:.0} us; RSS {rss0} -> {} MiB",
                at(0.5),
                at(0.99),
                micros[micros.len() - 1],
                rss_mib()
            );
        }
        "trap-leak" => {
            let mut trapped = 0;
            let mut line = format!("RSS after 0 calls: {} MiB", rss_mib());
            for i in 1..=n {
                let mut st = store(&engine, "touch-abort", false);
                let cmd = pre.instantiate_async(&mut st).await?;
                if cmd.wasi_cli_run().call_run(&mut st).await.is_err() {
                    trapped += 1;
                }
                if i % 1000 == 0 {
                    line.push_str(&format!(", after {i}: {} MiB", rss_mib()));
                }
            }
            println!("{n} calls that each touch 8 MiB and abort ({trapped} trapped). {line}");
        }
        _ => {
            eprintln!("unknown mode {mode}");
            std::process::exit(2);
        }
    }
    Ok(())
}
