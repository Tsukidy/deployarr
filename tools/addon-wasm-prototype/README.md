# Run the add-on WebAssembly prototype

This prototype tests one question for the add-on design. Can an add-on's server program be one WebAssembly component, built for `wasm32-wasip2`, that a Rust host runs with `wasmtime`? The add-on must still reach SQL Server, PostgreSQL, and MySQL or MariaDB over TCP, with TLS. [Order lookup](../../docs/architecture.md#order-lookup) describes the SQL add-on. The prototype is throwaway, and deployarr shares no code with it.

## What it contains

| Path | Holds |
| --- | --- |
| `probes/` | One small crate per SQL client. Each builds to a component that runs `SELECT 1` and one query with a bound parameter. |
| `host/` | `addon-host`, a `wasmtime` 50 host that runs one component with WASI 0.2, or with WASI 0.3 when given `--p3` |
| `setup-servers.sh` | Installs PostgreSQL and MariaDB, makes a test CA and server certificate, and turns on TLS |
| `reproduce.sh` | Builds every probe and the host, runs each component, and prints a result table |

Each probe's `Cargo.toml` has an empty `[workspace]` table, so Cargo does not treat it as a member of the deployarr workspace.

These are the probes:

| Probe | Client | TLS |
| --- | --- | --- |
| `tiberius` | `tiberius` 0.13 with the `rustls` feature | `rustls` |
| `tiberius-notls` | `tiberius` 0.13 with `tds73` only | none |
| `sqlx-postgres` | `sqlx` 0.9, `postgres`, `runtime-tokio` | `tls-rustls-ring-webpki` |
| `sqlx-mysql` | `sqlx` 0.9, `mysql`, `runtime-tokio` | `tls-rustls-ring-webpki` |
| `tokio-postgres` | `tokio-postgres` 0.7 with its default `runtime` feature | none |
| `tokio-postgres-raw` | `tokio-postgres` 0.7 without `runtime`, over a `tokio` `TcpStream` with `connect_raw` | `tokio-rustls` with `ring` |
| `mysql-async` | `mysql_async` 0.37 with `default-rustls-ring` | `rustls` |
| `wasi-pg-client` | `wasi-pg-client` 0.2, which uses WASI 0.3 sockets | not tested |
| `rustls-aws-lc` | `rustls` 0.23 with the `aws-lc-rs` provider and no SQL client. It builds a TLS client configuration. | `aws-lc-rs` |

## Requirements

- Linux with `rustup` and Cargo. The script adds the `wasm32-wasip2` target.
- `clang`. The `ring` and `aws-lc-sys` crates compile C code for `wasm32` with it.
- Optionally, the WASI libc headers from `apt-get install -y wasi-libc`. `aws-lc-sys` needs them. Without them the script skips the builds that use them.
- Root and `apt-get` for `setup-servers.sh`. It was run on Ubuntu 24.04.
- About 3 GB of disk in the output folder. A full run took 2.6 GB and 13 minutes on a 4-vCPU cloud VM.

SQL Server cannot run in the cloud sessions, so the `tiberius` probes only build.

## Set up the servers

```bash
sudo bash tools/addon-wasm-prototype/setup-servers.sh
```

The script writes the certificates to `/etc/probe-certs`. The server certificate names `127.0.0.1` in its subject alternative name. Both servers get the user `probe` with the password `probe` and a database named `probe`. The MariaDB user can only read that database. The script can run again.

## Run the probes

Pass a folder outside the repository for the build output and logs:

```bash
bash tools/addon-wasm-prototype/reproduce.sh "$SCRATCH/addon-wasm"
```

The script builds each probe twice. The first build uses the default configuration. The second passes `RUSTFLAGS="--cfg tokio_unstable"`, because `tokio` 1.54 allows its `net` feature on `wasm32` only with that flag. Each build uses 2 jobs. Set `JOBS` to change it.

When `/usr/include/wasm32-wasi` exists, the script builds three more times with `tokio_unstable` and `CFLAGS_wasm32_wasip2="-isystem /usr/include/wasm32-wasi"`. Set `WASI_INCLUDE` to use another folder.

- `rustls-aws-lc+wasi-libc` builds `rustls-aws-lc`.
- `tiberius+wasi-libc` builds `tiberius`.
- `tiberius+wasi-libc+certs-patch` builds a copy of `tiberius` in the output folder. Cargo replaces `rustls-native-certs` with a copy that has an empty `platform` module for `target_os = "wasi"`. The patch adds 6 lines.

The script runs each component that built against `127.0.0.1:5432` for PostgreSQL and `127.0.0.1:3306` for MariaDB. Set `PG_ADDR` and `MY_ADDR` to change them. A probe that supports TLS runs three times:

- without TLS
- with TLS, trusting the CA in `CA_FILE`, which defaults to `/etc/probe-certs/ca.pem`
- with TLS, trusting only the certificate in `WRONG_CA_FILE`, which defaults to Ubuntu's `ssl-cert-snakeoil.pem`. This run must fail.

The script then runs `sqlx-postgres` with the host's allow list set to the wrong port. That run must fail too.

## Run one component by hand

```bash
addon-host probe.wasm --allow 127.0.0.1:5432 --env 'DB_URL=postgres://probe:probe@127.0.0.1:5432/probe?sslmode=disable'
```

The component reads `DB_URL`. It reads `CA_PEM` as the text of a PEM file, and turns on TLS with certificate checks when it is set. `tokio-postgres-raw` also reads `DB_ADDR`. Add `--p3` for `wasi-pg-client`.

`--allow` names the one address the component may connect to. The host denies every other TCP connection, all UDP, and name lookups. It prints one `host: socket check` line for each check. The last line gives the time to compile the component, the time to instantiate it, and the time the guest ran, in milliseconds.

## Read the table

| Column | Meaning |
| --- | --- |
| `default_cfg` | Whether the probe built without `--cfg tokio_unstable` |
| `tokio_unstable` | Whether the probe built with it |
| `wasm_kib` | The size of the release component |
| `plain_run` | The result of the run without TLS |
| `tls_run` | The result of the TLS run. `tls=` gives what the server reports for the session, `true` from PostgreSQL's `pg_stat_ssl` or the cipher from MariaDB's `Ssl_cipher`. |
| `untrusted_ca_run` | The result of the TLS run that trusts the wrong CA. `FAIL: UnknownIssuer` is the expected result. |

Below the table the script prints up to two distinct build errors for each failed build, the results of the builds with the WASI libc headers, the allow list check, the timings of each run, the size of the host binary, and the crate versions from each `Cargo.lock`. The logs are in `logs/` and `runs/` under the output folder.
