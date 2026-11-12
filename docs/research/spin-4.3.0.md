# Spin 4.3.0 reference

This page records how Spin, a Rust host for WebAssembly components, gives components outbound PostgreSQL, MySQL, and HTTP through interfaces that the host implements. It covers the WIT for those interfaces, how the host enforces `allowed_outbound_hosts`, how variables and secrets reach a component, how the manifest and the WIT are versioned, and the resource limits. deployarr uses it to judge the "host-provided capability" design for the SQL order lookup add-on, where the server owns the database drivers and the add-on component calls them. The [Wasmtime prototype page](wasmtime-50.0.1.md) tests the other design, with the drivers inside the component.

## Sources

Each source has a short label. Citations name the label, then a path and a line range.

| Label | Source | Version | How it was obtained |
| --- | --- | --- | --- |
| spin | https://github.com/spinframework/spin | tag `v4.3.0`, commit `83b512fdf5a11783fde1d8546117b672fe9dd46e`, tagged 2026-11-05 | `git clone --depth 1 --branch v4.3.0 --filter=blob:none` into `$SCRATCH/research/spin/src` |
| sdk | https://github.com/spinframework/spin-rust-sdk | tag `v8.0.0`, commit `acf428a09f516dd1b9fe6ddfd33038f5b0f3e7e0`, tagged 2026-11-05 | the same clone command, into `$SCRATCH/research/spin/sdk` |
| main | `crates/factor-outbound-pg/src/host.rs` on the `main` branch of spin | fetched 2026-11-12 | `raw.githubusercontent.com`, read only to check whether one finding is fixed |
| crates | `wasmtime` 50.0.1 and `wasmtime-wasi` 50.0.1 | 50.0.1 | the Cargo registry under `~/.cargo/registry/src/` |
| reqwest | `reqwest` 0.12.28, the version in spin's `Cargo.lock` | 0.12.28 | `https://static.crates.io/crates/reqwest/reqwest-0.12.28.crate`, unpacked into `$SCRATCH/fc-spin/rq` |

