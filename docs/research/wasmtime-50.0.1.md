# Wasmtime 50.0.1 SQL add-on reference

This page records whether a deployarr add-on's server program can be one WebAssembly component that reaches an order database. The component targets `wasm32-wasip2`, which is WASI 0.2. A Rust host runs it with `wasmtime` 50.0.1 and `wasmtime-wasi` 50.0.1. The page covers SQL Server, PostgreSQL, and MySQL or MariaDB, over TCP, with and without TLS. Use it when you choose how the SQL order lookup add-on runs. The alternatives are a native executable per OS and a SQL capability that the host provides. This page does not test those.

## Sources

Each source has a short label. Citations name the label, then a path and a line range.

| Label | Source | Version | How it was obtained |
| --- | --- | --- | --- |
| lever | `tools/addon-wasm-prototype/` in the deployarr working tree | uncommitted, 2026-11-12 | written in this session. `reproduce.sh` builds and runs every probe. |
| run2 | `$SCRATCH/wasm-proto/run2.out` and the `logs/` and `runs/` folders under `$SCRATCH/wasm-proto/run2/` | 2026-11-12 | `bash tools/addon-wasm-prototype/reproduce.sh $SCRATCH/wasm-proto/run2`, which took 777 seconds |
| run1 | `$SCRATCH/wasm-proto/run1.out` and `$SCRATCH/wasm-proto/run1/` | 2026-11-12 | an earlier run of the same script, before the WASI libc builds were added |
| manual | `$SCRATCH/wasm-proto/manual/` | 2026-11-12 | single runs by hand with the same host and components |
| crates | crate sources under `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/` | the versions named in each citation | downloaded by Cargo from crates.io |
| wasi-pg-client | `README.md` of `wasi-pg-client` 0.2.1 | 0.2.1 | crates |

`$SCRATCH` is a scratch directory outside the repository, as in [`tools/re/README.md`](../../tools/re/README.md).

The probes resolved these versions. Each probe's `Cargo.lock` is in the lever, and run2 prints them.

| Crate | Version |
| --- | --- |
| `wasmtime`, `wasmtime-wasi` | 50.0.1. `cargo search wasmtime` listed 51.0.0-rc.1 as the newest, a release candidate, on 2026-11-12. |
| `tokio` | 1.54.1 |
| `mio` | 1.2.4 |
| `rustls` | 0.23.46 |
| `ring` | 0.17.14 |
| `aws-lc-sys` | 0.47.0 |
| `tokio-rustls` | 0.26.6 |
| `sqlx`, `sqlx-core` | 0.9.0 |
| `tokio-postgres` | 0.7.18 |
| `mysql_async` | 0.37.1 |
| `tiberius` | 0.13.0 |
| `rustls-native-certs` | 0.8.5 |
| `wasi-pg-client` | 0.2.1, with `wasip3` 0.8.0+wasi-0.3.0 |

The build machine ran Ubuntu 24.04.5, Linux 6.18, `rustc` 1.99.1, and Ubuntu clang 18.1.3. It had 4 vCPUs, an Intel Xeon at 2.10 GHz, and 15 GB of memory, shared with other agents (manual `hardware.txt`, plus `lsb_release` and `clang --version` rerun on 2026-11-12). The servers were PostgreSQL 16.15 and MariaDB 10.11.14 from Ubuntu's packages, on `127.0.0.1`. SQL Server was not available.

## What was tested

Each probe is a small Rust program that builds to a component with `cargo build --release --target wasm32-wasip2`. It reads `DB_URL` from its environment and runs `SELECT 1`. The `sqlx`, `tokio-postgres`, `mysql_async`, and `tiberius` probes then run one query with a bound parameter, `SELECT $1::int4 + 1`, `SELECT CAST(? AS SIGNED) + 1`, or `SELECT @P1 + 1`, with the value 41. The `sqlx`, `tokio-postgres-raw`, and `mysql_async` probes then ask the server whether the session uses TLS. PostgreSQL answers from `pg_stat_ssl`, and MariaDB answers with the `Ssl_cipher` status (lever `probes/*/src/main.rs`). The `wasi-pg-client` probe runs only `SELECT 1` (lever `probes/wasi-pg-client/src/lib.rs`). The `rustls-aws-lc` probe opens no connection. It only builds a TLS client configuration (lever `probes/rustls-aws-lc/src/main.rs`).

The host is `addon-host` (lever `host/src/main.rs`). By default it links the WASI 0.2 interfaces through `wasmtime_wasi::p2::add_to_linker_sync` and calls the component's `wasi:cli/run` export. With `--p3` it turns on `wasm_component_model_async`, links WASI 0.2 through `wasmtime_wasi::p2::add_to_linker_async` and WASI 0.3 through `wasmtime_wasi::p3::add_to_linker`, and calls the export inside `run_concurrent`.

