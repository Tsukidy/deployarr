//! Helpers shared by the probes in `src/bin/`.

use wasmtime::component::ResourceTable;
use wasmtime_wasi::{WasiCtx, WasiCtxView, WasiView};

/// A line of `/proc/self/status`, such as `VmRSS` or `VmHWM`, in MiB.
pub fn status_mib(key: &str) -> u64 {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix(key).and_then(|r| r.strip_prefix(':')) {
            let kib: u64 = rest.trim().trim_end_matches("kB").trim().parse().unwrap_or(0);
            return kib / 1024;
        }
    }
    0
}

/// The resident set size of this process, in MiB.
pub fn rss_mib() -> u64 {
    status_mib("VmRSS")
}

/// The peak resident set size of this process, in MiB.
pub fn peak_rss_mib() -> u64 {
    status_mib("VmHWM")
}

/// A store state with a WASI context, a resource table, and store limits.
pub struct WasiState {
    pub ctx: WasiCtx,
    pub table: ResourceTable,
    pub limits: wasmtime::StoreLimits,
}

impl WasiView for WasiState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.ctx,
            table: &mut self.table,
        }
    }
}