`$SCRATCH` is a scratch directory outside the repository, as in [Reproduce the MDT research](../../AGENTS.md#reproduce-the-mdt-research). `git ls-remote --tags` on 2026-11-12 listed `v4.3.0` as the newest Spin tag and `v8.0.0` as the newest SDK tag.

## Runtime and platforms

- Spin 4.3.0 depends on `wasmtime`, `wasmtime-wasi`, and `wasmtime-wasi-http` 50.0.1. It turns on `p3` for `wasmtime-wasi`, and `p3` and `component-model-async` for `wasmtime-wasi-http` (spin `Cargo.toml:209-212`). This is the same `wasmtime` release as the deployarr prototype.
- Its default engine configuration turns on epoch interruption, the component model, and component model async (spin `crates/core/src/lib.rs:80-85`). The prototype page quotes the `wasmtime` doc comment that calls async support incomplete. Spin ships it on by default anyway.
- Spin's release workflow builds a Windows amd64 binary and installs OpenSSL for it through vcpkg (spin `.github/workflows/release.yml:63-70`, `:121`, `.github/actions/spin-ci-dependencies/action.yml:146-152`).
- The Rust SDK 8.0.0 generates its bindings with the `wasip3` crate's `wit_bindgen` (sdk `crates/spin-sdk/src/lib.rs:79`). Its PostgreSQL world imports `spin:postgres/postgres@4.2.0` (sdk `crates/spin-sdk/wit/sdk.wit:23-25`).

## Database interfaces

The `platform` world that a 4.3.0 guest imports lists `spin:mysql/mysql@3.0.0`, `spin:postgres/postgres@3.0.0`, `spin:postgres/postgres@4.2.0`, `spin:redis/redis@3.0.0`, and `spin:sqlite/sqlite@3.1.0`. It also includes `fermyon:spin/platform@2.0.0`, which carries the older interfaces (spin `wit/world.wit:15-36`).

### PostgreSQL 4.2.0

| Item | Shape | Citation |
| --- | --- | --- |
| Error | `variant error { connection-failed(string), bad-parameter(string), query-failed(query-error), value-conversion-failed(string), other(string) }` | spin `wit/deps/spin-postgres@4.2.0/postgres.wit:5-11` |
| Query error | `text(string)` or `db-error(db-error)`. `db-error` has `as-text`, `severity`, `code`, `message`, `detail`, and `extras` as name and value pairs. | `postgres.wit:13-29` |
| Column type | `db-data-type`, 24 cases from `boolean` to `interval`, plus `other(string)` | `postgres.wit:32-58` |
| Value | `db-value` with the same 24 cases, carrying data, plus `db-null` and `unsupported(list<u8>)`. Dates are tuples. `decimal` is a string. | `postgres.wit:61-91` |
| Parameter | `parameter-value`, the same cases as `db-value` without `unsupported` | `postgres.wit:94-123` |
| Result | `record row-set { columns: list<column>, rows: list<row> }`, where `row` is `list<db-value>` and `column` is a name and a type | `postgres.wit:132-144` |
| Builder | `resource connection-builder` with `constructor(address)`, `set-ca-root(certificate)`, `build`, and `build-async`. `set-ca-root` takes PEM text. | `postgres.wit:152-159`, spin `crates/factor-outbound-pg/src/client.rs:45-51`, `host.rs:171-188` |
| Connection | `open(address)`, `query(statement, params)`, and `execute(statement, params) -> u64`, each with an `async` twin. `query-async` returns a column list, a `stream<row>`, and a `future` for the final error. `open-async` and `execute-async` return what their twins return. | `postgres.wit:161-183` |

- The `address` is a connection string that the guest supplies. The host parses it as `tokio_postgres::Config` (spin `crates/factor-outbound-pg/src/allowed_hosts.rs:32-34`). It carries the user name and password, so the guest holds the credentials (inferred from the WIT, which has no other way to pass them).
- The interface has no transaction call and no read-only flag (`postgres.wit:161-183`). A guest can send `BEGIN READ ONLY` through `execute` on one connection, because each connection resource holds one pooled client (inferred from `crates/factor-outbound-pg/src/lib.rs:112-115`).
- The host maps a server error to `db-error` with the SQLSTATE in `code` (spin `crates/factor-outbound-pg/src/client.rs:194-208`).

### MySQL 3.0.0

- The error variant has the same five cases, but `query-failed` holds a plain string (spin `wit/deps/spin-mysql@3.0.0/mysql.wit:5-11`).
- Values and parameters cover signed and unsigned integers, floats, `str`, `binary`, and `db-null`. There are no date, time, or decimal cases (`mysql.wit:32-66`).
- All three functions are `async func`, so a guest needs component model async to call them (`mysql.wit:78-87`).
- TLS is on only when the URL has an `ssl-mode` or `sslmode` parameter whose value is not `disabled`. Without the parameter, TLS is off. The host then uses `SslOpts::default()`, so the interface has no way to add a private CA (spin `crates/factor-outbound-mysql/src/client.rs:343-378`).

### Host implementation

- PostgreSQL uses `tokio-postgres` with `deadpool-postgres`. TLS uses `native-tls`, and the builder's CA goes in through `add_root_certificate` (spin `crates/factor-outbound-pg/src/client.rs:101-137`). The system roots likely stay trusted alongside it (inferred from `native-tls` defaults, not checked here). MySQL uses `mysql_async` 0.35 with `native-tls-tls` (spin `crates/factor-outbound-mysql/Cargo.toml`, `[dependencies]`).
- Each component gets its own PostgreSQL client factory. A factory keeps up to 16 pools, keyed by address and CA hash, with up to 64 connections each (spin `crates/factor-outbound-pg/src/lib.rs:54-56`, `client.rs:19-22`, `:85-96`). MySQL builds a new pool for every `open` (spin `crates/factor-outbound-mysql/src/client.rs:53-64`, `host.rs:41`).
- `open` takes a permit from a semaphore before it connects. The limit comes from `max_connections` in the runtime configuration when it is set, and an app-wide `max_total_connections`, when set, caps all connection types (spin `crates/factor-outbound-pg/src/host.rs:23-54`, `crates/factor-outbound-networking/src/lib.rs:247-274`).
- A buffered `query` stops with "query result exceeds limit" when the rows pass `MAX_HOST_BUFFERED_BYTES`, which is 128 MiB (spin `crates/factor-outbound-pg/src/client.rs:239-263`, `crates/world/src/lib.rs:59-64`).
- The PostgreSQL code logs the parsed config at debug level, because its `Debug` output hides the password (spin `crates/factor-outbound-pg/src/client.rs:109-111`). The MySQL code logs the raw address at debug level (spin `crates/factor-outbound-mysql/src/client.rs:57`). The allow-list check also logs the URL it receives at debug level, and MySQL passes it the whole address (spin `crates/outbound-networking-config/src/allowed_hosts.rs:43-44`, `crates/factor-outbound-mysql/src/host.rs:72-74`). A MySQL password therefore reaches the debug log (inferred from these lines, not run). A denied MySQL address also goes into the error text that the guest receives, `address {address} is not permitted` (spin `crates/factor-outbound-mysql/src/host.rs:37`).

## allowed_outbound_hosts

### Syntax

- Each entry is `scheme://host[:port]` (spin `crates/manifest/src/schema/v2/component.rs:66-77`). The scheme is `*` or letters only (`allowed_hosts.rs:229-243`). The host is a name, `*`, `*.domain`, an IP address, a CIDR range, or `self` (spin `crates/outbound-networking-config/src/allowed_hosts.rs:271-310`). The port is a number, `a..b`, or `*` (`allowed_hosts.rs:367-423`). A range is a Rust `Range`, so `a..b` excludes `b` (`allowed_hosts.rs:419`, `:430`).
- A missing port takes the scheme's default: 5432 for `postgres`, 3306 for `mysql`, 6379 for `redis`, 1883 for `mqtt`, 80 and 443 for HTTP. Any other scheme needs an explicit port (`allowed_hosts.rs:367-372`, `:436-446`).
- The parser rejects `insecure:allow-all` with a message that points to `*://*:*` (`allowed_hosts.rs:538-543`).
- An entry can use `{{ variable }}` templates. The host resolves them for each instance before the first check (spin `crates/factor-outbound-networking/src/lib.rs:146-162`). The host drops an entry that resolves to an empty string, `http://`, or `https://`, which supports optional endpoints. It keeps an entry such as `postgres://{{ db_host }}` with an empty `db_host` and parses it (`allowed_hosts.rs:483-498`).
- A runtime configuration can add `block_networks`, with CIDR ranges or `private` (spin `crates/factor-outbound-networking/src/runtime_config/spin.rs:221-253`, `crates/outbound-networking-config/src/blocked_networks.rs:20-57`). The manifest has no such field, so a component cannot override it (inferred).

### Host-provided connections

- PostgreSQL checks each host and port in the parsed config with `check_url(url, "postgres")` before it opens a pool. A denied address returns `connection-failed("address postgres://... is not permitted")` (spin `crates/factor-outbound-pg/src/allowed_hosts.rs:24-70`).
- MySQL checks the whole address with scheme `mysql` (spin `crates/factor-outbound-mysql/src/host.rs:21-40`, `:72-74`).
- The checks run on the host name in the string. The host's driver then resolves the name itself (inferred from `allowed_hosts.rs:320-345`, which compares names and never resolves).
- `block_networks` does not apply to these connections. Neither `crates/factor-outbound-pg` nor `crates/factor-outbound-mysql` refers to `BlockedNetworks` or `blocked_networks`, while `crates/factor-outbound-redis` and outbound HTTP do (a search of spin `crates/`, and `crates/factor-outbound-http/src/wasi.rs:623-624`).
- The synchronous `connection-builder.build` calls `open_connection` without `ensure_address_allowed` (spin `crates/factor-outbound-pg/src/host.rs:190-196`). `open` and `build-async` do check (`host.rs:207-217`, `:428-444`). The `main` branch on 2026-11-12 has the same `build` body (main). A guest that calls `build` therefore reaches any PostgreSQL host (inferred from the code, not run).

### Raw wasi-sockets

- The host turns on TCP and UDP for a component only when its allow list is not empty (spin `crates/factor-outbound-networking/src/lib.rs:183`, `crates/factor-wasi/src/lib.rs:526-528`).
- Each socket use goes through `socket_addr_use_allowed`. The host denies TCP listen, TCP accept, and UDP receive. It allows TCP bind only to an unspecified address with port 0. It checks a TCP connect as `tcp://<ip>:<port>`, and a UDP bind or send as `udp://<ip>:<port>`, against the allow list and then against `block_networks` (spin `crates/factor-outbound-networking/src/lib.rs:330-367`, `crates/factor-wasi/src/lib.rs:552-571`).
- The check sees an IP address, and a host name entry never matches an IP address (`allowed_hosts.rs:320-345`). So a raw-socket component needs IP or CIDR entries, such as `tcp://10.0.0.5:1433` (inferred).
- Spin never calls `allow_ip_name_lookup` (a search of spin `crates/` finds no call), and `wasmtime-wasi` keeps name lookup off by default (crates `wasmtime-wasi-50.0.1/src/ctx.rs:408-414`). A component that opens its own sockets cannot resolve a host name (inferred).

## Variables and secrets

- The manifest declares app variables with `description`, `required`, `default`, and `secret` (spin `crates/manifest/src/schema/common.rs:13-44`). A component cannot read them directly. It declares its own variables, whose values are templates over app variables (spin `crates/manifest/src/schema/v2.rs:35-41`, `crates/manifest/src/schema/v2/component.rs:28-35`).
- A guest calls `get: async func(name) -> result<string, error>` in `spin:variables/variables@3.0.0`. The errors are `invalid-name`, `undefined`, `provider`, and `other` (spin `wit/deps/spin-variables@3.0.0/variables.wit:3-20`). The host resolves the name only within the calling component's variables (spin `crates/factor-variables/src/host.rs:13-24`, `crates/expressions/src/lib.rs:50-54`). `wasi:config/store` gives the same values (`host.rs:70-90`).
- Providers are tried in order, and the first value wins (spin `crates/expressions/src/lib.rs:116-123`). The runtime configuration lists them in `[[variables_provider]]` with `type` set to `azure_key_vault`, `static`, `vault` (alias `open_bao`), or `env`. An `env` provider with prefix `SPIN_VARIABLE` is always appended (spin `crates/runtime-config/src/variables.rs:11-45`, `crates/variables-env/src/lib.rs:28`).
- The Vault provider reads a KV v2 secret at `mount` and `prefix/key` and returns its `value` field. It takes the Vault token as plain text from the runtime configuration and builds a new client for each read (spin `crates/variables-vault/src/lib.rs:13-53`).
- The Azure Key Vault provider takes a client secret from the runtime configuration, or uses the Azure SDK's environment credential chain (spin `crates/variables-azure/src/lib.rs:18-24`, `:56-75`).
- `secret = true` is copied into the locked app, whose doc comment says the value "shouldn't be logged" (spin `crates/loader/src/local.rs:715-723`, `crates/locked-app/src/locked.rs:373-376`). No other code in `crates/` reads the field (inferred from a search for `.secret`).

## Manifest and lock versions

- `spin.toml` must set `spin_manifest_version = 2` (spin `crates/manifest/src/schema/v2.rs:28-32`). The loader still reads version 1 and converts it (spin `crates/manifest/src/lib.rs:23-56`).
- The locked app, which the host runs, writes `spin_lock_version = 0` unless it uses `must_understand` or `host_requirements`, and 1 if it does (spin `crates/locked-app/src/locked.rs:126-140`). `host_requirements` names the features `local_service_chaining` and `middleware` as `required` or `optional`. `must_understand` tells a host that it must reject the app if it cannot meet them (`locked.rs:19-46`, `:48-65`, `:124`).

## WIT evolution

- Spin 2 put every interface in one package, `fermyon:spin@2.0.0` (spin `wit/deps/spin@2.0.0/world.wit:16-27`). From Spin 3, new versions ship as one package per capability, such as `spin:postgres@3.0.0` and `spin:mysql@3.0.0` (spin `wit/deps/spin@3.0.0/world.wit:10-15`, `wit/world.wit:27-35`).
- PostgreSQL shows the path. The unversioned interface passes the address on every call (spin `wit/deps/spin@unversioned/postgres.wit:14-18`). 2.0.0 adds a `connection` resource (`wit/deps/spin@2.0.0/postgres.wit:5-14`). 3.0.0 adds date and time values (`wit/deps/spin-postgres@3.0.0/postgres.wit:32-51`). 4.1.0 adds `connection-builder` and 4.2.0 adds the async functions, each marked `@since` in one file (`wit/deps/spin-postgres@4.2.0/postgres.wit:152-182`).
- The repository keeps older packages under `wit/deps/`, such as `spin-postgres@3.0.0`, `@4.0.0`, `@4.1.0`, and `@4.2.0`. The host's `bindgen!` includes the worlds `fermyon:spin/host`, `fermyon:spin/platform@2.0.0` and `@3.0.0`, and `spin:up/platform@3.2.0`, `@3.4.0`, and `@4.3.0`. It leaves out `spin:up/platform@4.0.0` and `@4.1.0`, although both are under `wit/deps/` (spin `crates/world/src/lib.rs:6-19`, `wit/deps/spin@4.0.0/world.wit:1`, `wit/deps/spin@4.1.0/world.wit:1`). The PostgreSQL factor links the unversioned, 2.0.0, 3.0.0, and 4.2.0 interfaces (spin `crates/factor-outbound-pg/src/lib.rs:36-46`). The old versions convert their parameters and call the 4.2.0 code (spin `crates/factor-outbound-pg/src/host.rs:84-92`, `:462-480`, `:550-590`).
- A guest built against 4.0.0 or 4.1.0 links to 4.2.0, because the `wasmtime` linker resolves an import to a semver-compatible newer version (crates `wasmtime-50.0.1/src/runtime/component/linker.rs:36-49`). That rule assumes a released interface never changes.
- A component dependency gets no capabilities by default. Spin composes a deny-all adapter in front of it, and `inherit_configuration` grants named ones such as `allowed_outbound_hosts` (spin `docs/content/sips/023-fine-grained-capability-inheritance.md:23-71`, `crates/compose/src/lib.rs:413-428`).

## Resource limits

- `--max-instance-memory`, or `SPIN_MAX_INSTANCE_MEMORY`, sets a store memory limit for each instance (spin `crates/runtime-factors/src/lib.rs:120-122`, `crates/trigger/src/cli/max_instance_memory.rs:5-26`).
- The pooling allocator is on by default when a probe can reserve 2^42 bytes of address space, and `SPIN_WASMTIME_POOLING` set to `1` or `0` overrides the probe (spin `crates/core/src/lib.rs:223-253`). It sizes for 1,000 instances (`SPIN_MAX_INSTANCE_COUNT`) and a 10 MiB instance size (`SPIN_WASMTIME_INSTANCE_SIZE`). Its maximum memory size is a fixed 4 GiB (spin `crates/core/src/lib.rs:92-182`).
- `Store::set_deadline` converts a wall-clock deadline to epoch ticks of 10 ms (spin `crates/core/src/store.rs:28-40`, `crates/core/src/lib.rs:29-30`). The HTTP trigger calls it only when `InstanceReuseConfig` holds a `request_deadline` (spin `crates/trigger-http/src/server.rs:68-75`). `spin up` leaves that field empty, and only `InstanceReuseConfig::single_use_with_request_deadline`, which nothing in the repository calls, sets it (spin `crates/trigger-http/src/lib.rs:261-275`, `:298-314`).
- The HTTP trigger's `--request-timeout` affects only WASI 0.3 components, and the default is no timeout (spin `crates/trigger-http/src/lib.rs:44`, `:114-126`). The WASI 0.3 worker starts a `tokio` timer for it on each request, not an epoch deadline (spin `crates/trigger-http/src/server.rs:741-745`, `:784-788`).
- Spin does not use fuel (a search of spin `crates/` for `consume_fuel` and `set_fuel` finds nothing).

## SQL Server

Spin has no SQL Server interface. No WIT package, crate, or dependency names SQL Server, MSSQL, TDS, or `tiberius` (a case-insensitive search of the spin tree at the tag finds only a libSQL comment and a variable named `tds`, and `Cargo.lock` has no `tiberius` entry). A Spin component that wants SQL Server must bring its own driver over raw sockets, with an IP allow list and no DNS (inferred from the raw wasi-sockets section).

## Outbound HTTP for Microsoft Graph

- The guest imports `wasi:http/outgoing-handler@0.2.6`, or `wasi:http/client` at `0.3.0-rc-2026-03-15` or `0.3.2` (spin `wit/world.wit:20-22`).
- The host checks the request URI with `check_url(uri, "https")` and returns `HttpRequestDenied` on a miss (spin `crates/factor-outbound-http/src/wasi.rs:417-438`). Graph needs entries such as `https://graph.microsoft.com` and `https://login.microsoftonline.com` (inferred from the syntax above).
- The host resolves the name with `tokio::net::lookup_host`, drops blocked addresses, and connects with a timeout (spin `crates/factor-outbound-http/src/wasi.rs:600-640`). Connect, first-byte, and between-bytes timeouts default to 600 seconds unless the guest sets them (`wasi.rs:58`, `:125-135`).
- TLS uses `rustls` with the OS verifier and the `webpki` roots by default (spin `crates/factor-outbound-networking/src/tls.rs:108-141`, `:169-177`). A `[[client_tls]]` entry per component and host can set `ca_roots_file`, `ca_use_platform_roots`, `ca_use_webpki_roots`, and a client certificate (spin `crates/factor-outbound-networking/src/runtime_config/spin.rs:200-211`). An entry with only `ca_roots_file` turns the OS roots off. Keeping the OS roots as well needs `ca_use_platform_roots = true` (spin `crates/factor-outbound-networking/src/runtime_config/spin.rs:141-151`).
- The `wasi:http` path has no forward proxy setting. A search of spin `crates/` for `HTTPS_PROXY`, `http_proxy`, and `https_proxy` finds nothing, and the connector opens a direct `TcpStream` (`wasi.rs:629`).
- The older `fermyon:spin/http` interface sends requests through a `reqwest` client and does not call `no_proxy` (spin `crates/factor-outbound-http/src/spin.rs:115-122`). `reqwest` 0.12.28 adds the system proxy matcher by default (reqwest `src/async_impl/client.rs:309`, `:419-421`, `src/proxy.rs:515-526`). So that interface likely honors proxy environment variables (inferred, not run).

## What changes in deployarr

- Put the SQL drivers in the server and give the SQL add-on a host interface, as Spin does. This removes the `tiberius` build problem from the [prototype](wasmtime-50.0.1.md), because `tiberius` then builds for Windows and Linux natively. Spin offers no SQL Server code to copy, so deployarr writes that mapping itself (SQL Server section).
- Do not copy Spin's connection-string argument. Spin's guest builds the address, password included (PostgreSQL section). Let the guest name a [data source](../architecture.md#data-sources) ID and a named query. The server keeps the credentials encrypted, as the [Server](../architecture.md#server) section requires, and the component never sees them.
- Copy the typed shapes. Use a `parameter-value` variant for bound parameters, a column list with types, rows as lists of a `db-value` variant with `db-null`, and a structured error with the SQLSTATE or engine error number. Offer `query` only, with no `execute`, and run it in a read-only transaction where the engine allows, matching [Data sources](../architecture.md#data-sources).
- Use one engine-neutral interface instead of Spin's one package per engine. Spin's MySQL and PostgreSQL types differ (MySQL section), and order lookup needs only strings, integers, dates, and nulls.
- Run the allow-list check inside the one function that opens a connection, not in each WIT entry point. Spin's `build` path skips the check because the check sits in the callers (Host-provided connections section). Test every WIT version's entry points against a denied host.
- Resolve the database host name in the server and check each resolved address against the block list before connecting. Spin applies `block_networks` to HTTP and sockets but not to its PostgreSQL and MySQL drivers (Host-provided connections section).
- Copy templated allow-list entries, and drop an entry whose variable is empty for every scheme. An add-on manifest can declare `postgres://{{ db_host }}`, and the admin fills `db_host` on the settings page. Spin drops an empty entry only for `http://` and `https://` (Syntax section).
- If an add-on opens raw sockets, follow Spin's rules. Deny listen and accept, allow bind only to port 0, and check each connect by IP. Decide name lookup explicitly. Spin leaves it off, which forces IP entries (Raw wasi-sockets section).
- Enforce `secret`. Spin marks secrets but nothing reads the mark, and its MySQL path can log a password at debug level (Variables and Host implementation sections). deployarr's add-on settings schema needs a secret flag that the server honors in storage, logs, traces, and the UI. Add a test that greps the debug log for a known test password.
- Copy the WIT versioning. Version each capability as its own package. Add functions in a minor version with `@since` and rely on `wasmtime` semver linking. Keep every released package in the server and convert old calls to the newest code (WIT evolution section).
- Copy `host_requirements` and `must_understand` for the add-on package manifest. An older server then refuses an add-on that needs a feature it lacks, instead of failing at instantiation (Manifest and lock versions section).
- Set limits per add-on. Use a store memory limit, an epoch deadline for each call, a connection semaphore per add-on, and a row byte cap far below Spin's 128 MiB, because an order lookup returns one row (Resource limits section).
- For the Autopilot add-on, provide outbound HTTP through the host with an allow list of the Graph and login hosts, host-side DNS, and `rustls` with the OS roots plus an admin CA. Make keeping the OS roots the default when an admin adds a CA, unlike Spin's `ca_roots_file`. Add the explicit forward proxy setting that Spin's `wasi:http` path lacks, because a managed network can require one for Graph traffic (Outbound HTTP section). On a network with no internet access, the add-on must report Graph as unreachable.

## Open questions

- Should the SQL capability be generic (`query(source, sql, params)`) or specific to order lookup (`lookup(source, query-id, params)`)? The specific form keeps the SQL text in the server, where the admin edits it, and the add-on adds little beyond settings pages.
- If the drivers live in the server, the base binary carries `sqlx` and `tiberius` even when no SQL add-on is installed. Is that acceptable, or should the capability ship as a separate native host module?
- Spin turns on component model async in production while the `wasmtime` docs call it incomplete. Should deployarr's interfaces use `async func` and `stream<row>`, or stay synchronous until `wasmtime` marks the feature complete?
- Windows authentication to SQL Server from a host that runs as a Linux service is not covered by any source here.