TLS runs used a test CA made with `openssl`. The server certificate names `IP:127.0.0.1` (lever `setup-servers.sh:11-14`). Each component received the CA as PEM text in the `CA_PEM` variable and checked the server certificate against it. `setup-servers.sh` also checks the same certificates with `psql` and `sslmode=verify-full`, and with the `mariadb` client and `--ssl-verify-server-cert` (lever `setup-servers.sh:44-47`). Both connected over TLS when rerun on 2026-11-12.

## Results by driver

| Probe | Builds for `wasm32-wasip2` | Runs | TLS | Evidence |
| --- | --- | --- | --- | --- |
| `sqlx` 0.9 `postgres` | yes, with `--cfg tokio_unstable` | yes | yes, verified | run2 `runs/sqlx-postgres-*.log` |
| `sqlx` 0.9 `mysql` | yes, with `--cfg tokio_unstable` | yes, MariaDB | yes, verified | run2 `runs/sqlx-mysql-*.log` |
| `mysql_async` 0.37 | yes, with `--cfg tokio_unstable` | yes, MariaDB | yes, verified | run2 `runs/mysql-async-*.log` |
| `tokio-postgres` 0.7 with `runtime` | no | no | no | run2 `logs/build-tokio-postgres-unstable.log` |
| `tokio-postgres` 0.7 without `runtime` | yes, with `--cfg tokio_unstable` | yes | yes, verified | run2 `runs/tokio-postgres-raw-*.log` |
| `tiberius` 0.13 with `rustls` | no | no | no | run2 `logs/build-tiberius-unstable.log` |
| `tiberius` 0.13 with `rustls`, WASI libc headers, and a 6-line patch | yes, with `--cfg tokio_unstable` | compile only | not tested | run2 `logs/build-tiberius+wasi-libc+certs-patch.log` |
| `tiberius` 0.13 without TLS | yes, with `--cfg tokio_unstable` | compile only | none | run2 `logs/build-tiberius-notls-unstable.log` |
| `wasi-pg-client` 0.2.1 | yes, with no flags | yes, with `--p3` | not tested | run2 `runs/wasi-pg-client-plain.log` |

"Verified" means three runs. The run without TLS reported `tls = false` or `tls = none`. The TLS run reported `tls = true` from PostgreSQL or `tls = TLS_AES_256_GCM_SHA384` from MariaDB. A third run trusted Ubuntu's `ssl-cert-snakeoil.pem` instead of the test CA, and it failed with `InvalidCertificate(UnknownIssuer)` (run2 `runs/*-wrongca.log`). So each client checked the certificate chain inside the component.

In run1 and run2, every run that succeeded printed `select 1 = 1`, or `select 1 = Some(1)` from `mysql_async`. Every one except `wasi-pg-client` also printed `param = 42`, or `param = Some(42)` from `mysql_async` (run1 and run2 `runs/*.log`).

### PostgreSQL

- `sqlx` connected with `PgConnectOptions`. TLS used `PgSslMode::VerifyFull` and `ssl_root_cert_from_pem` (lever `probes/sqlx-postgres/src/main.rs`). A `sslrootcert` parameter in the URL goes to `ssl_root_cert`, which takes a file path (crates `sqlx-postgres-0.9.0/src/options/parse.rs:56-58`, `src/options/mod.rs:241`). The host preopens no directory (lever `host/src/main.rs`), so such a path would not resolve inside the component (inferred, not tested).
- `tokio-postgres` with its default `runtime` feature fails to build. `src/lib.rs:173-174` compiles out `mod keepalive` on `wasm32`, but `src/client.rs:8` still imports it, and `Cargo.toml:233` drops `socket2` on `wasm32` (crates `tokio-postgres-0.7.18`). The first errors are `error[E0432]: unresolved import crate::keepalive` and `error[E0432]: unresolved import socket2` (run2).
- `tokio-postgres` without `runtime` builds. The probe opens a `tokio` `TcpStream` itself and calls `Config::connect_raw` (crates `tokio-postgres-0.7.18/src/config.rs:739-749`). `tokio-postgres-rustls` 0.14.0 cannot supply TLS here, because it turns `runtime` back on (crates `tokio-postgres-rustls-0.14.0/Cargo.toml`, `[dependencies.tokio-postgres]`). The probe implements `TlsConnect` over `tokio-rustls` in about 40 lines instead.
- `wasi-pg-client` uses WASI 0.3 sockets, not `tokio`. Its README says that running against PostgreSQL requires a separately documented integration-test environment, and it names Wasmtime 48.0.1 as the runtime it validated (wasi-pg-client `README.md:3`, `:102`). Here it ran `SELECT 1` against PostgreSQL 16 through `addon-host --p3` on Wasmtime 50.0.1. Its TLS was not tested. Its `TlsConfig::ca_cert` takes CA bytes, and the embedded Mozilla roots apply only when it is unset (crates `wasi-pg-client-0.2.1/src/transport/tls.rs:33-35`, `:263-277`). `wasmtime-wasi` 50.0.1 ships the WASI 0.3.0 packages, such as `wasi:sockets@0.3.0` (crates `wasmtime-wasi-50.0.1/src/p3/wit/`). The host must turn on `Config::wasm_component_model_async`, whose doc comment says that `wasmtime`'s support for the feature is "_very_ incomplete" (crates `wasmtime-50.0.1/src/config.rs:1301-1311`).

