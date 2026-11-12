//! A guest for `wasm32-wasip2` that the `wasi` and `starve` probes run. Its first argument
//! picks what it does:
//!
//! - `ambient` prints what an empty WASI context still gives it: the wall clock, the
//!   monotonic clock, randomness, and the result of opening a file, listing a folder,
//!   connecting a socket, and reading the environment.
//! - `spin` loops forever without calling the host.
//! - `noop` returns at once.
//! - `touch-abort` writes to 8 MiB of memory and then aborts, which traps.

use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn ambient() {
    let wall = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs());
    println!("guest: wall clock (unix seconds) = {wall:?}");
    let t = Instant::now();
    std::thread::sleep(Duration::from_millis(50));
    println!("guest: monotonic clock measured a 50 ms sleep as {} ms", t.elapsed().as_millis());
    println!("guest: random-seeded hash = {:016x}", RandomState::new().hash_one(42u64));
    println!("guest: File::open(\"/etc/passwd\") = {:?}", std::fs::File::open("/etc/passwd").map(|_| ()));
    println!("guest: read_dir(\".\") = {:?}", std::fs::read_dir(".").map(|_| ()));
    println!(
        "guest: TcpStream::connect(\"127.0.0.1:5432\") = {:?}",
        std::net::TcpStream::connect("127.0.0.1:5432").map(|_| ())
    );
    println!("guest: environment variables = {}", std::env::vars().count());
}

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("ambient") => ambient(),
        Some("spin") => loop {
            std::hint::black_box(());
        },
        Some("touch-abort") => {
            let mut v = vec![0u8; 8 << 20];
            for i in (0..v.len()).step_by(4096) {
                v[i] = 1;
            }
            std::hint::black_box(&v);
            std::process::abort();
        }
        _ => {}
    }
}
