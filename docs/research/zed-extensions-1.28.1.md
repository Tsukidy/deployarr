# Zed extensions 1.28.1 reference

This page records how the Zed editor 1.28.1 packages, installs, runs, versions, and restricts its extensions. Zed is the closest technical analog to deployarr's planned add-ons. It is a Rust host that runs extensions as WebAssembly components in wasmtime, with an API defined in WIT and versioned per release. It keeps every released API version working in one host binary. Use this page when you design the add-on runtime, the host-to-add-on API and its versioning, capability grants, and the add-on install flow.

## Sources

Each source below has a short label. Citations on this page name the label, then a path and line range in that source.

| Label | Source | Version | How it was obtained |
| --- | --- | --- | --- |
| zed | `https://github.com/zed-industries/zed` | tag `v1.28.1`, commit `e8bc16acee5ae1937600d571d441c4b5d7b0d288`, committed 2026-11-11 | `git clone --depth 1 --branch v1.28.1 --filter=blob:none --sparse`, then a sparse checkout of `crates/extension_host`, `crates/extension_api`, `crates/extension`, `crates/extension_cli`, `crates/extensions_ui`, `crates/agent_ui`, `docs/src/extensions`, and `assets/settings`. Root files such as `Cargo.toml` were read with `git show HEAD:<path>`. |
| registry | `https://github.com/zed-industries/extensions` | `main` at commit `74007eabeee3b2b27fbe41b1682696c25ad5ec17`, committed 2026-11-09 | `git clone --depth 1`, without submodules |
| crates | `https://crates.io/api/v1/crates/zed_extension_api/versions` | fetched 2026-11-12 | `curl` |
| osv | `https://api.osv.dev/v1/vulns/<CVE>` | fetched 2026-11-12 | `curl`, one record per CVE named under [Known failures and lessons](#known-failures-and-lessons) |
| blog | `https://zed.dev/blog/zed-decoded-extensions` | post dated 2024-10-21, describes extension API 0.2.0 | `curl` |

- `v1.28.1` is the newest stable Zed tag on 2026-11-12. `git ls-remote --tags` lists `v1.29.0-pre` and `v1.29.1-pre` after it, and both carry a prerelease suffix.
- The files under zed `docs/src/extensions` are the source of `https://zed.dev/docs/extensions`. Citations use the files at the tag.
- github.com web pages were not reachable from the research session. Every GitHub fact comes from a clone or from OSV records that link the GitHub advisories.
- The page cites zed paths relative to the repository root, such as `crates/extension_host/src/wasm_host.rs:557-603`.

## Package format and manifest

### `extension.toml`

- An extension is a Git repository with an `extension.toml` at its root (zed `docs/src/extensions/developing-extensions.md:46-54`).
- `ExtensionManifest` has the required fields `id`, `name`, `version`, and `schema_version`. The optional fields are `description`, `repository`, `authors`, `lib`, and one map or list per feature kind, such as `themes`, `languages`, `grammars`, `language_servers`, `context_servers`, `debug_adapters`, and `snippets` (zed `crates/extension/src/extension_manifest.rs:87-127`).
- `capabilities` lists the operations the extension may ask for, such as `process:exec` (zed `crates/extension/src/extension_manifest.rs:119-120`, `crates/extension/src/capabilities.rs:11-20`).
- `schema_version` versions the manifest format. It is separate from the runtime API version. The host accepts schema 0 to 1 (zed `crates/extension/src/extension_manifest.rs:43-61`, `crates/extension_host/src/extension_host.rs:129-132`).
- The host still reads the older `extension.json` format and converts it in memory (zed `crates/extension/src/extension_manifest.rs:22-41`, `338-347`). The registry rejects it at packaging time with "The `extension.json` manifest format has been superseded by `extension.toml`" (registry `src/package-extensions.js:158-162`).
- The host fills some manifest fields from the directory layout, such as `languages/*/config.toml` and `themes/*.json` (zed `crates/extension/src/extension_manifest.rs:353-437`).

### Code, platforms, and the API version

- Code ships as one file, `extension.wasm`, a WebAssembly component built for the `wasm32-wasip2` target (zed `crates/extension/src/extension_builder.rs:26-27`, `crates/extension_host/src/wasm_host.rs:858-882`).
- There is no per-OS or per-architecture binary. The same component runs on macOS, Linux, and Windows. Extension code calls `current_platform` to learn the OS and architecture, because `cfg` directives do not work in Wasm (zed `docs/src/extensions/developing-extensions.md:112`).
- Native programs, such as language servers, are not in the package. The publishing rules forbid bundling them. The extension downloads them at run time or finds them on `PATH` (zed `docs/src/extensions/publishing/prerequisites.md:43-44`, `49`, `72`).
- Most extensions have no code. Only language server, context server, and debugger extensions need Rust (zed `docs/src/extensions/developing-extensions.md:77`).
- The API version is inside the binary. `zed_extension_api` writes six bytes, three big-endian `u16` values, into a custom Wasm section named `zed:api-version` (zed `crates/extension_api/src/extension_api.rs:350-353`, `crates/extension_host/src/wasm_host.rs:846-856`).
- The host reads that section before it compiles the component. It fails the load if the section is missing or malformed (zed `crates/extension_host/src/wasm_host.rs:819-844`). The builder keeps the section when it strips other custom sections (zed `crates/extension/src/extension_builder.rs:565-578`).
- The builder also copies the version into the manifest's `lib.version` (zed `crates/extension/src/extension_builder.rs:190-192`). The packaging CLI writes it to `manifest.json` as `wasm_api_version`, next to `schema_version` and `provides` (zed `crates/extension_cli/src/main.rs:163-174`).
- The published artifact is `archive.tar.gz` plus `manifest.json` (zed `crates/extension_cli/src/main.rs:149-183`).

### Compatibility fields

- A Zed release reports a supported range of manifest schemas and of Wasm API versions. The client sends both ranges to the zed.dev API when it checks for updates and when it installs the latest version, so the server picks a compatible version (zed `crates/extension_host/src/extension_host.rs:687-715`, `975-994`). The catalog search sends only the maximum schema version (zed `crates/extension_host/src/extension_host.rs:661-685`).
- `is_version_compatible` rejects a version whose `schema_version` is above the current one or whose `wasm_api_version` is outside the range (zed `crates/extension_host/src/extension_host.rs:134-155`). The version picker greys out incompatible versions (zed `crates/extensions_ui/src/extension_version_selector.rs:220-233`).
- There is no "minimum Zed version" field. The API version range stands in for it (inferred from the fields of `ExtensionManifest` in zed `crates/extension/src/extension_manifest.rs:87-127`). The `zed_extension_api` README maps each Zed release to the API versions it accepts (zed `crates/extension_api/README.md:60-77`). That table stops at Zed 0.192.x and API 0.6.0, so it is out of date at the pinned tag.

## Install, upgrade, disable, and uninstall

### UX

- Users open the Extension Gallery and install from the catalog (zed `docs/src/extensions/installing-extensions.md:8-10`).
- A developer clicks `Install Dev Extension` and picks a source folder. Zed uninstalls the published version of the same ID, then compiles the folder locally, and the card shows "Overridden by dev extension" (zed `docs/src/extensions/developing-extensions.md:35-39`, `crates/extension_host/src/extension_host.rs:1119-1180`).
- Settings can auto-install named extensions. Updates are on by default for each installed extension, and a setting can turn them off per extension (zed `crates/extension_host/src/extension_settings.rs:8-37`).
- Zed checks for updates at startup. It asks the API for catalog versions that differ from the installed ones, then upgrades only those whose version is higher by semver (zed `crates/extension_host/src/extension_host.rs:464-470`, `687-732`, `778-811`). Dev extensions are left out (zed `crates/extension_host/src/extension_host.rs:698`).
- There is no disable action. The extension UI offers install, upgrade, and uninstall only (inferred from a search of `crates/extensions_ui/src` and `crates/extension_host/src` for "disable", which finds only disabled button states, the suggestion toggle, and the version picker).

### Where files land

- The extensions folder is `~/Library/Application Support/Zed/extensions` on macOS, `$XDG_DATA_HOME/zed/extensions` or `~/.local/share/zed/extensions` on Linux, and `%LOCALAPPDATA%\Zed\extensions` on Windows (zed `docs/src/extensions/installing-extensions.md:14-16`).
- It holds `installed/<id>` for the package, `work/<id>` for files the extension writes, `staging` for downloads in progress, and `index.json` (zed `crates/extension_host/src/extension_host.rs:380-384`).

### Order of steps

1. The store records an outstanding operation for the ID. A second operation on the same ID returns at once (zed `crates/extension_host/src/extension_host.rs:873-876`).
2. It downloads the archive into memory and compares its length with `Content-Length` (zed `crates/extension_host/src/extension_host.rs:888-911`). It checks no hash and no signature.
3. It unpacks into a temporary folder under `staging`, removes `installed/<id>`, and renames the temporary folder into its place (zed `crates/extension_host/src/extension_host.rs:913-945`). If it cannot create the staging folder, it removes the old folder and unpacks straight into `installed/<id>` (zed `crates/extension_host/src/extension_host.rs:946-949`).
4. It rebuilds the index and loads the new version (zed `crates/extension_host/src/extension_host.rs:954-955`).

- Uninstall removes `installed/<id>`, reloads, then removes `work/<id>`. It tries the work folder up to three times, 0, 100, and 200 ms apart, because on Windows a folder with a running process cannot be removed (zed `crates/extension_host/src/extension_host.rs:1040-1102`).

### Crash and failure handling

- An upgrade deletes the old version before it renames the new one. A crash between the two steps leaves no version installed (inferred from `crates/extension_host/src/extension_host.rs:934-943`). There is no rollback.
- Recovery depends on the index. At startup the store loads `index.json`. It rebuilds the index when the file is invalid or older than the `installed` folder (zed `crates/extension_host/src/extension_host.rs:416-453`). A file watcher on `installed` triggers a debounced reload of the changed IDs (zed `crates/extension_host/src/extension_host.rs:472-535`).
- One task performs all loads and unloads, so two loads never run at once (zed `crates/extension_host/src/extension_host.rs:472-475`).
- When an extension fails to load, the store logs it, emits `ExtensionFailedToLoad`, and loads the rest (zed `crates/extension_host/src/extension_host.rs:1660-1692`).

## Runtime model

- Extensions run in process, as WebAssembly components in wasmtime 48.0.7 with WASI preview 2 (zed `Cargo.toml:988-1000`, `crates/extension_host/src/wasm_host.rs:557-563`).
- One `Engine` serves all extensions. Each extension gets its own `Store` and instance (zed `crates/extension_host/src/wasm_host.rs:557-603`, `669-696`).
- After instantiation the host calls the exported `init-extension` (zed `crates/extension_host/src/wasm_host.rs:698-702`, `crates/extension_api/wit/since_v0.8.0/extension.wit:19-20`).
- Calls to one extension are serialized. Each instance has a channel, and one task runs one call at a time against the store (zed `crates/extension_host/src/wasm_host.rs:704-709`, `912-941`).
- A bounded compile cache of 32 MB holds incremental compilation artifacts across extensions (zed `crates/extension_host/src/wasm_host.rs:1003-1023`).

### Resource limits

- The engine turns on epoch interruption. A background task increments the epoch every 100 ms (zed `crates/extension_host/src/wasm_host.rs:566-598`).
- Each store yields to the async executor at every epoch tick and takes a new deadline (`epoch_deadline_async_yield_and_update(1)`) (zed `crates/extension_host/src/wasm_host.rs:685-687`). Epochs here keep a busy extension from blocking an executor thread. They do not stop it. A call that never returns keeps yielding and never ends, and every later call to that extension waits behind it (inferred from the yield mode and the serialized call channel).
- The host sets no fuel, no memory `ResourceLimiter`, and no per-call timeout (inferred from a search of `crates/extension_host/src` for `fuel`, `limiter`, `StoreLimits`, and `memory_size`, which finds none).

### Crashes, restarts, and health

- A trap or an error in a call returns an error to that caller. The host does not recreate the instance, and the same store serves the next call (inferred from `crates/extension_host/src/wasm_host.rs:704-709`, `912-941`).
- There is no health check and no restart policy (inferred from the load path, whose only failure signal is the `ExtensionFailedToLoad` event at load time, zed `crates/extension_host/src/extension_host.rs:1680-1690`).
- Native programs that an extension starts, such as language servers, are separate processes that Zed manages as language servers, not as extensions (inferred from `crates/extension_api/src/extension_api.rs:75-83`, which returns a `Command` for the host to run).

### Logging

- The WASI context inherits stdio, so extension output goes to Zed's own stdout and stderr. Developers run `zed --foreground` to see it (zed `crates/extension_host/src/wasm_host.rs:754-757`, `docs/src/extensions/developing-extensions.md:116`).
- Errors that an extension returns from a call are wrapped with its name and version, as `from extension "<name>" version <v>` (zed `crates/extension_host/src/wasm_host.rs:105-106`, `980-987`).

## Host-to-add-on API

### Transport

- The interface is the WebAssembly Component Model. WIT files define it. `wit_bindgen` generates the guest side in `zed_extension_api`, and `wasmtime::component::bindgen!` generates the host side from the same files (zed `crates/extension_api/src/extension_api.rs:355-360`, `crates/extension_host/src/wasm_host/wit/since_v0_8_0.rs:39-53`).
- The guest exports functions such as `language-server-command`. The host imports functions such as `download-file`, `get-settings`, and `run-command` into the guest (zed `crates/extension_api/wit/since_v0.8.0/extension.wit:51-65`, `94`, `crates/extension_api/wit/since_v0.8.0/process.wit:26-28`).
- The guest trait gives every optional method a default body that returns an error, such as "`language_server_command` not implemented" (zed `crates/extension_api/src/extension_api.rs:68-83`, `183-189`). A new optional export therefore does not break an author's source.

### Versions and negotiation

- There is no negotiation at run time. The binary states its version in `zed:api-version`, and the host picks matching bindings (zed `crates/extension_host/src/wasm_host.rs:659-666`).
- The repository keeps one WIT folder per released version: `since_v0.0.1`, `since_v0.0.4`, `since_v0.0.6`, `since_v0.1.0`, `since_v0.2.0`, `since_v0.3.0`, `since_v0.4.0`, `since_v0.5.0`, `since_v0.6.0`, and `since_v0.8.0` (zed `crates/extension_api/wit/`).
- The host has one Rust module per folder, each with its own `bindgen!` call, `MIN_VERSION`, and linker (zed `crates/extension_host/src/wasm_host/wit/since_v0_6_0.rs:11-53`). An enum holds one variant per version (zed `crates/extension_host/src/wasm_host/wit.rs:94-105`).
- `instantiate_async` walks the versions from newest to oldest and instantiates with the first whose `MIN_VERSION` the binary meets (zed `crates/extension_host/src/wasm_host/wit.rs:107-218`).
- The host accepts API versions from `0.0.1` to a maximum that depends on the release channel. Stable and Preview stop at `since_v0_6_0::MAX_VERSION`, which is `0.7.0`. Dev and Nightly accept `0.8.0` (zed `crates/extension_host/src/wasm_host/wit.rs:53-70`, `crates/extension_host/src/wasm_host/wit/since_v0_6_0.rs:11-12`).
- A binary built against an unreleased version fails to load on Stable with "unreleased versions of the extension API can only be used on development builds of Zed" (zed `crates/extension_host/src/wasm_host/wit.rs:72-92`, `118-119`).
- At the pinned tag, `zed_extension_api` 0.8.0 has `publish = false` (zed `crates/extension_api/Cargo.toml:3-10`). crates.io lists 0.7.0, published 2025-09-12, as the newest release (crates). No `since_v0.7.0` folder exists, so 0.7.0 runs on the `since_v0.6.0` bindings (inferred from `MAX_VERSION` 0.7.0 in `since_v0_6_0.rs:12`).

### How old versions keep working

- Each old module adapts its calls to the newest host code. For example, the 0.6.0 `github` host functions call `latest::github::Host` (zed `crates/extension_host/src/wasm_host/wit/since_v0_6_0.rs:141-157`).
- Interfaces that did not change reuse the newest generated types through the `with` option of `bindgen!`, so they need no conversion code (zed `crates/extension_host/src/wasm_host/wit/since_v0_6_0.rs:22-32`).
- Changed types get `From` and `TryFrom` conversions. In 0.8.0 the debug adapter `host` field changed from `u32` to an `ip-address` variant with IPv6 (zed `crates/extension_api/wit/since_v0.6.0/dap.wit:25`, `crates/extension_api/wit/since_v0.8.0/dap.wit:33`). The 0.6.0 adapter converts IPv4 and returns an error for IPv6 that names the fix: "the extension must be updated to v0.8.0 or later" (zed `crates/extension_host/src/wasm_host/wit/since_v0_6_0.rs:415-447`).
- 0.8.0 also removes the `x86` architecture value (zed `crates/extension_api/wit/since_v0.6.0/platform.wit:13-19`, `crates/extension_api/wit/since_v0.8.0/platform.wit:13-17`).
- The oldest module, 0.0.1, still loads. It reuses 0.6.0 types for `github` and `platform` (zed `crates/extension_host/src/wasm_host/wit/since_v0_0_1.rs:12-27`).
- The cost is size. The nine older modules hold 2,814 lines, and the newest module holds 1,163. The dispatch file `wit.rs` adds 1,308 lines, with a match on the version in each of its 21 `call_*` methods (`wc -l` and `grep` over `crates/extension_host/src/wasm_host/wit.rs` and `wit/*.rs` at the tag).

### Deprecation policy

- Breaking changes wait in `PENDING_CHANGES.md`, so the team can batch them into one release (zed `crates/extension_api/PENDING_CHANGES.md:3-12`).
- No written policy says how long an old API version stays (inferred from a search of zed `docs/src/extensions` and `crates/extension_api` for "deprecat", which finds only feature-kind retirements). At the pinned tag every released version since 0.0.1, from 2024-03-20, still loads (zed `crates/extension_host/src/wasm_host/wit.rs:69`, crates).
- Feature kinds are retired at the registry, not in the host. The packaging CLI rejects slash commands (zed `crates/extension_cli/src/main.rs:227-231`). The publishing rules no longer accept agent server or slash command extensions, and plan to retire MCP server extensions in favor of the MCP registry (zed `docs/src/extensions/publishing/prerequisites.md:66-76`).
- When the editor absorbs an extension's feature, the host adds the ID to `SUPPRESSED_EXTENSIONS`. It hides the ID from catalog results and uninstalls it if present (zed `crates/extension_host/src/extension_host.rs:105-127`, `843-845`, `1270-1275`, `1316-1321`).

## Isolation and permissions

### Files

- Each extension sees one preopened folder, `work/<id>`, with read and write access. It is mapped both as `.` and as its absolute path (zed `crates/extension_host/src/wasm_host.rs:742-763`).
- Host functions that write for an extension, such as `download-file`, pass the target through `writeable_path_from_extension`. It normalizes the path, resolves symlinks in the part that exists, and requires the result to start with `work/<id>` (zed `crates/extension_host/src/wasm_host.rs:765-816`).
- The `worktree` resource lets an extension read text files in the user's project and read the shell environment (zed `crates/extension_api/wit/since_v0.8.0/extension.wit:67-79`).

### Capabilities and grants

- Three capabilities exist: `process:exec`, `download_file`, and `npm:install` (zed `crates/extension/src/capabilities.rs:11-20`).
- `process:exec` matches a command name and an argument pattern, where `*` matches one argument and `**` matches the rest (zed `crates/extension/src/capabilities/process_exec_capability.rs:3-43`). `download_file` matches a host and path segments the same way (zed `crates/extension/src/capabilities/download_file_capability.rs:4-47`).
- Users restrict grants with the `granted_extension_capabilities` setting (zed `docs/src/extensions/capabilities.md:10-37`). The default grants all three with wildcards (zed `assets/settings/default.json:2376-2380`).
- `process:exec` passes two checks. The manifest must list it, and the user grant must allow it (zed `crates/extension_host/src/capability_granter.rs:23-47`). `download_file` and `npm:install` check only the user grant, not the manifest (zed `crates/extension_host/src/capability_granter.rs:49-83`).
- `http-client.fetch` and `fetch-stream` call the host HTTP client with no capability check (zed `crates/extension_host/src/wasm_host/wit/since_v0_8_0.rs:633-666`).
- `run-command` runs a native process with the user's rights, outside the Wasm sandbox (zed `crates/extension_host/src/wasm_host/wit/since_v0_8_0.rs:900-920`). Grants are global. One setting applies to every extension (zed `crates/extension_host/src/wasm_host.rs:637`, `678-681`).
- The publishing rules ask authors not to touch anything outside the folder Zed gives them (zed `docs/src/extensions/publishing/prerequisites.md:28-31`). Only review enforces this, since the default grants do not (inferred from the wildcard defaults above).

### Configuration and secrets

- `get-settings` returns host settings by category: `language`, `lsp`, and `context_servers` (zed `crates/extension_host/src/wasm_host/wit/since_v0_8_0.rs:952-1046`).
- The `lsp` and `context_servers` categories take a caller-supplied key. The code does not check the key against the calling extension's manifest, so an extension can read another server's settings, including its `env` map (inferred from `since_v0_8_0.rs:986-1043`).
- Context server settings come from Zed's project settings and reach the extension as a JSON string (zed `crates/extension_host/src/wasm_host/wit/since_v0_8_0.rs:1005-1038`).
- The WIT has no secret or credential interface (inferred from a search of `crates/extension_api/wit/since_v0.8.0` for `secret`, `credential`, and `keychain`, which finds none).
- A `key-value-store` resource lets an extension write strings (zed `crates/extension_host/src/wasm_host/wit/since_v0_8_0.rs:548-557`).

### Other extensions

- Each extension has its own store, instance, and work folder (zed `crates/extension_host/src/wasm_host.rs:669-684`, `742-760`). There is no API to call another extension (inferred from the WIT world imports in `crates/extension_api/wit/since_v0.8.0/extension.wit:3-10`).
- Grammars cannot be shared between extensions, so one extension's grammar update cannot break another's language (zed `docs/src/extensions/publishing/faq.md:87-89`).

## UI contributions

- Extensions ship no UI code. They contribute data that Zed renders: themes, icon themes, languages, snippets, and settings schemas (zed `crates/extension/src/extension_manifest.rs:103-126`).
- A context server returns `installation-instructions` in Markdown, a `settings-schema` in JSON Schema, and `default-settings` (zed `crates/extension_api/wit/since_v0.8.0/context-server.wit:3-10`).
- Zed shows the instructions, opens a JSON editor filled with the defaults, and validates the user's JSON against the schema (zed `crates/agent_ui/src/agent_configuration/configure_context_server_modal.rs:127-156`, `205-206`). It does not generate a form from the schema (inferred from the JSON editor it creates at line 153).
- 0.8.0 adds exports that return JSON schemas for language server options (zed `crates/extension_api/wit/since_v0.8.0/extension.wit:107`, `112`). The 0.6.0 world has neither.

## Trust

- Zed does not sign extensions (inferred from the install path). The client checks only the download length before it unpacks (zed `crates/extension_host/src/extension_host.rs:888-911`). Trust rests on HTTPS to zed.dev and on the registry review.
- The registry is a Git repository. Each extension is a submodule plus an entry in `extensions.toml` (zed `docs/src/extensions/publishing/publishing-guide.md:40-61`). It lists 1,590 entries at the commit above (registry `extensions.toml`, counted by table headers).
- Zed's CI builds every extension from source with a pinned `zed-extension` CLI. It does not accept author-built binaries (inferred from the packaging step, which runs the CLI on the source folder, registry `.github/workflows/ci.yml:14-15`, `53-57`, `src/package-extensions.js:183-200`).
- CI checks that the IDs and versions match and that the license is on an allowed list (registry `src/package-extensions.js:164-181`, `216-225`, zed `docs/src/extensions/publishing/license-requirements.md:8-26`). It rejects Git LFS (registry `.github/workflows/ci.yml:90-96`).
- On `main`, CI uploads `archive.tar.gz` and `manifest.json` to an S3 bucket under `extensions/<id>/<version>/` (registry `src/package-extensions.js:86`, `229-242`, `.github/workflows/ci.yml:72-80`).
- Human review applies the publishing rules. Each pull request adds or updates one extension, an author may have three open at most, and a pull request whose author does not answer maintainer feedback within 3 weeks is closed (zed `docs/src/extensions/publishing/publishing-guide.md:12-20`). Updates get the same review as new submissions (zed `docs/src/extensions/publishing/faq.md:44-48`).
- Review is slow. The FAQ says first feedback takes a few weeks and can take one or two months (zed `docs/src/extensions/publishing/faq.md:14`).
- Dev extensions are unsigned local source. The user picks the folder, and Zed builds it (zed `docs/src/extensions/developing-extensions.md:35`).

## Known failures and lessons

- **Zip Slip in `download-file` (CVE-2026-27800, GHSA-v385-xh3h-rrfr, published 2026-02-25).** `extract_zip` did not reject `../` in entry names, so an extension could write outside its folder. Zed 0.224.4 fixed it (osv `CVE-2026-27800`). At the pinned tag, `extract_zip` rejects any entry that fails `archive_path_is_normal`, which accepts only normal and `.` path parts (zed `crates/util/src/archive.rs:10-17`, `35`, `102`, read with `git show`).
- **Tar symlink sandbox escape (CVE-2026-27976, GHSA-59p4-3mhm-qm3r, published 2026-02-25).** The tar extractor created symlinks from the archive, and the path guard checked only the text of the path. A tar could create `escape -> /` and then write through it, which the advisory says enables code execution. Zed 0.224.4 fixed it (osv `CVE-2026-27976`). At the pinned tag the guard resolves symlinks in the existing part of the path (zed `crates/extension_host/src/wasm_host.rs:786-815`).
- Both escapes sat in host code that acts for an extension, not in wasmtime. The Wasm sandbox held, and the host's helpers broke it (inferred from the two advisories).
- **A breaking type change needs a down-conversion that can fail.** The IPv6 `host` change could not map back to 0.6.0, so the adapter returns a clear error instead (zed `crates/extension_host/src/wasm_host/wit/since_v0_6_0.rs:426-447`).
- **The manifest format changed once.** `extension.json` became `extension.toml`. The host still converts the old file, and the registry refuses it (zed `crates/extension/src/extension_manifest.rs:338-347`, registry `src/package-extensions.js:158-162`). The manifest still carries a `TODO` for schema 2 (zed `crates/extension/src/extension_manifest.rs:63-64`).
- **Features moved out of extensions.** Slash commands and agent servers were deprecated, and MCP servers are next (zed `docs/src/extensions/publishing/prerequisites.md:66-76`). Extensions for features that moved into the editor are auto-uninstalled (zed `crates/extension_host/src/extension_host.rs:105-127`).
- **The registry repository does not scale.** The blog post reports that Max Brunsfeld, one of its authors, said the repository "will likely have to go", and adds "It doesn't scale, but it worked very well so far" (blog).
- `zed_extension_api` 0.0.7 is yanked on crates.io (crates). No source found gives the reason.

## What changes in deployarr

1. **Put the API version inside the add-on binary.** Copy the `zed:api-version` custom section. The server reads it before it compiles the component and picks bindings from it. The manifest repeats it for the catalog and the upload check. See [Code, platforms, and the API version](#code-platforms-and-the-api-version).
2. **Keep one WIT folder per released API version, and adapt old versions to the newest host code.** Copy the `since_vX.Y.Z` layout, the `with` reuse for unchanged interfaces, and `From`/`TryFrom` adapters. When a new value cannot map back, return an error that names the version the add-on needs, as the IPv6 adapter does. Write the support window down. Zed has no written window, and its older modules hold about 2.4 times the lines of its newest one. See [How old versions keep working](#how-old-versions-keep-working).
3. **Version the manifest schema separately from the API.** Zed's `schema_version` and `wasm_api_version` evolve on their own schedules. deployarr's manifest needs both fields, and the "Add add-on..." dialog checks both before it installs. See [Compatibility fields](#compatibility-fields).
4. **Gate unreleased API versions and batch breaking changes.** Copy the dev-build gate for the next API version and the `PENDING_CHANGES.md` list. A tagged deployarr release then never accepts an add-on built against an API that may still change. See [Versions and negotiation](#versions-and-negotiation).
5. **Ship one Wasm component for both server OSes.** Zed's single `extension.wasm` runs on every OS. The SQL order lookup add-on can do the same, with no per-OS packages, if its drivers compile to `wasm32-wasip2`. See [Code, platforms, and the API version](#code-platforms-and-the-api-version).
6. **Make install and upgrade crash-safe with a rollback.** Copy staging in a folder on the same file system and a single rename. Avoid Zed's order, which deletes the old version before the rename and keeps no copy. Keep the previous version until the new one loads and passes a health call, and record each step in the server database so a restart finishes or undoes it. AGENTS.md requires every state-changing step to be safe to repeat after a crash. See [Crash and failure handling](#crash-and-failure-handling).
7. **Sign packages and check them before unpacking.** Zed checks only `Content-Length` and relies on its central build. deployarr add-ons arrive as files an admin uploads, so there is no central build to rely on. Verify a publisher signature and a hash over the whole package before extraction, and show unsigned packages as unsigned with an explicit admin confirmation. See [Trust](#trust).
8. **Reject links and unsafe paths in every archive the server unpacks.** Both Zed CVEs came from archive extraction in host helpers. The package reader and any download helper the API offers must reject absolute paths, `..`, symlinks, and hard links, and must resolve the final path before each write. See [Known failures and lessons](#known-failures-and-lessons).
9. **Set real resource limits.** Zed uses epochs only to yield. deployarr should give each call an epoch deadline that traps, cap linear memory with a `ResourceLimiter`, and drop a store after a trap and instantiate a fresh one. Report each add-on's state (loaded, failed, restarted) in the web UI. A lookup resolver that hangs must not stall the order lookup chain. See [Resource limits](#resource-limits) and [Crashes, restarts, and health](#crashes-restarts-and-health).
10. **Deny by default, and check every capability against both the manifest and the admin grant.** Copy Zed's two-layer check for `process:exec`. Avoid its gaps. Two of its three capabilities skip the manifest, HTTP fetch has no check, the defaults are wildcards, and grants are global rather than per add-on. Show the requested capabilities in the "Add add-on..." dialog, store grants per add-on, and scope network grants to hosts from the add-on's own admin settings, such as the SQL server address. See [Capabilities and grants](#capabilities-and-grants).
11. **Offer no process execution to add-ons.** Zed's main pattern returns a `Command` for the host to run natively, outside the sandbox. deployarr add-ons should run their logic inside Wasm and reach the outside only through narrow host interfaces, such as a socket to a granted host. See [Runtime model](#runtime-model).
12. **Give add-ons a scoped settings and secrets interface.** Zed has no secret API, hands extension settings over as plain JSON, and lets an extension read other servers' settings by key. deployarr already encrypts secrets at rest with a server key (see [Server](../architecture.md#server)). The add-on should get its own settings and secrets only, through a handle the host scopes to its ID, and never a raw settings lookup. See [Configuration and secrets](#configuration-and-secrets).
13. **Contribute settings pages as a declarative schema, and render real forms.** Copy Zed's rule that add-ons ship no UI code, and its schema plus defaults plus Markdown instructions. Improve on its raw JSON editor. The Leptos UI should render typed forms from the schema, with secret fields marked so the UI never shows their stored values. See [UI contributions](#ui-contributions).
14. **Add a disable action that keeps configuration.** Zed has only uninstall. An admin who pauses the SQL lookup add-on should keep its connections and secrets. See [UX](#ux).
15. **Do not auto-update add-ons.** Zed updates every extension at startup by default. A deployment server should update only when an admin asks, and not while a deployment uses the add-on. See [UX](#ux).
16. **Plan for features that move between add-on and base.** Zed auto-uninstalls extensions whose feature moved into the editor, and retires feature kinds at the registry. deployarr should keep a list of superseded add-on IDs and migrate their settings into the base feature instead of dropping them. See [Deprecation policy](#deprecation-policy).
17. **Tag every log line with the add-on.** Zed sends extension stdout and stderr to its own process output. deployarr should capture each add-on's output into the server log with the add-on ID and version, as Zed does for errors an extension returns. See [Logging](#logging).

## Open questions

- Zed has no analog for an add-on that adds a prompt on the target PC, such as the SQL add-on's order-number prompt in WinPE. The agent must run in base WinPE as one static executable, so a declarative prompt that the agent renders may fit better than add-on code on the PC. Zed's evidence covers only the host side.
- Zed relies on a central build from source. deployarr add-ons come as uploaded files. Who holds the signing keys, and whether deployarr runs a catalog at all, is open.
- Zed publishes no support window for old API versions. deployarr needs a number, such as "every API version from the last N releases", before the first add-on ships.
- The memory cap and call deadline need measured values. Zed gives no numbers to copy.