### MySQL and MariaDB

- `sqlx` with `mysql` and `mysql_async` both ran against MariaDB 10.11.14. MySQL itself was not tested.
- `sqlx` TLS used `MySqlSslMode::VerifyIdentity` and `ssl_ca_from_pem`. `mysql_async` TLS used `SslOpts::with_root_certs` with a `PathOrBuf::Buf` (crates `mysql_async-0.37.1/src/opts/mod.rs:146-149`, `:181-184`, `:238`).

### SQL Server

- `tiberius` with its `rustls` feature fails for two separate reasons.
- First, `tiberius` enables `tokio-rustls` with its default features (crates `tiberius-0.13.0/Cargo.toml:296-298`). Those include `aws_lc_rs` (crates `tokio-rustls-0.26.6/Cargo.toml:56-59`). `aws-lc-sys` 0.47.0 then compiles C for `wasm32-wasip2` against `/usr/include` and stops with `fatal error: 'bits/libc-header-start.h' file not found` (run2 `logs/build-tiberius-unstable.log`). A downstream crate cannot turn those default features off (inferred from Cargo's feature unification).
- Second, the `rustls` feature pulls in `rustls-native-certs` (crates `tiberius-0.13.0/Cargo.toml:73-79`). That crate defines its `platform` module only for Unix, Windows, and macOS (crates `rustls-native-certs-0.8.5/src/lib.rs:31-44`). `target_os = "wasi"` is none of these, so `src/lib.rs:123` fails with `error[E0433]: cannot find module or crate platform`.
- With the WASI libc headers from Ubuntu's `wasi-libc` package passed as `CFLAGS_wasm32_wasip2="-isystem /usr/include/wasm32-wasi"`, and with `--cfg tokio_unstable`, `aws-lc-sys` builds. The `rustls-native-certs` error remains (run2, the `tiberius+wasi-libc` row; lever `reproduce.sh:49-64`).
- With both the headers and a 6-line patch that gives `rustls-native-certs` an empty `platform` module for WASI, `tiberius` with `rustls` builds. Cargo applied the patch with `--config patch.crates-io.rustls-native-certs.path=...` (lever `reproduce.sh:65-81`). The component is 3,261,869 bytes (run2 `target-unstable/wasm32-wasip2/release/probe-tiberius.wasm`).
- That component started in `addon-host`, passed the host's socket check for `127.0.0.1:1433`, and failed with `ConnectionRefused`, because nothing listened there (manual `tiberius-patched-noserver.log`). No TDS login or query ran. SQL Server through a component is therefore INCONCLUSIVE.
- `tiberius` can trust a CA given as in-memory bytes through `trust_cert_ca_bundle` (crates `tiberius-0.13.0/src/client/config.rs:457-462`). Its doc comment on `build_trust_store` says that an empty OS store is tolerated when extra CAs are supplied (crates `tiberius-0.13.0/src/client/tls_stream/rustls_tls_stream.rs:318-327`). So the patched build with its empty `platform` module could still verify a server against a CA passed in memory (inferred, not tested).
- `tiberius` with `tds73` only and no TLS builds with `tokio_unstable`. Without a TLS backend, `tiberius` defaults to `EncryptionLevel::NotSupported`, which its doc comment describes as "Do not encrypt anything" (crates `tiberius-0.13.0/src/client/config.rs:358-363`, `src/tds.rs:19-27`). Such a client sends the login and the queries in clear text and needs a server that accepts unencrypted connections, so this build is not a usable path (inferred, not tested).
- Windows authentication was not tested. The `winauth` feature was left out of every probe.

## tokio on `wasm32-wasip2`

- `tokio` 1.54.1 stops the build with `compile_error!("Only features sync,macros,io-util,rt,time are supported on wasm.")` when any of `fs`, `io-std`, `net`, `process`, `rt-multi-thread`, or `signal` is on for a `wasm` target without `tokio_unstable` (crates `tokio-1.54.1/src/lib.rs:503-516`). Every `tokio`-based probe hit this error without the flag (run2).
- Its docs say `wasm32-wasip1` and `wasm32-wasip2` support `tokio::net` under `tokio_unstable`, and that only `wasm32-wasip2` can create sockets from inside WASM (crates `tokio-1.54.1/src/lib.rs:460-470`).
- With `RUSTFLAGS="--cfg tokio_unstable"`, `tokio::net::TcpStream::connect` worked in every `tokio` probe that ran (run2). `sqlx` and `mysql_async` call it themselves (crates `sqlx-core-0.9.0/src/net/socket/mod.rs:192`, `mysql_async-0.37.1/src/io/mod.rs:358-364`), and the `tokio-postgres-raw` probe calls it directly. The probes used the `current_thread` runtime. `tokio-postgres-raw` also ran its connection task with `tokio::spawn` (lever `probes/*/src/main.rs`).
- `mio` 1.2.4 builds its Unix back end for `target_os = "wasi"` when the environment is not `p1` (crates `mio-1.2.4/src/sys/mod.rs:53-57`). The selector for WASI is `selector/poll.rs` (crates `mio-1.2.4/src/sys/unix/mod.rs:45-60`). The `poll` call then comes from the Rust target's WASI libc, which maps it onto `wasi:io/poll` (inferred, not read).
- `tokio_unstable` is a `RUSTFLAGS` setting. It applies to the whole build of the add-on, and `tokio` may change unstable APIs in any release (inferred from the flag's name and the `tokio` docs quoted above).

## rustls on `wasm32-wasip2`

- `rustls` 0.23.46 with the `ring` provider built and completed TLS handshakes in four probes (run2).
- `ring` 0.17.14 compiles its C sources for `wasm32` with `-nostdlibinc`, under the comment "Allow cross-compiling without a target sysroot for these targets" (crates `ring-0.17.14/build.rs:594-603`). It adds the flag only when the C compiler is clang (`build.rs:599`). `ring` has no assembly target for `wasm32` (`build.rs:172-223`). The probes built with clang 18.1.3. Another C compiler was not tried.
- `rustls` with the `aws-lc-rs` provider fails without WASI libc headers. The error is the same `bits/libc-header-start.h` error as `tiberius` (run2 `logs/build-rustls-aws-lc-unstable.log`).
- With the headers from `wasi-libc` 0.0~git20230113.4362b18-3, the `aws-lc-rs` probe built, ran, and built a client configuration with 9 cipher suites (manual `rustls-aws-lc-run.log`). It made no TLS connection.

## Network limits

- `WasiCtxBuilder::socket_addr_check` takes a function that receives each socket address and its use, and returns whether to allow it (crates `wasmtime-wasi-50.0.1/src/ctx.rs:393-406`). `allow_tcp`, `allow_udp`, and `allow_ip_name_lookup` switch each kind of use on or off, and each is off by default (`src/ctx.rs:408-430`). `SocketAddrUse` names the uses, such as `TcpBind` and `TcpConnect` (`src/sockets/mod.rs:172`).
- The host allowed `TcpConnect` to one address, allowed `TcpBind` to `0.0.0.0:0`, and denied UDP and name lookups. Every connect was preceded by a `TcpBind` check on `0.0.0.0:0` (run2, the `socket check` lines in each log). The doc comment of `TcpBind` says it covers the implicit bind of a connect (crates `wasmtime-wasi-50.0.1/src/sockets/mod.rs:173-181`). So a host that denies that bind blocks every connect (inferred, not tested).
- With the allow list set to `127.0.0.1:5433`, `sqlx-postgres` failed to reach `127.0.0.1:5432`. The host logged `socket check TcpConnect 127.0.0.1:5432 -> deny`, and the guest got `PermissionDenied` (run2 `runs/deny.log`).
- The same check applied to the WASI 0.3 sockets of `wasi-pg-client` (run2 `runs/wasi-pg-client-plain.log`, which shows the same two check lines).
- The probes connected to IP addresses. With name lookups denied, a host name in the connection string would fail (inferred, not tested). A host that resolves the name itself and allows only the resolved address is one option (inferred).

## Sizes and times

These are raw single-run facts on the shared 4-vCPU machine. They are not benchmarks. run1 and run2 gave the same component sizes (`run1.out`, `run2.out`). Their timings differed by up to about 65%, for the instantiation of `sqlx-mysql` without TLS, which took 0.511 ms in run1 and 0.311 ms in run2.

| Item | Size |
| --- | --- |
| `probe-sqlx-postgres.wasm` | 2,741,486 bytes |
| `probe-sqlx-mysql.wasm` | 2,269,781 bytes |
| `probe-mysql-async.wasm` | 4,140,585 bytes |
| `probe-tokio-postgres-raw.wasm` | 2,119,614 bytes |
| `probe_wasi_pg_client.wasm` | 2,255,545 bytes |
| `probe-tiberius.wasm`, patched | 3,261,869 bytes |
| `probe-tiberius-notls.wasm` | 1,224,598 bytes |
| `addon-host`, release | 37,603,440 bytes, 25,063,936 after `strip` (manual `host-size.txt`) |

The components are release builds with default profile settings. Nothing stripped or optimized them for size.

From run2, in milliseconds:

| Run | Compile | Instantiate | Guest run |
| --- | --- | --- | --- |
| `sqlx-postgres`, plain | 673.4 | 0.383 | 11.1 |
| `sqlx-postgres`, TLS | 602.2 | 0.448 | 101.1 |
| `tokio-postgres-raw`, TLS | 537.8 | 0.319 | 101.9 |
| `sqlx-mysql`, plain | 564.0 | 0.311 | 2.5 |
| `sqlx-mysql`, TLS | 557.5 | 0.414 | 47.7 |
| `mysql-async`, TLS | 1041.9 | 0.523 | 50.8 |
| `wasi-pg-client`, plain | 499.0 | 0.422 | 9.7 |

- "Compile" is `Component::from_file`, which compiles the component with Cranelift on every start. "Instantiate" is `Command::instantiate`, or the WASI 0.3 `Command::instantiate_async` with `--p3`. "Guest run" covers the connection, the handshake, and the queries (lever `host/src/main.rs`).
- Instantiation took under 1 ms in every run. The compile step took 0.48 to 1.04 seconds in run2, and up to 1.22 seconds in run1. `Component::serialize` writes a compiled component, and `Component::deserialize` loads it. Only `Component::from_binary` needs the `cranelift` or `winch` feature (crates `wasmtime-50.0.1/src/runtime/component/component.rs:215-216`, `:242`, `:623`). Loading a serialized component was not measured here.
- The TLS runs against PostgreSQL took about 100 ms, against about 50 ms for MariaDB. This page does not explain the gap. No profile was taken.
- A release build of the host took 5 minutes 54 seconds with 2 jobs (run2 `logs/build-host.log`).

## Not tested

- A Windows host. `wasmtime` and the components were built and run on Linux only. The same `.wasm` file should run on Windows, because the component has no OS-specific code (inferred).
- MySQL Server, as opposed to MariaDB.
- SQL Server, for any probe.
- TLS for `wasi-pg-client`.
- Read-only transactions, timeouts, and connection pools inside a component.
- Memory and CPU limits on the guest, such as fuel or epoch interruption.

## What changes in deployarr

- A WASI 0.2 component can reach PostgreSQL and MariaDB over TCP with verified TLS today. `sqlx` 0.9, which [Data sources](../architecture.md#data-sources) already names for those engines, works inside the component with `ring` as its TLS provider. A `sqlx` add-on build needs `RUSTFLAGS="--cfg tokio_unstable"`, and it needs clang for `ring` (the clang part is inferred from `ring`'s build script).
- SQL Server through `tiberius` with TLS does not build for `wasm32-wasip2` without WASI libc headers and a patched `rustls-native-certs`. Even then, no TDS session was observed. If deployarr adopts components for add-ons, it must either keep a patched dependency, run the SQL Server login in a lab before committing, or move SQL Server into a host-provided SQL capability. The choice between these designs stays open until a SQL Server run.
- The host can confine an add-on to the one database address an admin configures, through `socket_addr_check`. Name lookups need a decision, because denying them would break host names in connection strings (inferred, not tested).
- Database credentials reach the component as environment values in this prototype. A real host would pass them through a WIT interface instead (inferred).
- The stripped prototype host, which holds `wasmtime`, `wasmtime-wasi`, and `tokio`, is about 25 MB on Linux. That is an upper bound on what the runtime adds to the server binary (inferred, not measured against the server). Each add-on start compiles its component for about half a second to a second, unless the server caches compiled code.
- Settle the remaining SQL Server question with a Windows lab run of the patched `tiberius` component against SQL Server, with `Encrypt=true` and a CA passed to `trust_cert_ca_bundle`.
