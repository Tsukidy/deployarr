# Grafana plugins 13.3.1 reference

This page records how Grafana 13.3.1 packages, installs, runs, signs, and isolates plugins. Grafana is the closest prior art for deployarr's add-ons. Its data source plugins answer a query from settings an admin enters, as deployarr's SQL order lookup does. Its backend plugins run as separate processes on Windows and Linux, and its web UI loads code that plugins ship. Use this page when you design the add-on manifest, the install flow, the add-on runtime, or the add-on settings pages.

## Sources

Each source below has a short label. Citations on this page name the label, then a path and line range in that source.

| Label | Source | Version | How it was obtained |
| --- | --- | --- | --- |
| grafana | `https://github.com/grafana/grafana` | tag `v13.3.1`, commit `5db6ba887630c7fa12e5bfd063f2fa854025468c`, committed 2026-11-10 | `git clone --depth 1 --branch v13.3.1 --filter=blob:none --sparse`, then a sparse checkout of `pkg/plugins`, `pkg/services/pluginsintegration`, `pkg/api`, `conf`, `docs/sources`, and `public/app/features/plugins` |
| sdk | `https://github.com/grafana/grafana-plugin-sdk-go` | tag `v0.297.0`, commit `632d3396dfc7250f8d1fe6ef9f9b605d99bacbe7`, committed 2026-10-07 | `git clone --depth 1 --branch v0.297.0`. Grafana 13.3.1 requires this version in `pkg/plugins/go.mod:15`. The newest SDK tag on 2026-11-12 is `v0.298.0`. |
| go-plugin | `https://github.com/hashicorp/go-plugin` | tag `v1.8.0`, commit `155dcddc94873a285e14b7fa24b2f6ab6139668e` | `git clone --depth 1 --branch v1.8.0`. Grafana 13.3.1 requires this version in `pkg/plugins/go.mod:18`. |
| plugin-tools | `https://github.com/grafana/plugin-tools` | `main` at commit `ba3f1a32672d47a62773edaa34f5f71c39301faf`, committed 2026-11-11. `@grafana/sign-plugin` is version 4.0.0 there. | `git clone --depth 1 --sparse`, then a sparse checkout of `docusaurus/docs` and `packages/sign-plugin`. These files are the source of `https://grafana.com/developers/plugin-tools/`. |
| changelog | `https://raw.githubusercontent.com/grafana/grafana/v13.3.1/CHANGELOG.md` | v13.3.1 | `curl`. Line 3 dates the 13.3.1 release 2026-11-10. |
| osv | `https://api.osv.dev/v1/vulns/<CVE>` | fetched 2026-11-12 | `curl`, one record per CVE named under [Known failures and lessons](#known-failures-and-lessons) |
| angular | `https://grafana.com/docs/grafana/latest/developers/angular_deprecation/` | page for Grafana v12.1 | `curl -L`. The link redirects to `https://archive.grafana.com/docs/grafana/v12.1/developers/angular_deprecation/`. |

- `v13.3.1` is the newest stable Grafana tag on 2026-11-12. `git ls-remote --tags` lists no later tag without a prerelease suffix.
- github.com web pages were not reachable from the research session. Every GitHub fact comes from a clone at the tag above or from `raw.githubusercontent.com`.

## Package format and manifest

### `plugin.json`

- Every plugin has a `plugin.json`. The schema requires `type`, `name`, `id`, `info`, and `dependencies` (grafana `docs/sources/developers/plugins/plugin.schema.json:7`).
- `dependencies` requires `grafanaDependency`, a semver range checked with node-semver, such as `>=12.0.0` (grafana `plugin.schema.json:166-181`). The older `grafanaVersion` field is deprecated (grafana `plugin.schema.json:172-176`).
- `dependencies.plugins` lists other external plugins by `id`, `name`, and `type` (grafana `plugin.schema.json:182-203`). `dependencies.extensions.exposedComponents` lists UI components from other plugins that this plugin uses (grafana `plugin.schema.json:205-216`).
- `backend: true` marks a plugin with a backend process (grafana `plugin.schema.json:236-239`). `alerting` requires `backend` (grafana `plugin.schema.json:224-227`). Alert rules run on the server with no browser, which is the likely reason (inferred).
- `iam` asks Grafana to create a service account with listed RBAC permissions. Grafana hands the account's token to the backend in the `GF_PLUGIN_APP_CLIENT_SECRET` environment variable. The schema notes that this sits behind the `externalServiceAccounts` feature toggle (grafana `plugin.schema.json:531-533`).
- `extensions.addedComponents` must list every UI component the plugin registers into another extension point. The schema says unlisted components do not work (grafana `plugin.schema.json:603-612`).
- The schema has no field for a host API version or a plugin protocol version (grafana `plugin.schema.json`, searched for `version` and `protocol`). The protocol version is negotiated at process start (see [Host-to-plugin API](#host-to-plugin-api)).

### Per-OS and per-architecture binaries

- `executable` names the first part of the backend file name. Grafana looks for `<executable>_<GOOS>_<lowercase GOARCH>`, plus `.exe` on Windows, such as `plugin_linux_amd64` (grafana `plugin.schema.json:268-271`).
- The loader builds that path from the running Grafana's own `runtime.GOOS` and `runtime.GOARCH` (grafana `pkg/plugins/plugins.go:461-478`).
- One ZIP can hold every platform. The packaging guide shows one archive with darwin, linux, and windows binaries next to one `module.js` and one `plugin.json` (plugin-tools `docusaurus/docs/publish-a-plugin/package-a-plugin.md:47-74`).
- The ZIP holds one top-level folder named after the plugin ID (plugin-tools `package-a-plugin.md:37-42`).
- The extractor sets mode `0755` on files whose names end in one of six fixed suffixes, `_linux_amd64`, `_linux_arm`, `_linux_arm64`, `_darwin_amd64`, `_darwin_arm64`, and `_windows_amd64.exe` (grafana `pkg/plugins/storage/fs.go:336-343`).
- The catalog can also store one package per platform. Each catalog version has a `packages` map from an `<os>-<arch>` key, or `any`, to a download URL and SHA-256 (grafana `pkg/plugins/repo/models.go:20-35`, `pkg/plugins/repo/version.go:90-111`, `pkg/plugins/repo/ifaces.go:82-89`).

### Compatibility checks

- The installer sends the running Grafana version, OS, and architecture to the catalog in `grafana-version`, `grafana-os`, and `grafana-arch` headers (grafana `pkg/plugins/repo/client.go:219-228`).
- The catalog answers with an `isCompatible` flag per version. Grafana picks the newest version that is compatible and built for its platform (grafana `pkg/plugins/repo/version.go:17-88`, `pkg/plugins/repo/version.go:132-144`).
- The loader does not check `grafanaDependency`. In `pkg/plugins` and `pkg/services/pluginsintegration`, the field appears only in two struct definitions (grafana `pkg/plugins/models.go:46`, `pkg/plugins/repo/models.go:27`). A plugin unzipped by hand therefore loads on any Grafana version (inferred from that search).

## Install, upgrade, disable, and uninstall

### UX and entry points

- The default path is the catalog in the UI. The admin opens **Administration > Plugins and data > Plugins**, picks a plugin, and clicks **Install** (grafana `docs/sources/administration/plugin-management/plugin-install.md:30-37`).
- A ZIP from outside the catalog is unzipped into the plugin directory by hand (grafana `plugin-install.md:45-55`). The UI has no upload (inferred, because the install page offers none and `public/app/features/plugins/admin` contains no upload code).
- The install API is `POST /api/plugins/:pluginId/install` with an optional version. It passes no URL, so it installs from the catalog only, for the server's own version, OS, and architecture (grafana `pkg/api/api.go:446`, `pkg/api/plugins.go:468-506`, `pkg/plugins/ifaces.go:64-66`).
- `plugins.preinstall` and `plugins.preinstall_sync` in the config file install plugins at startup. `preinstall_sync` blocks startup until they are installed (grafana `conf/defaults.ini:2289-2294`).
- The API refuses to uninstall a preinstalled plugin and refuses to update a version-pinned one, with HTTP 409 (grafana `pkg/api/plugins.go:477-479`, `pkg/api/plugins.go:519-521`).
- `plugin_admin_enabled` turns off install, update, and uninstall from within Grafana, and the install and uninstall API routes with them (grafana `conf/defaults.ini:2272-2273`, `pkg/api/api.go:444-449`). `plugin_catalog_hidden_plugins` hides plugins in the catalog (grafana `conf/defaults.ini:2276-2277`).
- A catalog install takes effect without a restart. A CLI install needs a restart (plugin-tools `docusaurus/docs/key-concepts/plugin-lifecycle.md:27-31`).

### Steps and where files land

1. The installer refuses a second install of the same ID while one is running. It returns `nil`, so the second caller sees success (grafana `pkg/plugins/manager/installer.go:61-68`).
2. It downloads the ZIP to a temporary file. When the catalog gave a SHA-256, a mismatch fails the download (grafana `pkg/plugins/repo/client.go:182-191`).
3. A new install extracts to `<plugins dir>/<id>`. An update extracts to `<plugins dir>/.<id>.staging`, and the old version keeps running from its own files (grafana `pkg/plugins/manager/installer.go:140-144`).
4. Extraction rejects absolute names and `..` paths, writes through an `os.Root` confined to the install folder, and checks every symlink target twice. A rejected archive leaves nothing behind (grafana `pkg/plugins/storage/fs.go:100-139`, `pkg/plugins/storage/fs.go:191-206`).
5. A mismatch between the requested ID or version and the extracted `plugin.json` is logged, not refused, "for backwards compatibility with older plugins" (grafana `pkg/plugins/manager/installer.go:163-170`).
6. The installer installs each `dependencies.plugins` entry from the catalog, recursively (grafana `pkg/plugins/manager/installer.go:84-95`, `pkg/plugins/storage/fs.go:59-64`).
7. For an update, `replace` unloads the old version, deletes its folder, and renames the staging folder into place (grafana `pkg/plugins/manager/installer.go:178-192`).
8. The loader then runs the load pipeline on the new folder (grafana `pkg/plugins/manager/installer.go:103-107`).

### Failure and rollback

- The signature is checked in step 8, after the files are on disk and, for an update, after the old version is deleted. Signature checks run in the validation stage of the load pipeline (grafana `pkg/services/pluginsintegration/pipeline/pipeline.go:59-67`).
- A failed signature check does not fail the install. The loader records a validation or initialization failure in its error tracker, skips the plugin, and returns no error (grafana `pkg/plugins/manager/loader/loader.go:86-116`). `Add` returns an error only when `Load` does (grafana `pkg/plugins/manager/installer.go:103-107`), so the install API answers HTTP 200 for a plugin that did not load (grafana `pkg/api/plugins.go:483-506`).
- Nothing restores the old version, because step 7 already deleted it (grafana `pkg/plugins/manager/installer.go:97-107`, `pkg/plugins/manager/installer.go:182-190`).
- A crash between `os.RemoveAll` and `os.Rename` in step 7 leaves only the staging folder, whose name starts with a dot (grafana `pkg/plugins/manager/installer.go:185-190`). This research did not check whether discovery loads a folder whose name starts with a dot.
- The plugin lifecycle lives in memory, so every restart runs discovery and loading again (plugin-tools `plugin-lifecycle.md:41-45`).

### Disable and uninstall

- `disable_plugins` in the config file stops plugins, core plugins included, from loading and hides them in the catalog (grafana `conf/defaults.ini:2285-2286`). It is a config file setting, so it needs a restart (inferred).
- Uninstall unloads nested plugins, stops the backend, deletes the plugin folder, and then removes the plugin's RBAC roles and service account (grafana `pkg/plugins/manager/installer.go:261-308`).
- A failed RBAC cleanup is logged, and the log tells the admin to set a config option for cleanup at the next start (grafana `pkg/plugins/manager/installer.go:267-270`).

## Runtime model

### Separate process over gRPC

- Each backend runs as a subprocess. Grafana talks to it over gRPC through HashiCorp's go-plugin (plugin-tools `docusaurus/docs/key-concepts/backend-plugins/index.md:16`).
- The docs claim two benefits. A panic in a plugin does not crash Grafana, and a plugin reaches only the interfaces and arguments it is given, not Grafana's memory (plugin-tools `backend-plugins/index.md:26-28`).
- Grafana starts the process with `exec.Command(executablePath, args...)` and an explicit environment (grafana `pkg/plugins/backendplugin/grpcplugin/client.go:93-99`). The client config sets no OS user, job object, cgroup, or resource limit (grafana `client.go:64-102`). The process runs as Grafana's own OS user (inferred from that code).
- go-plugin waits up to 1 minute for the plugin to report its address, because Grafana leaves `StartTimeout` unset (go-plugin `client.go:398-400`).
- On Linux the plugin listens on a Unix socket. On Windows it listens on TCP `127.0.0.1` in ports 10000 to 25000 (go-plugin `server.go:528-575`, `client.go:393-396`).
- go-plugin offers `AutoMTLS` (go-plugin `client.go:244-250`). Grafana's `pkg/plugins` never sets it, so the Windows loopback port takes connections from any local process (inferred from that search).

### Lifecycle, crashes, and health

- The load pipeline has five stages. They are discovery, bootstrap, validation, initialization, and termination (grafana `pkg/plugins/manager/pipeline/doc.go:3-9`).
- Initialization registers the plugin's service account, creates the backend client, starts the process, registers RBAC roles, and then adds the plugin to the registry (grafana `pkg/services/pluginsintegration/pipeline/pipeline.go:69-89`).
- A goroutine checks once per second whether the process has exited and restarts it. Repeated start failures back off exponentially, up to 1 minute between tries (grafana `pkg/plugins/manager/process/process.go:10-14`, `process.go:70-103`).
- The keep-alive loop checks only `p.Exited()`. A plugin that hangs but stays alive is not restarted (inferred from `process.go:84-86`).
- Stop asks the plugin to exit, waits 2 seconds, and then kills it (go-plugin `client.go:530-566`).
- `CheckHealth` is part of the protocol. The data source **Save & test** button calls it, and the HTTP API exposes it for external polling (plugin-tools `backend-plugins/index.md:76-80`).
- `CollectMetrics` returns Prometheus metrics from the plugin process (plugin-tools `backend-plugins/index.md:82-84`).

### Logging

- go-plugin mirrors the plugin's log output into the host log, prefixed with the plugin path. With hclog on both sides the entries stay structured (go-plugin `README.md:43-49`).
- Grafana wraps its own logger as the hclog sink (grafana `pkg/plugins/backendplugin/grpcplugin/client.go:72`).
- `log_backend_requests` logs every backend request (grafana `conf/defaults.ini:2278-2279`).

### Instance management

- The SDK caches one instance per data source. It builds a new instance when the data source's `Updated` time or the Grafana config changes (sdk `backend/datasource/instance_provider.go:65-74`).
- The old instance is disposed 5 seconds after it is replaced (sdk `backend/instancemgmt/instance_manager.go:23`, `instance_manager.go:126-140`). The delay lets requests in flight finish (inferred).

## Host-to-plugin API

### Transport and services

- The protocol is protobuf over gRPC. The services are `Resource`, `Data`, `Diagnostics`, `Stream`, `AdmissionControl`, and `ResourceConversion` (sdk `proto/backend.proto:97-131`, `backend.proto:241-298`, `backend.proto:385-474`).
- Every request carries a `PluginContext` with the org, the plugin ID and version, the user, the instance settings, and selected Grafana config (sdk `proto/backend.proto:50-90`).
- Grafana 13 adds the app-sdk services to the same plugin set (grafana `pkg/plugins/backendplugin/grpcplugin/client.go:35-49`).

### Version negotiation

- The handshake has a protocol version and a magic cookie. The SDK's protocol version is 2, and the cookie is `grafana_plugin_type=datasource` (sdk `backend/grpcplugin/handshake.go:8-26`).
- The magic cookie is "not a security measure, just a UX feature" (go-plugin `server.go:47-52`).
- The host sends the versions it accepts in `PLUGIN_PROTOCOL_VERSIONS`. The plugin picks the newest version it also serves, or its oldest one as a fallback (go-plugin `client.go:642`, `server.go:150-216`).
- Grafana 13.3.1 offers only protocol version 2 (grafana `pkg/plugins/backendplugin/grpcplugin/client.go:38-49`).

### Evolution and deprecation

- Grafana adds services, messages, and fields without a new major version. A breaking change gets a new major protocol version, released with a major Grafana release, and Grafana serves the old and new versions "for some time" (plugin-tools `docusaurus/docs/key-concepts/backend-plugins/plugin-protocol.md:31-37`). The page names no fixed support period.
- The SDK README calls the protocol stable but allows breaking changes in the SDK's Go API. Plugins built with an older SDK still work and miss new features (sdk `README.md:12`).
- An older plugin that lacks a service returns gRPC `Unimplemented`, and Grafana maps each case to a safe answer. A missing `QueryData` becomes `ErrMethodNotImplemented`. A missing `CheckHealth` becomes the status `Unknown` with "Health check not implemented". A missing `CollectMetrics` becomes an empty result (grafana `pkg/plugins/backendplugin/grpcplugin/client_v2.go:112-170`).
- For `QueryChunkedData`, Grafana remembers the first `Unimplemented` answer and falls back to `QueryData` from then on (grafana `client_v2.go:189-207`).
- The app and data source instance settings in `PluginContext` carry the `apiVersion` they were saved with, with the note that it "may be an older version than the current apiVersion" (sdk `proto/backend.proto:15-17`, `backend.proto:38-40`).

## Isolation and permissions

- Nothing in the client config limits file or network access (grafana `pkg/plugins/backendplugin/grpcplugin/client.go:64-102`). The process can reach whatever Grafana's OS user can reach (inferred).
- Grafana passes the plugin a fixed list of host variables, the proxy and CA variables plus `PLUGIN_UNIX_SOCKET_DIR` (grafana `pkg/plugins/envvars/envvars.go:9-27`). It adds its own `GF_*` variables, such as `GF_VERSION` (grafana `pkg/services/pluginsintegration/pluginconfig/envvars.go:51-71`). `forward_host_env_vars` lists plugins that get the full host environment (grafana `conf/defaults.ini:2287-2288`).
- A backend gets only the host data that each request carries in `PluginContext`. To call the Grafana API it needs the service account from `iam`, limited to the RBAC actions the manifest lists (grafana `plugin.schema.json:531-533`).
- A backend reaches another plugin only through the Grafana API (inferred from the protocol services, none of which address another plugin).
- When managed service accounts and the `externalServiceAccounts` feature toggle are on, the install API asks whether the installer holds the permissions the plugin requests. The check runs after the install and does not block it (grafana `pkg/api/plugins.go:498-504`).

### Configuration and secrets

- A data source keeps plain settings in `jsonData` and secrets in `secureJsonData` (plugin-tools `docusaurus/docs/how-to-guides/data-source-plugins/add-authentication-for-data-source-plugins.md:34-52`).
- Grafana encrypts `secureJsonData` before it stores it. After that the browser cannot read the secret back (plugin-tools `add-authentication-for-data-source-plugins.md:50-52`).
- The UI receives `secureJsonFields`, a map of booleans that says which secrets are set (plugin-tools `docusaurus/docs/how-to-guides/app-plugins/add-authentication-for-app-plugins.md:52-64`).
- The backend receives the secrets decrypted in every request, in `decryptedSecureJsonData` (sdk `proto/backend.proto:12`, `backend.proto:30`).
- Settings shared by all instances of a plugin go in the plugin configuration, and the best-practices page tells authors not to use environment variables for configuration (plugin-tools `docusaurus/docs/key-concepts/best-practices.md:70`). That configuration is a `[plugin.<plugin id>]` section of the Grafana config (grafana `docs/sources/setup-grafana/configure-grafana/_index.md:3080-3084`).

## UI contributions

- A plugin ships frontend code as `module.js`. The browser lazy-loads it when a page needs the plugin. An app plugin can preload it at page start (plugin-tools `plugin-lifecycle.md:74-90`).
- A data source's settings page is a React component the plugin passes to `setConfigEditor()` (plugin-tools `docusaurus/docs/key-concepts/anatomy-of-a-plugin.md:79-81`). Grafana has no declarative settings schema (inferred, because the plugin.json schema has no settings field).
- UI extensions let a plugin add links, components, and functions to extension points in Grafana or in other plugins. An exposed component ID must start with the plugin ID, such as `myorg-incidents-app/create-incident-form/v1` (plugin-tools `docusaurus/docs/reference/ui-extensions-reference/ui-extensions.md:200`). The schema recommends a version suffix to prevent future breaking changes (grafana `plugin.schema.json:693`).
- Plugins share one React instance with Grafana at runtime (plugin-tools `docusaurus/docs/migration-guides/update-from-grafana-versions/v12.x-v13.x.md:36-39`).
- The Frontend Sandbox evaluates a plugin's frontend code in a near-membrane virtual environment (grafana `public/app/features/plugins/sandbox/sandboxPluginLoader.ts:1`, `sandboxPluginLoader.ts:90`, `sandboxPluginLoader.ts:209`). It is opt-in per plugin through `enable_frontend_sandbox_for_plugins` (grafana `docs/sources/administration/plugin-management/plugin-frontend-sandbox.md:50-52`).
- Plugins signed by Grafana Labs cannot be sandboxed, and Angular plugins are not supported (grafana `plugin-frontend-sandbox.md:46-48`).
- The sandbox preview "has been deprecated and will be removed in Grafana 13.4" (grafana `plugin-frontend-sandbox.md:26-27`).

## Trust

### Signature format

- A signed plugin carries `MANIFEST.txt`, a PGP clear-signed JSON message. It names the plugin ID, version, key ID, signing time, signature type, signing org, optional root URLs, and the SHA-256 of every file (grafana `pkg/plugins/manager/signature/manifest.go:41-54`, plugin-tools `docusaurus/docs/publish-a-plugin/sign-a-plugin.md:124-165`).
- The signing tool hashes every file and posts the list to `https://grafana.com/api/plugins/ci/sign` with an access policy token. grafana.com fills in the key ID, time, and org and returns the signed manifest (plugin-tools `packages/sign-plugin/src/utils/manifest.ts:9-25`, `manifest.ts:56-101`). Only grafana.com holds the private key (inferred from that flow).
- The tool rejects symlinks that point outside the plugin folder and writes `/` path separators on every OS (plugin-tools `manifest.ts:29-54`, `manifest.ts:74-77`).

### Verification at load

The validation stage computes a status for each plugin (grafana `pkg/plugins/manager/signature/manifest.go:90-248`, `manifest.go:324-373`):

| Condition | Status |
| --- | --- |
| No `MANIFEST.txt`, or one shorter than 10 bytes | unsigned |
| Not a PGP clear-signed message, bad JSON, missing fields, an unknown key ID, a bad signature, or a manifest older than version 2 | invalid |
| Manifest ID or version differs from `plugin.json` | modified |
| `rootUrls` set and none matches Grafana's `root_url` | invalid |
| A listed file is missing or its SHA-256 differs | modified |
| A file on disk is not in the manifest | modified |
| All checks pass | valid, with the signature type and org |

- Signature types are `grafana`, `commercial`, `community`, `private`, and `private-glob` (grafana `pkg/plugins/models.go:247-251`). Only `private-glob` matches `rootUrls` as globs (grafana `manifest.go:284-316`).
- A private signature binds a plugin to one Grafana URL. The signer passes `--rootUrls`, which must match `root_url` (plugin-tools `sign-a-plugin.md:88-100`, `sign-a-plugin.md:177-179`).
- A nested plugin inherits its parent's signature (grafana `pkg/plugins/manager/signature/signature.go:33-49`).

### Unsigned and modified plugins

- Grafana does not load or start an unsigned plugin and writes a message to the server log (grafana `docs/sources/administration/plugin-management/plugin-sign.md:42-48`).
- An unsigned plugin loads only in development mode or when `allow_loading_unsigned_plugins` lists its ID (grafana `pkg/plugins/manager/signature/authorizer.go:22-38`). Grafana logs "Permitting unsigned plugin. This is not recommended" (grafana `signature.go:57-65`).
- No setting loads an invalid or modified plugin (grafana `signature.go:66-78`). The config comment says "Plugins with modified signatures are never loaded" (grafana `conf/defaults.ini:2270`).

### Key management

- Grafana ships one built-in public key, ID `7e4d0c6a708866e7` (grafana `pkg/plugins/manager/signature/statickey/static_retriever.go:10-11`).
- By default Grafana downloads the current key list from `<grafana.com API>/plugins/ci/keys` every 10 days (grafana `pkg/services/pluginsintegration/keyretriever/dynamic/dynamic_retriever.go:21`, `dynamic_retriever.go:111-120`).
- A key that leaves the downloaded list is deleted from the local store, which revokes it (grafana `dynamic_retriever.go:166-176`).
- With no stored keys, Grafana stores the built-in key (grafana `dynamic_retriever.go:180-198`). `public_key_retrieval_disabled` uses the built-in key only (grafana `conf/defaults.ini:2280-2281`, `pkg/services/pluginsintegration/keyretriever/retriever.go:17-24`).

### Catalog review

- A public plugin is reviewed before Grafana grants a signature level. Review runs an automated validator, then a manual code review and a test install (plugin-tools `docusaurus/docs/publish-a-plugin/publish-or-update-a-plugin.md:67-77`).
- Every update goes through the same review (plugin-tools `publish-or-update-a-plugin.md:85-89`).
- Builds through GitHub Actions can attach a signed provenance attestation, and the plugin validator reports an error when one is missing (plugin-tools `docusaurus/docs/publish-a-plugin/build-automation.md:152-189`).

## Known failures and lessons

| Event | What happened | Source |
| --- | --- | --- |
| CVE-2021-43798 | `/public/plugins/<plugin id>/` served files outside the plugin folder in Grafana 8.0.0-beta1 to 8.3.0. Any installed plugin ID worked. CISA lists it as known exploited. | osv `CVE-2021-43798` |
| CVE-2022-21702 | The data source and plugin proxy could serve attacker HTML from the Grafana origin, which made XSS possible. | osv `CVE-2022-21702` |
| CVE-2022-31130 | Data source and plugin proxy endpoints could pass a user's Grafana auth token to the destination plugin. Fixed in 9.1.8 and 8.5.14. | osv `CVE-2022-31130` |
| CVE-2022-39201 | The same endpoints could pass the user's auth cookie to plugins, from 5.0.0-beta1 to before 8.5.14 and 9.1.8. | osv `CVE-2022-39201` |
| CVE-2024-8986 | The plugin SDK's build step embedded the output of `git remote get-url origin` in the binary, with any credentials that URI held. | osv `CVE-2024-8986` |
| CVE-2025-4123 | A client path traversal and an open redirect let an attacker make Grafana load a frontend plugin from another site. The default CSP `connect-src` blocks it. | osv `CVE-2025-4123` |
| Angular removal | Grafana 11 turned AngularJS plugin support off by default, and the page plans its removal in 12. The page names security and `unsafe-eval` in the CSP as reasons, and says data source editors "will likely need a total rewrite". | angular |
| React 19 in Grafana 13 | Plugins use Grafana's React at runtime. React 19 renamed internals that `react/jsx-runtime` and some dependencies rely on, so plugins that expect the old name may fail or crash at runtime. The guide calls this the most likely cause of plugin loading issues. | plugin-tools `v12.x-v13.x.md:28-39`, `v12.x-v13.x.md:73-75` |
| Manifest version 1 | The loader marks any manifest older than version 2 as invalid (grafana `manifest.go:162-166`). | grafana |
| Windows signing | Signing on Windows wrote `\\` separators, and Grafana reported "Modified signature". The tool now writes `/` on every OS. | plugin-tools `sign-a-plugin.md:169-171`, `manifest.ts:74-77` |
| sign-plugin 4.0 | Version 4 requires Node.js 24.15.0 and drops PAC and SOCKS proxies. | plugin-tools `sign-a-plugin.md:22-24` |
| Frontend Sandbox | The opt-in browser sandbox is deprecated after a public preview from Grafana 11.5, and the docs schedule its removal for 13.4. | grafana `plugin-frontend-sandbox.md:26-27`, `plugin-frontend-sandbox.md:44-46` |

- Five of the six CVEs above sit where plugin content meets the host's web origin or the host's credentials (inferred from the advisories). Three of them, CVE-2021-43798, CVE-2022-21702, and CVE-2025-4123, came from serving or loading plugin content in the Grafana origin (inferred from the advisories).
- Shipped UI code ties plugins to the host's frontend framework. Grafana has made two forced migrations, Angular to React and React 18 to 19 (inferred from the angular page and the v12.x-v13.x guide).

## What changes in deployarr

These changes follow from the findings above. They apply to the add-on system, and to [Order lookup](../architecture.md#order-lookup), [Data sources](../architecture.md#data-sources), [Server](../architecture.md#server), and [Web UI](../architecture.md#web-ui) in the architecture.

### Copy

- Give each add-on a manifest with a required host compatibility range, like `grafanaDependency`. Unlike Grafana, check it at load as well as at install, because deployarr has no catalog to compute `isCompatible`. See [Compatibility checks](#compatibility-checks).
- Keep every platform's binary in one package, named by a fixed `<name>_<os>_<arch>` pattern that the host resolves at load. deployarr needs `windows_amd64` and `linux_amd64` at least. See [Per-OS and per-architecture binaries](#per-os-and-per-architecture-binaries).
- Sign a manifest that lists the SHA-256 of every file and binds the add-on ID and version. Treat an extra file, a missing file, or a changed hash as modified, and never load a modified add-on. See [Verification at load](#verification-at-load).
- Allow an unsigned add-on only when the admin names its ID in a server setting, and log a warning at each load. See [Unsigned and modified plugins](#unsigned-and-modified-plugins).
- Run each add-on with a backend as its own process. Negotiate the protocol version at start from a list the host sends, and grow the protocol only by adding services and fields. Map `Unimplemented` to a safe fallback, as Grafana falls back from chunked queries to `QueryData`, or to a feature the UI hides. See [Host-to-plugin API](#host-to-plugin-api).
- Restart a crashed add-on with capped exponential backoff, and stop it gracefully with a short deadline before a kill. See [Lifecycle, crashes, and health](#lifecycle-crashes-and-health).
- Pass the add-on a fixed list of environment variables, not the server's environment. See [Isolation and permissions](#isolation-and-permissions).
- Store add-on secrets encrypted with the server key, and send the UI only a "set or not set" flag per secret, like `secureJsonFields`. See [Configuration and secrets](#configuration-and-secrets).
- Make a health call part of the protocol, and wire it to the existing data source test button. See [Lifecycle, crashes, and health](#lifecycle-crashes-and-health).
- Rebuild an add-on's connection pool when its settings change, and drain the old one after a delay, like the SDK's instance manager. See [Instance management](#instance-management).
- Extract archives through a root-confined writer, and reject absolute paths, `..`, and symlinks that leave the folder. See [Steps and where files land](#steps-and-where-files-land).

### Avoid

- Do not verify the signature after extraction or after the old version is gone. Verify the package before extraction. Then extract to staging, start the new process, and pass its health check before the swap. Keep the previous version's folder until the new one is healthy, so a failed upgrade rolls back. Report a package that fails to load as a failed install. Grafana deletes the old version first, has no rollback, and reports success when the new version fails validation. See [Failure and rollback](#failure-and-rollback).
- Write each install step to a journal, so a crash mid-install resumes or rolls back on the next start. This matches the rule that every state-changing step is safe to run again. See [Failure and rollback](#failure-and-rollback).
- Do not return success to a second install request while one runs. Return a conflict. See [Steps and where files land](#steps-and-where-files-land).
- Refuse a package whose manifest ID or version differs from what the admin selected. Grafana only logs it. See [Steps and where files land](#steps-and-where-files-land).
- Do not rely on a magic cookie or an unauthenticated loopback port. On Windows use a named pipe with an ACL for the server's account, and on Linux a Unix socket with mode `0600`, or mutual TLS with keys made per start. See [Separate process over gRPC](#separate-process-over-grpc).
- Do not treat "the process has not exited" as healthy. Call the health service on a timer, and restart an add-on that misses several calls. See [Lifecycle, crashes, and health](#lifecycle-crashes-and-health).
- Set resource limits. Use a job object on Windows and a cgroup or `systemd-run` scope on Linux, and run the process as a less privileged account where the OS allows. Grafana sets none. See [Separate process over gRPC](#separate-process-over-grpc).
- Do not load add-on code into the web UI's origin. Describe settings pages with a declarative schema that the Leptos UI renders. If an add-on needs a custom page, render it in a sandboxed iframe on a separate origin. Grafana's browser sandbox is being removed, and three CVEs came from plugin content in the main origin. See [UI contributions](#ui-contributions) and [Known failures and lessons](#known-failures-and-lessons).
- Do not share the UI framework with add-ons as a runtime dependency. The Angular and React 19 migrations show the cost. See [Known failures and lessons](#known-failures-and-lessons).
- Never forward an admin's session cookie or token to an add-on. Pass only the add-on's own settings and the request's typed parameters. See CVE-2022-31130 and CVE-2022-39201 in [Known failures and lessons](#known-failures-and-lessons).
- Serve any add-on static file only by a path listed in its signed manifest, never by joining a URL path to a folder. See CVE-2021-43798 in [Known failures and lessons](#known-failures-and-lessons).
- Make the add-on build tooling embed no repository URL or other build environment value that can hold a credential. See CVE-2024-8986 in [Known failures and lessons](#known-failures-and-lessons).
- Do not auto-install dependencies from a catalog that deployarr does not have. Refuse the install and name each missing add-on and version. See [Steps and where files land](#steps-and-where-files-land).
- Let an admin disable an add-on in the UI, without a restart. Disabling stops the process and keeps its settings and secrets. Grafana's `disable_plugins` is a config file setting. See [Disable and uninstall](#disable-and-uninstall).

### Open questions

- Who signs add-ons? Grafana signs everything through grafana.com and revokes keys by publishing a key list. deployarr has no hosted service in its design. One option is a project key built into the server plus admin-added publisher keys, with revocation by a signed list shipped in server updates.
- Is there a catalog, or only an upload? Grafana's compatibility choice depends on its catalog. An upload-only flow needs the server to read the package's compatibility range and platform list itself.
- Does the order-number prompt on the PC come from the add-on? The agent runs in base WinPE and cannot load add-on code. A declarative prompt that the server sends to the agent fits the agent's limits. Grafana has no remote agent to compare.
- Should add-ons run as processes or as WebAssembly components? Grafana's evidence supports processes with gRPC. The transport and resource-limit gaps above apply only to the process model.
