# HashiCorp go-plugin 1.8.0 reference

This page records how HashiCorp go-plugin 1.8.0 runs plugins as separate processes, negotiates versions, secures the connection, and shuts plugins down. It also records how Terraform evolves its provider protocol on top of go-plugin, and which Rust crates speak the go-plugin protocol. The README names Packer, Terraform, Nomad, Vault, Boundary, and Waypoint as users (`go-plugin/README.md:4-9`). Grafana's backend plugins also speak its protocol (see [Rust implementations](#rust-implementations)). deployarr plans optional add-ons, such as SQL order lookup, that run beside `deployarr-server`. Use this page when you design how the server launches, talks to, versions, and trusts an add-on process.

## Sources

Each path on this page starts with the clone it comes from. `go-plugin/client.go:392-440` means `client.go` in the go-plugin clone, lines 392 to 440.

| Clone | Repository | Tag | Commit | Commit date |
| --- | --- | --- | --- | --- |
| `go-plugin/` | `https://github.com/hashicorp/go-plugin` | `v1.8.0` | `155dcddc94873a285e14b7fa24b2f6ab6139668e` | 2026-04-29 |
| `terraform/` | `https://github.com/hashicorp/terraform` | `v1.17.2` | `ae094fc0ee6d4f3d2a990b40ac48a6d2dbaeb53e` | 2026-11-11 |
| `terraform-plugin-go/` | `https://github.com/hashicorp/terraform-plugin-go` | `v0.31.0` | `09a1181b051c53a3700401895ae281afbc91f0fc` | 2026-03-10 |
| `vault/` | `https://github.com/hashicorp/vault` | `v2.2.2` | `c56e7e670bd8c79104c27b6c10138226538a6db2` | 2026-11-11 |

- Each clone used `git clone --depth 1 --branch <tag> --filter=blob:none`. The Terraform and Vault clones were sparse. Terraform kept `docs/plugin-protocol`, `internal/plugin`, `internal/plugin6`, `internal/command`, and `internal/getproviders`. Vault kept `sdk/helper/pluginutil`, `sdk/plugin`, and `vault/plugincatalog`.
- Each tag was the highest stable tag from `git ls-remote --tags` on 2026-11-12. go-plugin `v1.8.0` is a lightweight tag, so its date is the commit date.
- go-plugin's `CHANGELOG.md` stops at `v1.7.0` (`go-plugin/CHANGELOG.md:1`). The 1.8.0 changes come from `git log v1.7.0..v1.8.0` after `git fetch --unshallow`. They are a pidfd leak fix in `_pidAlive` (`155dcdd`), a race fix in the pipe wait group (`31383c0`), a logging level change (`d662936`), and example, CI, license header, and build tag updates.
- Older Terraform proto files were fetched from `https://raw.githubusercontent.com/hashicorp/terraform/v1.0.0/docs/plugin-protocol/<file>`.
- Rust crates were found with the crates.io search API (`https://crates.io/api/v1/crates?q=go-plugin` and `?q=hashicorp%20plugin`) on 2026-11-12. Each crate's source came from `https://static.crates.io/crates/<name>/<name>-<version>.crate`. Paths into a crate start with `<name>-<version>/`.

## Package format and manifest

