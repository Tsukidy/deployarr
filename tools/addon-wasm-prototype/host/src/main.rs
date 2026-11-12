use std::net::SocketAddr;
use std::time::{Duration, Instant};

use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::p2::bindings::sync::Command;
use wasmtime_wasi::sockets::SocketAddrUse;
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

struct State {
    ctx: WasiCtx,
    table: ResourceTable,
}

impl WasiView for State {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.ctx,
            table: &mut self.table,
        }
    }
}

struct Args {
    component: String,
    allow: SocketAddr,
    env: Vec<(String, String)>,
    p3: bool,
}

fn parse_args() -> Result<Args, String> {
    let usage = "usage: addon-host <component.wasm> --allow <ip:port> [--env KEY=VALUE]... [--p3]";
    let mut it = std::env::args().skip(1);
    let component = it.next().ok_or(usage)?;
    let mut allow = None;
    let mut env = Vec::new();
    let mut p3 = false;
    while let Some(flag) = it.next() {
        if flag == "--p3" {
            p3 = true;
            continue;
        }
        let value = it.next().ok_or(usage)?;
        match flag.as_str() {
            "--allow" => allow = Some(value.parse().map_err(|e| format!("--allow: {e}"))?),
            "--env" => {
                let (k, v) = value.split_once('=').ok_or("--env needs KEY=VALUE")?;
                env.push((k.to_owned(), v.to_owned()));
            }
            _ => return Err(usage.into()),
        }
    }
    Ok(Args {
        component,
        allow: allow.ok_or(usage)?,
        env,
        p3,
    })
}

fn wasi_ctx(args: &Args) -> WasiCtx {
    let allowed = args.allow;
    let mut builder = WasiCtxBuilder::new();
    builder
        .inherit_stdio()
        .allow_tcp(true)
        .allow_udp(false)
        .allow_ip_name_lookup(false)
        .socket_addr_check(move |addr, usage| {
            let ok = match usage {
                SocketAddrUse::TcpConnect => addr == allowed,
                SocketAddrUse::TcpBind => addr.ip().is_unspecified() && addr.port() == 0,
                _ => false,
            };
            eprintln!("host: socket check {usage:?} {addr} -> {}", if ok { "allow" } else { "deny" });
            Box::pin(async move { ok })
        });
    for (k, v) in &args.env {
        builder.env(k, v);
    }
    builder.build()
}

fn report(compile: Duration, instantiate: Duration, run: Duration, ok: bool) -> ! {
    eprintln!(
        "host: compile_ms={:.1} instantiate_ms={:.3} run_ms={:.1} guest_result={}",
        compile.as_secs_f64() * 1e3,
        instantiate.as_secs_f64() * 1e3,
        run.as_secs_f64() * 1e3,
        if ok { "ok" } else { "err" }
    );
    std::process::exit(if ok { 0 } else { 1 })
}

fn run_p2(args: &Args) -> wasmtime::Result<()> {
    let engine = Engine::new(&Config::new())?;
    let t = Instant::now();
    let component = Component::from_file(&engine, &args.component)?;
    let compile = t.elapsed();

    let mut linker = Linker::<State>::new(&engine);
    wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;
    let mut store = Store::new(
        &engine,
        State {
            ctx: wasi_ctx(args),
            table: ResourceTable::new(),
        },
    );

    let t = Instant::now();
    let command = Command::instantiate(&mut store, &component, &linker)?;
    let instantiate = t.elapsed();

    let t = Instant::now();
    let result = command.wasi_cli_run().call_run(&mut store)?;
    report(compile, instantiate, t.elapsed(), result.is_ok())
}

async fn run_p3(args: &Args) -> wasmtime::Result<()> {
    let mut config = Config::new();
    config.wasm_component_model_async(true);
    let engine = Engine::new(&config)?;
    let t = Instant::now();
    let component = Component::from_file(&engine, &args.component)?;
    let compile = t.elapsed();

    let mut linker = Linker::<State>::new(&engine);
    wasmtime_wasi::p2::add_to_linker_async(&mut linker)?;
    wasmtime_wasi::p3::add_to_linker(&mut linker)?;
    let mut store = Store::new(
        &engine,
        State {
            ctx: wasi_ctx(args),
            table: ResourceTable::new(),
        },
    );

    let t = Instant::now();
    let command = wasmtime_wasi::p3::bindings::Command::instantiate_async(&mut store, &component, &linker).await?;
    let instantiate = t.elapsed();

    let t = Instant::now();
    let result = store
        .run_concurrent(async move |store| command.wasi_cli_run().call_run(store).await)
        .await??;
    report(compile, instantiate, t.elapsed(), result.is_ok())
}

fn main() -> wasmtime::Result<()> {
    let args = parse_args().map_err(wasmtime::Error::msg)?;
    if args.p3 {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(run_p3(&args))
    } else {
        run_p2(&args)
    }
}
