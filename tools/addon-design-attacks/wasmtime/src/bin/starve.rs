//! Finding 8: add-on calls that share the server's tokio runtime can starve it.
//!
//!   starve <guest.wasm> <calls> [trap|yield|yield-now|separate|separate-yield]
//!
//! The server's runtime has 4 worker threads. A heartbeat task on it sleeps 50 ms in a loop
//! and records the longest gap between two of its wake-ups. After 300 ms, `calls` tasks each
//! instantiate the guest with `spin`, which loops forever, and call it inside a 3 s `tokio`
//! timeout. An engine thread ticks the epoch every 100 ms, and every call has a deadline of
//! 30 ticks. The mode says what happens at each tick and where the calls run:
//!
//! - `trap` (the default, as in the finding): the call runs on the server's runtime and traps
//!   at the deadline. It never yields.
//! - `yield`: the call runs on the server's runtime. An epoch callback returns
//!   `UpdateDeadline::Yield(1)` on each tick and `UpdateDeadline::Interrupt` at the deadline.
//! - `yield-now`: the same, but the callback yields with `tokio::task::yield_now`, through
//!   `UpdateDeadline::YieldCustom`, as the wasmtime 50.0.1 docs recommend for tokio.
//! - `separate`: as `yield-now`, but the calls run on a second runtime with its own 4 worker
//!   threads. This is the design's change. The heartbeat stays on the server's runtime.
//! - `separate-yield`: as `yield`, but the calls run on the second runtime. Each call prints
//!   when it ended, which shows whether its 3 s timeout fired on time.

use attack_wasmtime::WasiState;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Config, Engine, Result, Store, StoreLimitsBuilder, UpdateDeadline};
use wasmtime_wasi::p2::bindings::CommandPre;
use wasmtime_wasi::WasiCtxBuilder;

const WORKERS: usize = 4;
const TICK: Duration = Duration::from_millis(100);
const DEADLINE_TICKS: u64 = 30;
const DEADLINE: Duration = Duration::from_secs(3);

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Trap,
    Yield,
    YieldNow,
    Separate,
    SeparateYield,
}

fn runtime() -> std::io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_multi_thread().worker_threads(WORKERS).enable_all().build()
}

async fn one_call(pre: CommandPre<WasiState>, engine: Engine, mode: Mode) -> (Duration, &'static str) {
    let mut b = WasiCtxBuilder::new();
    b.args(&["guest", "spin"]);
    let limits = StoreLimitsBuilder::new().memory_size(64 << 20).build();
    let mut st = Store::new(&engine, WasiState { ctx: b.build(), table: ResourceTable::new(), limits });
    st.limiter(|s| &mut s.limits);
    if mode == Mode::Trap {
        st.set_epoch_deadline(DEADLINE_TICKS);
        st.epoch_deadline_trap();
    } else {
        st.set_epoch_deadline(1);
        let mut ticks = 0;
        st.epoch_deadline_callback(move |_| {
            ticks += 1;
            Ok(match (ticks >= DEADLINE_TICKS, mode) {
                (true, _) => UpdateDeadline::Interrupt,
                (false, Mode::Yield | Mode::SeparateYield) => UpdateDeadline::Yield(1),
                (false, _) => UpdateDeadline::YieldCustom(1, Box::pin(tokio::task::yield_now())),
            })
        });
    }
    let t = Instant::now();
    let r = tokio::time::timeout(DEADLINE, async {
        let cmd = pre.instantiate_async(&mut st).await?;
        cmd.wasi_cli_run().call_run(&mut st).await
    })
    .await;
    (t.elapsed(), match r { Err(_) => "tokio timeout", Ok(Err(_)) => "epoch deadline trap", Ok(Ok(_)) => "returned" })
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let guest = args
        .get(1)
        .expect("usage: starve <guest.wasm> <calls> [trap|yield|yield-now|separate|separate-yield]")
        .clone();
    let calls: usize = args.get(2).and_then(|a| a.parse().ok()).unwrap_or(1);
    let (mode, label) = match args.get(3).map(String::as_str).unwrap_or("trap") {
        "trap" => (Mode::Trap, "same runtime, epoch trap at 3 s, no yield"),
        "yield" => (Mode::Yield, "same runtime, UpdateDeadline::Yield every 100 ms"),
        "yield-now" => (Mode::YieldNow, "same runtime, tokio::task::yield_now every 100 ms"),
        "separate" => (Mode::Separate, "calls on a second 4-worker runtime, tokio::task::yield_now every 100 ms"),
        "separate-yield" => (Mode::SeparateYield, "calls on a second 4-worker runtime, UpdateDeadline::Yield every 100 ms"),
        other => panic!("unknown mode {other}"),
    };

    let mut config = Config::new();
    config.epoch_interruption(true);
    let engine = Engine::new(&config)?;
    {
        let engine = engine.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(TICK);
            engine.increment_epoch();
        });
    }
    let component = Component::from_file(&engine, &guest)?;
    let mut linker: Linker<WasiState> = Linker::new(&engine);
    wasmtime_wasi::p2::add_to_linker_async(&mut linker)?;
    let pre = CommandPre::new(linker.instantiate_pre(&component)?)?;

    let server = runtime()?;
    let addons = runtime()?;
    let addon_handle = match mode {
        Mode::Separate | Mode::SeparateYield => addons.handle().clone(),
        _ => server.handle().clone(),
    };
    server.block_on(async move {
        let done = Arc::new(AtomicBool::new(false));
        let heartbeat = {
            let done = done.clone();
            tokio::spawn(async move {
                let mut longest = Duration::ZERO;
                let mut last = Instant::now();
                while !done.load(Ordering::Relaxed) {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    longest = longest.max(last.elapsed());
                    last = Instant::now();
                }
                longest
            })
        };
        tokio::time::sleep(Duration::from_millis(300)).await;

        let handles: Vec<_> = (0..calls).map(|_| addon_handle.spawn(one_call(pre.clone(), engine.clone(), mode))).collect();
        for h in handles {
            let (took, how) = h.await.unwrap();
            println!("  call ended by {how} after {:.1} s", took.as_secs_f64());
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
        done.store(true, Ordering::Relaxed);
        let longest = heartbeat.await.unwrap();
        println!(
            "{calls} concurrent call(s) into a looping guest, {WORKERS} tokio workers, {label}: a 50 ms heartbeat task on the server's runtime stalled for up to {} ms",
            longest.as_millis()
        );
        Ok(())
    })
}
