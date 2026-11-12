// Probe: does a tokio::time::timeout around a wasmtime async call bound time spent in a
// host function, as the add-on design claims? Also checks the epoch deadline and StoreLimits.
//
// Finding 22. Cases A to D follow the surviving text of haiku/probe-timeout/src/main.rs
// (lines 1 to 120, recovered on #82). Case E is rewritten from its surviving start and its
// output line. wasmtime 50 removed `Config::async_support`, which is now a deprecated no-op,
// so the engine no longer calls it.
use std::time::{Duration, Instant};
use wasmtime::*;

struct State {
    limits: StoreLimits,
}

const BLOCK_MS: u64 = 3000;
const DEADLINE_MS: u64 = 100;

fn engine() -> Result<Engine> {
    let mut c = Config::new();
    c.epoch_interruption(true);
    Engine::new(&c)
}

fn spawn_ticker(e: Engine) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(10));
        e.increment_epoch();
    });
}

fn blocking_linker(e: &Engine) -> Result<Linker<State>> {
    let mut linker: Linker<State> = Linker::new(e);
    linker.func_wrap("env", "block_sync", || -> i32 {
        std::thread::sleep(Duration::from_millis(BLOCK_MS));
        42
    })?;
    Ok(linker)
}

const BLOCKING_MODULE: &str = r#"(module (import "env" "block_sync" (func $b (result i32)))
      (func (export "run") (result i32) call $b))"#;

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<()> {
    let e = engine()?;
    spawn_ticker(e.clone());

    // Case A: sync host function that blocks the thread for BLOCK_MS (e.g. a blocking driver call).
    {
        let linker = blocking_linker(&e)?;
        let m = Module::new(&e, BLOCKING_MODULE)?;
        let mut store = Store::new(&e, State { limits: StoreLimitsBuilder::new().build() });
        store.set_epoch_deadline(1);
        store.epoch_deadline_async_yield_and_update(1);
        let inst = linker.instantiate_async(&mut store, &m).await?;
        let f = inst.get_typed_func::<(), i32>(&mut store, "run")?;
        let t = Instant::now();
        let r = tokio::time::timeout(Duration::from_millis(DEADLINE_MS), f.call_async(&mut store, ())).await;
        println!(
            "A blocking_host_fn deadline_ms={DEADLINE_MS} block_ms={BLOCK_MS} elapsed_ms={} result={}",
            t.elapsed().as_millis(),
            match r { Err(_) => "Elapsed (timeout fired)".to_string(), Ok(Ok(v)) => format!("Ok({v})"), Ok(Err(e)) => format!("Err({e})") }
        );
    }

    // Case B (control): async host function that yields. The timeout should fire at DEADLINE_MS.
    {
        let mut linker: Linker<State> = Linker::new(&e);
        linker.func_wrap_async("env", "block_async", |_caller: Caller<'_, State>, _: ()| {
            Box::new(async move {
                tokio::time::sleep(Duration::from_millis(BLOCK_MS)).await;
                42i32
            })
        })?;
        let m = Module::new(&e, r#"(module (import "env" "block_async" (func $b (result i32)))
              (func (export "run") (result i32) call $b))"#)?;
        let mut store = Store::new(&e, State { limits: StoreLimitsBuilder::new().build() });
        store.set_epoch_deadline(1);
        store.epoch_deadline_async_yield_and_update(1);
        let inst = linker.instantiate_async(&mut store, &m).await?;
        let f = inst.get_typed_func::<(), i32>(&mut store, "run")?;
        let t = Instant::now();
        let r = tokio::time::timeout(Duration::from_millis(DEADLINE_MS), f.call_async(&mut store, ())).await;
        println!(
            "B async_host_fn deadline_ms={DEADLINE_MS} block_ms={BLOCK_MS} elapsed_ms={} result={}",
            t.elapsed().as_millis(),
            match r { Err(_) => "Elapsed (timeout fired)".to_string(), Ok(Ok(v)) => format!("Ok({v})"), Ok(Err(e)) => format!("Err({e})") }
        );
    }

    // Case C (control): pure guest loop, no host call. Epoch deadline should trap it.
    {
        let linker: Linker<State> = Linker::new(&e);
        let m = Module::new(&e, r#"(module (func (export "spin") (loop $l (br $l))))"#)?;
        let mut store = Store::new(&e, State { limits: StoreLimitsBuilder::new().build() });
        store.set_epoch_deadline(5);
        let inst = linker.instantiate_async(&mut store, &m).await?;
        let f = inst.get_typed_func::<(), ()>(&mut store, "spin")?;
        let t = Instant::now();
        let r = f.call_async(&mut store, ()).await;
        println!(
            "C pure_loop epoch_ticks=5 elapsed_ms={} result={}",
            t.elapsed().as_millis(),
            match r { Ok(()) => "Ok".to_string(), Err(e) => format!("Err({})", e.root_cause()) }
        );
    }

    // Case D: StoreLimits memory cap at 64 MiB. Guest asks to grow memory to 128 MiB.
    {
        let linker: Linker<State> = Linker::new(&e);
        let m = Module::new(&e, r#"(module (memory (export "mem") 1)
              (func (export "grow") (param i32) (result i32) local.get 0 memory.grow))"#)?;
        let mut store = Store::new(&e, State { limits: StoreLimitsBuilder::new().memory_size(64 << 20).build() });
        store.limiter(|s| &mut s.limits);
        store.set_epoch_deadline(100000);
        let inst = linker.instantiate_async(&mut store, &m).await?;
        let g = inst.get_typed_func::<i32, i32>(&mut store, "grow")?;
        let r = g.call_async(&mut store, 2048).await; // 2048 pages = 128 MiB
        println!("D memory_grow_128MiB_under_64MiB_cap result={:?}", r.map_err(|e| e.root_cause().to_string()));
        let r2 = g.call_async(&mut store, 512).await; // 512 pages = 32 MiB, fits
        println!("D memory_grow_32MiB_under_cap result={:?}", r2.map_err(|e| e.root_cause().to_string()));
    }

    // Case E: two concurrent add-on calls block in a sync host function on a 2-worker runtime.
    // An unrelated task on the same runtime (standing in for the PXE or agent API) ticks every 10 ms.
    {
        let linker = blocking_linker(&e)?;
        let m = Module::new(&e, BLOCKING_MODULE)?;
        let ticker = tokio::spawn(async {
            let mut iv = tokio::time::interval(Duration::from_millis(10));
            let mut last = Instant::now();
            let mut longest = Duration::ZERO;
            let end = Instant::now() + Duration::from_millis(BLOCK_MS + 300);
            while Instant::now() < end {
                iv.tick().await;
                longest = longest.max(last.elapsed());
                last = Instant::now();
            }
            longest
        });
        tokio::time::sleep(Duration::from_millis(100)).await;

        let mut calls = Vec::new();
        for _ in 0..2 {
            let (linker, m, e) = (linker.clone(), m.clone(), e.clone());
            calls.push(tokio::spawn(async move {
                let mut store = Store::new(&e, State { limits: StoreLimitsBuilder::new().build() });
                store.set_epoch_deadline(1);
                store.epoch_deadline_async_yield_and_update(1);
                let inst = linker.instantiate_async(&mut store, &m).await?;
                let f = inst.get_typed_func::<(), i32>(&mut store, "run")?;
                f.call_async(&mut store, ()).await
            }));
        }
        let mut results = Vec::new();
        for c in calls {
            results.push(match c.await.unwrap() { Ok(v) => format!("Ok({v})"), Err(e) => format!("Err({e})") });
        }
        let gap = ticker.await.unwrap();
        println!(
            "E two_blocking_calls_2_workers unrelated_task_max_gap_ms={} results=[{}]",
            gap.as_millis(),
            results.join(", ")
        );
    }
    Ok(())
}