- go-plugin defines no package format and no manifest. A plugin is an executable. The README says to "put the binary in a location where the host will find it" (`go-plugin/README.md:96-98`). Each host decides discovery.
- The only compatibility data is the handshake, described under [Host-to-add-on API](#host-to-add-on-api). There is no minimum host version and no semantic version check. The README lists semantic versioning as roadmap work (`go-plugin/README.md:139-144`).
- go-plugin does not check the binary's architecture before launch. When the handshake fails, `Diagnose` prints the binary's ELF, Mach-O, or PE machine type next to the host's (`go-plugin/client.go:839-849`, `go-plugin/internal/cmdrunner/cmd_runner.go:26-31`, `go-plugin/internal/cmdrunner/cmd_runner.go:127-128`, `go-plugin/internal/cmdrunner/notes_windows.go:19-45`).

Terraform adds a package format on top, through its registry:

- Each provider release ships one archive per OS and architecture. The registry's download response carries `protocols`, `os`, `arch`, `filename`, `download_url`, `shasum`, `shasums_url`, `shasums_signature_url`, and `signing_keys` (`terraform/internal/getproviders/registry_client.go:217-232`).
- Terraform rejects a response whose `os` or `arch` differs from the requested platform (`terraform/internal/getproviders/registry_client.go:279-281`).
- `protocols` lists the plugin protocol versions a release speaks. Terraform checks them against `SupportedPluginProtocols`, which is `>= 5, <7` at this tag. With no match, it fails at install time and suggests the closest compatible release (`terraform/internal/getproviders/registry_client.go:61`, `terraform/internal/getproviders/registry_client.go:240-277`). The protocol README shows the resulting messages (`terraform/docs/plugin-protocol/README.md:101-136`).

## Install, upgrade, disable, and uninstall

- go-plugin has no install, upgrade, disable, or uninstall step. It has no UI and no catalog. The host owns all of it.
- Terraform installs providers during `terraform init` and checks the protocol before it downloads (`terraform/internal/getproviders/registry_client.go:253-277`). A manually placed plugin gets only a generic incompatibility error, because Terraform has no registry data for it (`terraform/docs/plugin-protocol/README.md:124-136`).
- Vault registers each plugin in a catalog with its SHA-256. For a plugin that runs as a local executable, not a container image, the run config carries that hash into go-plugin's `SecureConfig` at every launch (`vault/sdk/helper/pluginutil/runner.go:76`, `vault/sdk/helper/pluginutil/run_config.go:163-168`).

## Runtime model

### Process and transport

- Each plugin is a child process. The host talks to it over RPC on a local socket (`go-plugin/README.md:78-85`). The README says plugins over a real network "are not supported" (`go-plugin/README.md:11-13`).
- The plugin listens on a Unix socket on Linux and macOS. On Windows it listens on TCP at `127.0.0.1` (`go-plugin/server.go:528-534`). The TCP port comes from `PLUGIN_MIN_PORT` to `PLUGIN_MAX_PORT`, which the host sets to 10000 to 25000 by default (`go-plugin/server.go:536-576`, `go-plugin/client.go:393-396`).
- A Unix socket is owner-only unless the host sets a group through `PLUGIN_UNIX_SOCKET_GROUP` (`go-plugin/server.go:599-606`).
- Two RPC protocols exist. `netrpc` is Go's `net/rpc` over a `yamux` multiplexer (`go-plugin/rpc_client.go:64-66`). `grpc` uses HTTP/2 for multiplexing. A host must opt in to gRPC through `AllowedProtocols`. The default allows only `netrpc` (`go-plugin/client.go:208-219`, `go-plugin/client.go:413-415`).
- The gRPC client sets the receive and send size limits to `math.MaxInt32` (`go-plugin/grpc_client.go:39-41`). terraform-plugin-go lowers its server limit to 256 MiB (`terraform-plugin-go/tfprotov6/tf6server/server.go:89`, `terraform-plugin-go/tfprotov6/tf6server/server.go:285-288`).

### Start

- `Client.Start` builds the child's environment, checks `SecureConfig`, sets up AutoMTLS, starts the process, and waits for one handshake line on stdout (`go-plugin/client.go:580-948`).
- The start timeout defaults to one minute (`go-plugin/client.go:398-400`, `go-plugin/client.go:824-833`). If the process exits first, `Start` returns "plugin exited before we could connect" (`go-plugin/client.go:832-833`).
- A deferred function kills the process if any later start step fails (`go-plugin/client.go:740-751`).
- The child inherits the host's whole environment unless `SkipHostEnv` is set. go-plugin's own variables are appended after it (`go-plugin/client.go:655-658`). The child also shares the host's stdin (`go-plugin/client.go:659`).
- A plugin ignores `SIGINT`, so a Ctrl-C in a terminal reaches the host only and the host controls shutdown (`go-plugin/server.go:460-473`).

### Crash handling, health, and restart

- A plugin crash does not crash the host. The README states this as the main benefit of separate processes (`go-plugin/README.md:89-90`).
- A goroutine waits on the child, logs the exit, and sets `exited` (`go-plugin/client.go:765-791`). `Exited()` reports it (`go-plugin/client.go:477-482`). The context passed to `GRPCClient` is cancelled when the process exits (`go-plugin/plugin.go:42-45`).
- The gRPC server always registers the standard gRPC health service with service name `plugin` set to `SERVING` (`go-plugin/grpc_server.go:24-26`, `go-plugin/grpc_server.go:78-82`). It also registers gRPC reflection (`go-plugin/grpc_server.go:84-85`).
- `Ping()` calls that health check (`go-plugin/grpc_client.go:126-134`). go-plugin never calls `Ping()` itself. The only definitions and callers are the interface and its two implementations (`go-plugin/protocol.go:46-47`, `go-plugin/grpc_client.go:127`, `go-plugin/rpc_client.go:170`). Periodic health checks are the host's job.
- go-plugin never restarts a plugin. A `Client` starts once and "cannot be started again, even if it was killed" (`go-plugin/client.go:574-579`). A host that wants a restart creates a new `Client`.
- go-plugin has no code that ties the plugin's lifetime to the host's. A search of v1.8.0 for a parent-death signal, a process group, or a Windows job object finds nothing. A plugin can outlive a host that crashes (inferred from that absence). Nomad relies on this to leave executor processes running across agent restarts (`df94fce` commit message).

### Shutdown and kill

- `Kill` closes the RPC client, which sends the `GRPCController.Shutdown` RPC and closes the connection (`go-plugin/client.go:526-549`, `go-plugin/grpc_client.go:105-109`). The plugin stops its gRPC server on that call (`go-plugin/grpc_controller.go:20-26`). The code carries a `TODO` that `GracefulStop` does not work, so it calls `Stop` (`go-plugin/grpc_controller.go:23-24`).
- `Kill` then waits 2 seconds for the process to exit. After that it kills the process (`go-plugin/client.go:551-567`). The kill is `os.Process.Kill` on the direct child only (`go-plugin/internal/cmdrunner/cmd_runner.go:88-99`). go-plugin does not track grandchildren (inferred from that call).
- `Kill` is safe to call more than once (`go-plugin/client.go:492-498`). It removes the plugin's Unix socket directory (`go-plugin/client.go:511-517`).
- A client created with `Managed: true` joins a global list. `CleanupClients` kills every managed client in parallel and "must only be called once" (`go-plugin/client.go:180-185`, `go-plugin/client.go:359-383`). Terraform launches providers as managed clients (`terraform/internal/command/meta_providers.go:463-473`).

### Reattach

- `ReattachConfig` holds the protocol, protocol version, address, and PID of a running plugin (`go-plugin/client.go:296-315`). The README describes it as the way to upgrade a host while a plugin keeps running (`go-plugin/README.md:68-72`).
- Reattach cannot be combined with `SecureConfig` or with gRPC broker multiplexing (`go-plugin/client.go:606-612`). AutoMTLS cannot reattach (`go-plugin/client.go:249`).
- Reattach finds the PID and dials the address (`go-plugin/internal/cmdrunner/cmd_reattach.go:17-38`). On Unix, `FindProcess` never fails, so the dial is the only liveness test. go-plugin waits on a reattached process by polling its PID once a second (`go-plugin/internal/cmdrunner/cmd_reattach.go:53`, `go-plugin/internal/cmdrunner/process.go:13-25`).

### Pluggable runners

- `RunnerFunc` lets a host replace the subprocess runner, for example to run the plugin in a container (`go-plugin/client.go:166-171`, `go-plugin/runner/runner.go:14-68`). An `AddrTranslator` maps socket paths between the container and the host (`go-plugin/runner/runner.go:57-68`).
- Vault uses `RunnerFunc` for containerized plugins and gives them no host environment variables (`vault/sdk/helper/pluginutil/run_config.go:137-140`, `vault/sdk/helper/pluginutil/run_config.go:169-175`).
- go-plugin sets no CPU, memory, or file limits. A host can add them only through a custom runner (inferred from `go-plugin/runner/runner.go:14-68`, which has no limit fields).

### Logging and stdio

- The plugin's stderr is the log channel. The plugin's default logger writes JSON `hclog` lines to stderr at `Trace` level (`go-plugin/server.go:273-281`).
- The host reads stderr line by line with a 64 KiB buffer by default (`go-plugin/client.go:75-76`, `go-plugin/client.go:425-427`). A JSON line is re-logged at its own level with its key-value pairs. A plain line gets its level from a `[TRACE]`, `[DEBUG]`, `[INFO]`, `[WARN]`, or `[ERROR]` prefix, else `Debug` (`go-plugin/client.go:1226-1277`).
- A line that starts with `panic: ` or `fatal error: ` switches the reader to `Error` for every later untagged line, so a stack trace is not lost at `Debug` (`go-plugin/client.go:1242-1246`). This came in 1.7.0 (`go-plugin/CHANGELOG.md:3-5`).
- A line longer than the buffer is split and logged at `Debug` (`go-plugin/client.go:1198-1210`).
- After the handshake, the plugin replaces its `os.Stdout` and `os.Stderr` with pipes (`go-plugin/server.go:475-494`). In gRPC mode the host reads them through the `GRPCStdio.StreamStdio` stream (`go-plugin/internal/plugin/grpc_stdio.proto:13-19`, `go-plugin/grpc_client.go:75-80`). They go to `io.Discard` unless the host sets `SyncStdout` and `SyncStderr` (`go-plugin/client.go:201-206`, `go-plugin/client.go:406-411`).

## Host-to-add-on API

### Handshake

- The host passes setup data in environment variables. These are the magic cookie, `PLUGIN_MIN_PORT`, `PLUGIN_MAX_PORT`, `PLUGIN_PROTOCOL_VERSIONS`, and optionally `PLUGIN_MULTIPLEX_GRPC`, `PLUGIN_CLIENT_CERT`, and the Unix socket variables (`go-plugin/client.go:638-646`, `go-plugin/client.go:684`, `go-plugin/client.go:698-700`).
- The magic cookie is a fixed key and value. A binary started without it prints that it is a plugin and exits with code 1 (`go-plugin/server.go:247-267`). The code calls it "not a security measure, just a UX feature" (`go-plugin/server.go:49-52`). Terraform's comment says its cookie values "should NEVER be changed" (`terraform/internal/plugin/serve.go:34-36`).
- The plugin answers with one line on stdout: `CORE|APP|NETWORK|ADDRESS|PROTOCOL|SERVER-CERT`, plus `|true` when the host asked for broker multiplexing (`go-plugin/server.go:421-446`). The docs show the five-field form `1|3|unix|/path/to/socket|grpc` (`go-plugin/docs/internals.md:33-63`).
- The host requires at least four fields. A missing fifth field means `netrpc` (`go-plugin/client.go:839-850`, `go-plugin/client.go:903-908`). The sixth field counts as a certificate only if it is longer than 50 characters, which skips "unused extra data" from older plugins (`go-plugin/client.go:923-931`).
- The core version must equal `CoreProtocolVersion`, which is 1. Any other value fails with "the plugin usually only needs to be recompiled" (`go-plugin/server.go:28-33`, `go-plugin/client.go:852-868`).

### App protocol version negotiation

- The host lists every major version it supports in `VersionedPlugins`, a map from version to plugin set. The legacy `ProtocolVersion` and `Plugins` fields fold into that map (`go-plugin/client.go:150-153`, `go-plugin/client.go:619-631`).
- The host sends the list as `PLUGIN_PROTOCOL_VERSIONS`, comma separated (`go-plugin/client.go:633-643`).
- The plugin sorts both lists in descending order and serves the highest version both sides support (`go-plugin/server.go:145-213`). With no match it serves its lowest version and leaves the host to report the error (`go-plugin/server.go:215-221`).
- The host accepts the plugin's version only if it is in its own map. Otherwise it fails with "incompatible API version with plugin" and lists both sides' versions (`go-plugin/client.go:1031-1053`). `NegotiatedVersion()` exposes the result (`go-plugin/client.go:120-124`).
- Within one version, all plugins in a set use one transport. The server reads it from the first plugin in the set (`go-plugin/server.go:193-206`).

### Bidirectional calls and the broker

- The host can pass an object to the plugin, and the plugin can call back into the host (`go-plugin/README.md:39-41`). The `GRPCBroker` makes this work. It keeps one bidirectional `StartStream` RPC open and sends `ConnInfo` messages with a service ID, network, and address over it (`go-plugin/internal/plugin/grpc_broker.proto:8-21`).
- `AcceptAndServe` opens a new listener for each call and serves a gRPC server on it. Its comment says to use it "sparingly" (`go-plugin/grpc_broker.go:368-418`). `Dial` waits 5 seconds for the other side's `ConnInfo` (`go-plugin/grpc_broker.go:528-538`).
- 1.6.0 added multiplexing of brokered connections over the original listener (`go-plugin/CHANGELOG.md:31-41`). Multiplexed streams "MUST be established sequentially" (`go-plugin/client.go:257-268`). Only the plugin side of multiplexing has a Go implementation (`go-plugin/client.go:257-262`).

### Evolving the core protocol

- The core version has stayed at 1. The comment says a bump "will invalidate any prior plugins" and HashiCorp will "do this very infrequently" (`go-plugin/server.go:28-33`).
- go-plugin extends the handshake line by adding fields at the end. The gRPC protocol name became field 5, the AutoMTLS certificate field 6, and multiplexing support field 7 (`go-plugin/client.go:903-943`).
- An unconditional field 7 would break older hosts, because they parse with `strings.SplitN(line, "|", 6)` and fold the seventh field into the sixth. So the plugin appends it only when the host sets `PLUGIN_MULTIPLEX_GRPC`. The field and this guard came in the same commit, `d16cec3`, in 1.6.0 (`go-plugin/server.go:434-444`, `go-plugin/CHANGELOG.md:35-37`).
- go-plugin publishes no deprecation policy (inferred, because `README.md`, `CHANGELOG.md`, and `docs/` never mention deprecation). `netrpc` stays the default protocol "for legacy reasons" (`go-plugin/client.go:208-219`).

### Terraform provider protocol 5 and 6

Terraform is the main example of an app protocol that evolves on top of go-plugin.

- Protocol 5 started with Terraform v0.12 and is the first version under the current strategy (`terraform/docs/plugin-protocol/README.md:20-22`, `terraform/docs/plugin-protocol/README.md:53-55`).
- The major version is in the protobuf package name, `tfplugin5` or `tfplugin6`. A plugin can serve several majors at once as separate gRPC services (`terraform/docs/plugin-protocol/README.md:75-78`).
- Terraform v1.17.2 offers both majors to every provider through `VersionedPlugins` with keys 5 and 6 (`terraform/internal/plugin/plugin.go:11-22`). It branches on `NegotiatedVersion()` to pick the client type (`terraform/internal/command/meta_providers.go:486-500`).
- A minor version adds optional features that older peers can ignore. Minor versions are not on the wire. Peers detect features instead (`terraform/docs/plugin-protocol/README.md:63-82`).
- Feature detection uses two messages. `ServerCapabilities` comes from the provider, for example `plan_destroy` and `move_resource_state`. `ClientCapabilities` comes from Terraform, for example `deferral_allowed` and `write_only_attributes_allowed` (`terraform/docs/plugin-protocol/tfplugin6.proto:280-320`).
- At this tag the files declare protocol 6.11 and 5.10 (`terraform/docs/plugin-protocol/tfplugin6.proto:4-6`, `terraform/docs/plugin-protocol/tfplugin5.proto:4-6`). terraform-plugin-go serves 6.11 and uses the minor number only in logs (`terraform-plugin-go/tfprotov6/tf6server/server.go:40-61`).
- The rule for proto files changed over time. The README says a released proto file is immutable and each minor version gets a new file (`terraform/docs/plugin-protocol/README.md:179-181`, `terraform/docs/plugin-protocol/README.md:206-212`). Tag v1.0.0 has separate files such as `tfplugin5.2.proto` and `tfplugin6.0.proto`. Tag v1.17.2 has one file per major, and the contributor steps say to edit those files in place (`terraform/docs/plugin-protocol/README.md:222-231`). The proto header also says to modify the file for minor versions (`terraform/docs/plugin-protocol/tfplugin6.proto:10-12`).
- Terraform recommends that SDKs support the previous and current majors together for a while, so users need not upgrade Terraform and every provider at once (`terraform/docs/plugin-protocol/README.md:197-204`).
- Dropping a major is a breaking change for a provider and needs a provider major release. Adding one is a minor release (`terraform/docs/plugin-protocol/README.md:140-154`).
- Protocol 4 is gone. `SupportedPluginProtocols` accepts only 5 and 6 (`terraform/internal/getproviders/registry_client.go:61`). The legacy default of 4 stays in the handshake config for pre-0.12 hosts (`terraform/internal/plugin/serve.go:17-22`).
- A new major needs four coordinated changes: a new proto file, a new terraform-plugin-go package, the registry's allowed list, and Terraform's `SupportedPluginProtocols` (`terraform/docs/plugin-protocol/releasing-new-version.md:9-38`).

How terraform-plugin-go adds RPCs within a major:

- A new RPC first appears in a separate optional Go interface, such as `ProviderServerWithListResource`, marked deprecated with a note that it will move into `ProviderServer` (`terraform-plugin-go/tfprotov6/provider.go:83-98`).
- If the provider does not implement it, the server returns gRPC `Unimplemented` (`terraform-plugin-go/tfprotov6/tf6server/server.go:1269-1277`).
- A later release makes the method required. That breaks provider code at compile time even though the wire protocol did not change. This happened for functions and `MoveResourceState` in 0.23.0, ephemeral resources in 0.26.0, resource identity in 0.28.0, and `GenerateResourceConfig` in 0.31.0 (`terraform-plugin-go/CHANGELOG.md:1-5`, `terraform-plugin-go/CHANGELOG.md:67-72`, `terraform-plugin-go/CHANGELOG.md:97-101`, `terraform-plugin-go/CHANGELOG.md:137-142`).

Vault shows a second approach. It tries to dispense a plugin as backend v5. On failure it relaunches the plugin as v4 with AutoMTLS off (`vault/vault/plugincatalog/plugin_catalog.go:645-665`).

## Isolation and permissions

- A plugin is an ordinary process with the host's user, privileges, and, by default, environment. The README calls plugins "relatively secure" because they reach only the interfaces and arguments the host gives them, not the host's memory (`go-plugin/README.md:100-103`). go-plugin adds no sandbox for files or network (inferred from the absence of such code in `go-plugin/runner/runner.go:14-68` and `go-plugin/internal/cmdrunner/cmd_runner.go`).
- The Vault developers worked around go-plugin's environment order. go-plugin puts the host's environment after any variables the host sets on the command, so the host's values win. Vault sets `SkipHostEnv` and rebuilds the order itself, to avoid "a breaking change in go-plugin" (`vault/sdk/helper/pluginutil/run_config.go:114-136`, `vault/sdk/helper/pluginutil/run_config.go:161`).
- go-plugin has no configuration or secret store. Configuration and secrets travel in the app protocol. In Terraform, `ConfigureProvider` carries the provider configuration (`terraform/docs/plugin-protocol/tfplugin6.proto:376`).
- Plugins cannot reach each other through go-plugin. Each `Client` manages one executable (`go-plugin/client.go:82-87`).

### Transport authentication

- Without TLS, the gRPC client dials with insecure credentials (`go-plugin/grpc_client.go:30-37`). On Windows the listener is loopback TCP, so any local process that finds the port can connect (inferred from `go-plugin/server.go:528-534` and `go-plugin/server.go:567-573`).
- AutoMTLS fixes this. The host generates a one-time ECDSA P-521 certificate and passes the PEM in `PLUGIN_CLIENT_CERT`. The plugin generates its own and returns the DER, base64 encoded, as field 6 of the handshake line. Both sides then require and verify the other's certificate (`go-plugin/client.go:229-250`, `go-plugin/client.go:669-692`, `go-plugin/server.go:305-340`, `go-plugin/mtls.go:20-76`).
- The generated certificate is a self-signed CA valid for about 30 years, with `localhost` as its name (`go-plugin/mtls.go:21`, `go-plugin/mtls.go:32-52`).
- AutoMTLS was broken for bidirectional broker calls until 1.4.4. The server config lacked `RootCAs` and `ServerName` (`0d4e9d8` commit, `go-plugin/CHANGELOG.md:131`).
- Terraform turns AutoMTLS on for every provider. `TF_DISABLE_PLUGIN_TLS` turns it off for the SDK test framework only (`terraform/internal/command/meta_providers.go:30-35`, `terraform/internal/command/meta_providers.go:469`).
- Vault without AutoMTLS creates its own CA and hands the plugin a server certificate through a response-wrapped token in the environment (`vault/sdk/helper/pluginutil/run_config.go:90-112`).

## UI contributions

- go-plugin has no UI model.
- Terraform's nearest equivalent is the schema RPC. The provider returns schema information for itself, its data sources, and its managed resources through `GetProviderSchema` (`terraform/docs/plugin-protocol/tfplugin6.proto:360-362`). The `get_provider_schema_optional` capability lets the host cache a schema instead of asking every run (`terraform/docs/plugin-protocol/tfplugin6.proto:289-293`).

## Trust

- `SecureConfig` holds an expected checksum and a hash function. `Check` hashes the executable and compares in constant time before launch (`go-plugin/client.go:317-357`, `go-plugin/client.go:661-667`).
- The comment says the host must get the checksum from "a trusted and authoritative source" and keep the binary unmodifiable between check and run (`go-plugin/client.go:323-326`). The check reads the file by path and the OS then opens it again, so a writable binary allows a swap between the two (inferred from `go-plugin/client.go:343-356` and `go-plugin/client.go:735`).
- 1.4.5 logged a warning when `SecureConfig` was nil. 1.4.9 removed it (`go-plugin/CHANGELOG.md:94-96`, `go-plugin/CHANGELOG.md:116-120`).
- Terraform does not use `SecureConfig` (inferred, because the sparse clone, which holds `internal/command`, `internal/plugin`, `internal/plugin6`, and `internal/getproviders`, has no match). It checks packages at install time with three checks. The registry's SHA-256 for the archive must appear in the release's `SHA256SUMS` file, the downloaded archive must hash to it, and a detached OpenPGP signature over `SHA256SUMS` must verify (`terraform/internal/getproviders/registry_client.go:357-361`, `terraform/internal/getproviders/package_authentication.go:297-318`, `terraform/internal/getproviders/package_authentication.go:330-345`, `terraform/internal/getproviders/package_authentication.go:386-458`).
- Terraform classes a signer as official (HashiCorp key), partner (key with a trust signature from the HashiCorp partners key), or community (any other valid key) (`terraform/internal/getproviders/package_authentication.go:390-398`).
- Vault pins the SHA-256 that an operator registered in its plugin catalog. For a plugin that is not a container image, it checks the hash at every launch through `SecureConfig` (`vault/sdk/helper/pluginutil/run_config.go:163-168`).

## Known failures and lessons

| Problem | Fix or state | Citation |
| --- | --- | --- |
| Reattach killed the process at a stored PID when the dial failed. After a reboot or exit the PID could belong to an unrelated process, so go-plugin inside Nomad could kill it. | 1.6.2 removed the kill. | `df94fce` commit message, `go-plugin/CHANGELOG.md:17-19` |
| An unconditional seventh handshake field would break hosts that parse with `SplitN(..., 6)`. | 1.6.0 added the field and sends it only when the host asks for it through an environment variable. | `d16cec3` commit, `go-plugin/server.go:434-444` |
| A sixth field of "unused extra data" from old plugins could be mistaken for a certificate. | The host ignores a sixth field of 50 characters or fewer. | `go-plugin/client.go:923-931` |
| AutoMTLS failed for bidirectional calls. | 1.4.4 set `RootCAs` and `ServerName` on the server side. | `0d4e9d8` commit, `go-plugin/CHANGELOG.md:131` |
| The gRPC broker leaked goroutines on `Stop` or `GracefulStop`. | Fixed in 1.4.6. | `go-plugin/CHANGELOG.md:110-114` |
| `GracefulStop` on plugin shutdown does not work. | The controller calls `Stop`, with a `TODO`. | `go-plugin/grpc_controller.go:23-24` |
| The bundled gRPC was exposed to CVE-2023-44487 (HTTP/2 rapid reset). | 1.6.1 moved to gRPC 1.58.3. | `557fdfb` commit, `go-plugin/CHANGELOG.md:27-29` |
| `_pidAlive` leaked a pidfd on each call on Linux with Go 1.23 and later. | Fixed in 1.8.0. | `155dcdd` commit, `go-plugin/internal/cmdrunner/process_posix.go:15-27` |
| The host's environment overrode values the host set for the plugin. | Vault sets `SkipHostEnv` and layers the environment itself. go-plugin keeps the old order. | `vault/sdk/helper/pluginutil/run_config.go:114-136` |
| Plugin stack traces were logged at `Debug` and missed. | 1.7.0 raises panic output to `Error`. | `go-plugin/CHANGELOG.md:3-5` |
| Optional RPCs in terraform-plugin-go became required in later releases. | Each step is a breaking SDK release for provider authors. | `terraform-plugin-go/CHANGELOG.md:1-5`, `terraform-plugin-go/CHANGELOG.md:137-142` |

## Rust implementations

The two crates.io searches on 2026-11-12 found no mature host-side (client) implementation of go-plugin over gRPC. The one gRPC host crate, `magma-plugin`, is three months old and has the certificate format bug in its row below. A host side is what `deployarr-server` would need to launch add-ons.

| Crate | Version | Side | Protocol | Notes | Citation |
| --- | --- | --- | --- | --- | --- |
| `grr-plugin` | 0.3.1 | Plugin only | gRPC | The README says "only server side (Plugin side)". It always advertises a Unix socket, so it does not fit Windows (inferred). 14,167 downloads. | `grr-plugin-0.3.1/README.md:3-6`, `grr-plugin-0.3.1/src/lib.rs:35`, `grr-plugin-0.3.1/src/lib.rs:192-193` |
| `tf-provider` | 0.2.2 | Plugin only | gRPC, protocol 6 | A Terraform and OpenTofu provider framework. It advertises TCP and reads `PLUGIN_CLIENT_CERT` for AutoMTLS. | `tf-provider-0.2.2/src/server.rs:49`, `tf-provider-0.2.2/src/server.rs:219-220`, `tf-provider-0.2.2/src/server.rs:304` |
| `grafana-plugin-sdk` | 0.5.0 | Plugin only | gRPC | Grafana's own Rust SDK, from the `grafana` GitHub organization. Its handshake line gives app protocol version 2 for Grafana and advertises TCP. | `grafana-plugin-sdk-0.5.0/Cargo.toml:17-26`, `grafana-plugin-sdk-0.5.0/src/backend/mod.rs:546-559` |
| `goplugin` | 0.1.0 | Host and plugin | `netrpc` only | Includes a `yamux` port and the `MuxBroker`. Published 2026-09-19 with 22 downloads. | `goplugin-0.1.0/src/lib.rs:1-11`, `goplugin-0.1.0/src/client.rs:20`, `goplugin-0.1.0/src/server.rs:113` |
| `magma-plugin` | 0.1.44 | Host | gRPC | First published 2026-08-07, with 1,900 downloads. Its docs say "M0 ships the plain TCP dial", with no TLS layer yet. They also say it base64-encodes the PEM in `PLUGIN_CLIENT_CERT`. go-plugin 1.8.0 sends and parses raw PEM there. | `magma-plugin-0.1.44/src/lib.rs:10-30`, `magma-plugin-0.1.44/src/lib.rs:548-551`, `go-plugin/client.go:684`, `go-plugin/server.go:306-314` |

## What changes in deployarr

Each item names what to copy or avoid and the finding behind it.

1. Run each add-on as its own child process of `deployarr-server`, so a crash in an add-on such as a SQL driver cannot take down PXE or the agent API. Copy go-plugin's process boundary ([Crash handling, health, and restart](#crash-handling-health-and-restart)). A separate process is not a sandbox, so permissions need their own design ([Isolation and permissions](#isolation-and-permissions)).
2. Write a supervisor that go-plugin lacks. It polls a health RPC on a timer, restarts a crashed add-on with backoff, and records each exit in the event log. go-plugin never calls `Ping()` and never restarts a client (`go-plugin/client.go:574-579`).
3. Tie each add-on's life to the server. On Windows, put add-ons in a job object with kill-on-close. On Linux, use a parent-death signal or a cgroup through systemd. go-plugin has none of these, and Nomad shows add-ons that outlive the host ([Crash handling, health, and restart](#crash-handling-health-and-restart)). Do not offer reattach, and never kill by a stored PID. The Nomad incident shows why (`df94fce`).
4. Shut down in two steps. Send a shutdown RPC, wait a grace period, then kill. go-plugin waits 2 seconds (`go-plugin/client.go:551-567`). A SQL add-on with open connections may need longer, so make the grace period a setting with a default.
5. Make the handshake one JSON object on one stdout line, with named fields and a format version. go-plugin's positional pipe format needed an opt-in environment variable and a length heuristic to add fields ([Evolving the core protocol](#evolving-the-core-protocol)). Copy the magic-cookie check, so an admin who runs an add-on by hand gets a clear message (`go-plugin/server.go:247-267`).
6. Negotiate the add-on API major version at launch. The server sends the list of majors it supports and the add-on picks the highest it shares. On no match, the server shows both lists. Copy `VersionedPlugins` and `PLUGIN_PROTOCOL_VERSIONS` ([App protocol version negotiation](#app-protocol-version-negotiation)).
7. Also put the supported API majors in the add-on package manifest, and reject an incompatible package in "Add add-on..." before any file lands. Terraform rejects at install, not at launch ([Package format and manifest](#package-format-and-manifest)).
8. Version the add-on API like Terraform's provider protocol. Put the major in the protobuf package name, such as `deployarr.addon.v1`. Add features within a major through capability messages in both directions, and treat `Unimplemented` from an older add-on as an absent feature ([Terraform provider protocol 5 and 6](#terraform-provider-protocol-5-and-6)). Support the current and previous majors together, and drop a major only in a server major release.
9. Avoid terraform-plugin-go's pattern of turning optional methods into required ones. In the Rust add-on SDK crate, give each new trait method a default body that returns `Unimplemented`, so a new RPC never breaks add-on code at compile time (`terraform-plugin-go/CHANGELOG.md:137-142`).
10. Authenticate the local connection on both platforms. Copy AutoMTLS's per-launch certificates, which Terraform turns on for every provider ([Transport authentication](#transport-authentication)). Prefer a named pipe on Windows and a Unix socket on Linux, each restricted to the server's account, over loopback TCP. go-plugin's Windows path leaves an unauthenticated TCP port when AutoMTLS is off.
11. Start add-ons with an empty environment and pass only named variables. Pass secrets, such as SQL passwords, in a configure RPC over the authenticated connection, never in environment variables or arguments. go-plugin passes the host's environment by default, and Vault had to work around its order ([Isolation and permissions](#isolation-and-permissions)).
12. Verify trust at install and at launch. At install, check the package signature and record each executable's SHA-256 in the server database. At launch, check the hash again, as Vault does through `SecureConfig` ([Trust](#trust)). Keep add-on files in a directory that only the server's account can write, which closes the check-to-run gap the `SecureConfig` comment warns about.
13. Package one executable per server target, such as `windows-x86_64`, `linux-x86_64`, and `linux-aarch64`, and name each target in the manifest. Install only the one that matches the server, and refuse a package without it. Terraform's registry does the same per OS and architecture (`terraform/internal/getproviders/registry_client.go:279-281`).
14. Read add-on logs from stderr as JSON lines and tag each with the add-on's ID. Raise panic output to error level and cap the line length. Copy go-plugin's log forwarding ([Logging and stdio](#logging-and-stdio)). Redact secrets on the server side too, because an add-on's log line is untrusted input.
15. Set explicit gRPC message size limits. go-plugin's client defaults to `math.MaxInt32`, and terraform-plugin-go sets 256 MiB (`go-plugin/grpc_client.go:39-41`, `terraform-plugin-go/tfprotov6/tf6server/server.go:89`).
16. Give add-ons a fixed host service on the same connection for callbacks, such as reading a stored secret or emitting an event. Avoid go-plugin's broker, which opens a listener per brokered server and later needed multiplexing with ordering rules ([Bidirectional calls and the broker](#bidirectional-calls-and-the-broker)).
17. Let an add-on describe its settings as a schema returned by an RPC, and render the form in the Leptos UI from that schema. This follows Terraform's `GetProviderSchema`. Cache the schema per add-on version, as `get_provider_schema_optional` allows ([UI contributions](#ui-contributions)).
18. Do not depend on wire compatibility with go-plugin. deployarr writes both the server and its add-on SDK, and no mature Rust host-side crate exists ([Rust implementations](#rust-implementations)). Copy the concepts above with `tonic` instead. Revisit this only if the owner wants add-ons written in Go.
