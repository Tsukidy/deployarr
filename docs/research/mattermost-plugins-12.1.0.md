# Mattermost plugins 12.1.0 reference

This page records how Mattermost Server 12.1.0 packages, installs, runs, versions, and trusts server plugins. Mattermost is a self-hosted server where an admin uploads a plugin bundle in a web console. It supports production servers on Linux only (`docs/main/deployment-guide/software-hardware-requirements.mdx:160-171`), but its plugin manifest also names Windows binaries. Its admin-uploaded bundle model is the closest product match to the add-on model planned for deployarr. deployarr uses this page to decide what its add-on system copies and what it avoids. The design it informs is in [Components](../architecture.md#components), [Order lookup](../architecture.md#order-lookup), [Server](../architecture.md#server), and [Web UI](../architecture.md#web-ui).

## Sources

- Mattermost Server tag `v12.1.0` of `https://github.com/mattermost/mattermost.git`. The tag points at commit `243e3d0135203ab9166fe8f2873b07f697b06fd4`, committed 2026-11-05. `git ls-remote --tags` on 2026-11-12 listed `v12.1.0` as the highest tag without an `rc` or `beta` suffix. The security feed below lists 12.1.0 as a fix release dated 2026-11-05.
- The clone was `git clone --depth 1 --branch v12.1.0 --filter=blob:none --sparse`, with `server/public/plugin`, the plugin files of `server/public/model`, `server/channels/app/plugin*.go`, `server/channels/api4/plugin.go`, and the plugin pages under `docs/` checked out. Other files cited here come from `git show v12.1.0:<path>`.
- Every `server/...` and `docs/...` path on this page is a file in that tag, with line numbers at that commit.
- The tag's `docs/` folder builds `docs.mattermost.com`, with `docs/develop` under `/developers` (`docs/site/docusaurus.config.ts:50`, `:105-107`). The live sites were also fetched on 2026-11-12. The live pages are not versioned. They already describe a v12.2 feature, `webapp.pre_auth`, that is not in 12.1.0 (`https://developers.mattermost.com/integrate/plugins/components/webapp/`, fetched 2026-11-12). Where the two differ, this page uses the tag.
- The security advisory feed is `https://securityupdates.mattermost.com/security_updates.json`, fetched 2026-11-12. The docs page `docs/main/security-guide/security-updates.mdx` renders it, and `docs/site/src/components/SecurityUpdates/feed.ts:3` names the URL. The feed held 753 advisories, 49 of them server advisories that mention plugins. This page cites advisories by their `MMSA` ID.
- The server links `github.com/hashicorp/go-plugin v1.8.0` (`server/public/go.mod:14`).

## Package format and manifest

A plugin is one `.tar.gz` bundle (`docs/develop/contribute/more-info/server/plugins.md:24`). The manifest sits at the bundle root, or inside the bundle's only top-level folder (`server/public/model/manifest.go:138-140`, `server/channels/app/plugin_install.go:439-445`).

| Field | Purpose | Citation |
| --- | --- | --- |
| `id` | Unique ID, 3 to 190 characters, `^[a-zA-Z0-9-_\.]+$`. Reverse DNS is suggested. | `server/public/model/manifest.go:180-183`, `server/public/model/plugin_valid.go:12-14` |
| `name`, `description`, `homepage_url`, `support_url`, `release_notes_url`, `icon_path` | Display metadata. The icon must be SVG. | `server/public/model/manifest.go:185-202` |
| `version` | Semver is recommended. `Manifest.IsValid` requires strict semver when present, but the install path does not call it. | `server/public/model/manifest.go:204-205`, `:340-345`, `server/channels/app/plugin_install.go:256` |
| `min_server_version` | Lowest server version that may activate the plugin. | `server/public/model/manifest.go:207-210`, `server/public/plugin/environment.go:268-282` |
| `required_add_on` | License add-on that must be present before the plugin can be enabled. New in 12.0. | `server/public/model/manifest.go:212-216`, `server/channels/app/plugin.go:464-466` |
| `server.executables` | Map from `<GOOS>-<GOARCH>` to a binary path in the bundle. | `server/public/model/manifest.go:232-245` |
| `server.executable` | Single binary path, used when no map entry matches. | `server/public/model/manifest.go:241-244`, `:279-297` |
| `webapp.bundle_path` | One JavaScript bundle for the web app. | `server/public/model/manifest.go:247-254` |
| `settings_schema` | Settings rendered by the System Console. | `server/public/model/manifest.go:124-136`, `:224-226` |
| `props` | Free data other plugins can read. | `server/public/model/manifest.go:228-229` |

- The manifest can be `plugin.json` or `plugin.yaml` (`server/public/model/manifest.go:138-140`).
- Per-platform binaries are separate files in one bundle. The server picks `server.executables["<os>-<arch>"]` for its own `runtime.GOOS` and `runtime.GOARCH`, and falls back to `server.executable` (`server/public/model/manifest.go:274-297`, `server/public/plugin/supervisor.go:45-51`). The documented example ships `linux-amd64`, `darwin-amd64`, and `windows-amd64` binaries, and a Windows binary must end in `.exe` (`server/public/model/manifest.go:155-160`, `:240`).
- The server rejects an executable path that resolves outside the bundle with `..` (`server/public/plugin/supervisor.go:52-55`).
- The only compatibility field is a minimum. There is no maximum server version and no API version field (`server/public/model/manifest.go:179-230`). The server checks `min_server_version` at activation, not at upload (`server/public/plugin/environment.go:268-282`, `:350-353`).
- The Marketplace query carries the server version and `<os>-<arch>` platform (`server/channels/app/plugin.go:792-813`, `server/public/model/marketplace_plugin.go:80-93`). The catalog service filters by both (inferred, because the Marketplace service is not in this repository).

## Install, upgrade, disable, and uninstall

### Admin UX

- Three install paths exist. An admin uploads a bundle in System Console, installs from a URL, or picks a plugin in the Marketplace. All three need the `sysconsole_write_plugins` permission (`server/channels/api4/plugin.go:27-44`, `:56-59`, `:111-114`, `:144-147`).
- Uploads are off by default. `PluginSettings.EnableUploads` defaults to `false` (`server/public/model/config.go:3701-3703`). The docs say it "cannot be enabled via the System Console or REST API" and must be set in `config.json` (`docs/develop/integrate/plugins/using-and-managing-plugins.md:227-229`).
- The upload request is multipart with a 50 MiB in-memory limit (`server/channels/api4/plugin.go:24`, `:61-68`).
- Re-uploading an existing ID fails unless the request sets `force`. The conflict error carries the existing version, the uploaded version, and whether the upload is an upgrade or a downgrade, so the UI can ask for confirmation (`server/channels/api4/plugin.go:91-94`, `server/channels/app/plugin_install.go:255-286`).
- Install and enable are separate steps. An installed plugin does not run until an admin enables it, unless config already marks it enabled (`docs/develop/contribute/more-info/server/plugins.md:24`, `server/channels/app/plugin_install.go:170-179`).

### Where files land

- The server stores the uploaded `.tar.gz` and its `.sig`, if any, in the file store as `plugins/<id>.tar.gz` and `plugins/<id>.tar.gz.sig` (`server/channels/app/plugin_install.go:214-249`, `:646-656`).
- The server unpacks the bundle to `PluginSettings.Directory/<id>`, and also copies a web app bundle to `PluginSettings.ClientDirectory` for serving (`server/channels/app/plugin_install.go:4-10`, `:516-528`).
- The file store copy is the source of truth. At startup the server deletes every local unpacked plugin and unpacks each bundle again from the file store (`server/channels/app/plugin.go:282-362`, `:222-224`).
- Enabled state lives in config, as `PluginSettings.PluginStates[<id>].Enable`. Plugin settings live in `PluginSettings.Plugins[<id>]` (`server/public/model/config.go:3686-3687`, `server/channels/app/plugin.go:468-470`).
- Prepackaged plugins ship in `prepackaged_plugins/` next to the server. They install automatically only when `AutomaticPrepackagedPlugins` is on and config already marks them enabled (`server/channels/app/plugin_install.go:35-38`, `server/channels/app/plugin.go:1041-1058`).
- Since v10.5 and v11.0, copying files straight into the plugin directory is unsupported. Admins who need scripted deployment prepackage the bundle instead (`docs/main/administration-guide/upgrade/important-upgrade-notes.mdx:697`, `:783`).

### Order of steps

1. Extract the bundle into a temporary directory, read the manifest, and check the plugin ID. The server validates no other manifest field (`server/channels/app/plugin_install.go:400-455`, `:256`).
2. Look for an installed plugin with the same ID and apply the strategy: reject, skip unless newer, or always replace (`server/channels/app/plugin_install.go:385-394`, `:477-512`).
3. Remove the old local install, deactivating it first (`server/channels/app/plugin_install.go:509-512`, `:591-626`).
4. Copy the new files into `Directory/<id>` and unpack the web app bundle (`server/channels/app/plugin_install.go:516-528`).
5. If config marks the plugin enabled, activate it (`server/channels/app/plugin_install.go:530-545`).
6. Write the bundle and signature to the file store and send a cluster event (`server/channels/app/plugin_install.go:196-199`, `:214-249`).
7. Notify web clients over WebSocket when every cluster node reports the same version (`server/channels/app/plugin.go:844-886`).

### Failure and rollback

- There is no rollback. Step 3 deletes the old version before step 4 copies the new one, so a failed copy or a failed activation leaves no working version on disk (`server/channels/app/plugin_install.go:508-545`).
- A failed activation returns an error after the files are in place. The file store write in step 6 never runs, so the next restart restores the previous bundle from the file store (inferred from `server/channels/app/plugin_install.go:186-199` and `server/channels/app/plugin.go:282-362`).
- Uninstall disables the plugin in config first, so a later reinstall stays disabled. It then deletes the local files, the file store bundle, and the signature (`server/channels/app/plugin_install.go:549-593`).
- Disable writes `Enable: false` and saves config. A config listener runs `syncPluginsActiveState`, which reconciles every plugin to its desired state, activating or deactivating as needed (`server/channels/app/plugin.go:75-162`, `:238-243`, `:494-530`).

## Runtime model

- A server plugin is a separate process started from the bundle's executable. It talks to the server over `hashicorp/go-plugin` with Go `net/rpc`. gRPC is not supported (`docs/develop/integrate/plugins/components/server/index.md:6`, `server/public/plugin/client.go:57-91`).
- The handshake is protocol version `1` plus a magic cookie in an environment variable (`server/public/plugin/api.go:1866-1870`).
- The process must answer within a 3 second start timeout (`server/public/plugin/supervisor.go:116-124`).
- The server computes a SHA-256 of the executable just before it starts it and hands it to go-plugin. The code comment says this "doesn't add more security than before" and only silences a warning (`server/public/plugin/supervisor.go:60-74`).
- The plugin's stdout and stderr go to the server log, tagged with the plugin ID (`server/public/plugin/supervisor.go:102-122`, `server/public/model/bundle_info.go:18-23`).
- Lifecycle is `OnConfigurationChange`, then `OnActivate`. An error from `OnActivate` kills the process. `OnDeactivate` gets 10 seconds before the server kills the process anyway (`server/public/plugin/hooks.go:92-115`, `server/public/plugin/environment.go:284-306`, `:462-488`).
- States are `NotRunning`, `Running`, `FailedToStart`, and `FailedToStayRunning`. `Starting` and `Stopping` exist but the server does not use them (`server/public/model/plugin_status.go:7-12`).

### Health checks and restarts

- A job pings each active plugin over RPC every 30 seconds. Three failed pings in a row count as one failure (`server/public/plugin/health_check.go:14-19`, `server/public/plugin/supervisor.go:207-222`).
- After a failure the server restarts the plugin. After 3 failures within 60 minutes it deactivates the plugin and sets `FailedToStayRunning` (`server/public/plugin/health_check.go:50-79`, `:106-114`).
- `EnableHealthCheck` defaults to `true` (`server/public/model/config.go:3709-3711`).
- The check only proves the RPC link answers. It does not test the plugin's own work (`server/public/plugin/supervisor.go:224-235`).

### Limits

- There are no CPU, memory, or file handle limits on the plugin process in the plugin package. The host sets no timeout of its own on hooks other than `OnDeactivate`, and records hook duration as a metric only (inferred from the absence of limits in `server/public/plugin/*.go` and from `server/public/plugin/environment.go:633-731`).
- Since v12.0, an HTTP request's deadline and cancellation reach the plugin over a separate stream. Context values do not (`server/public/plugin/context_rpc.go:13-18`, `docs/develop/integrate/plugins/components/server/best-practices.md:22-26`).
- In a cluster, each node runs its own copy of each plugin. Plugins must keep shared state in the KV store (`docs/develop/integrate/plugins/components/server/ha.md:10-12`).

## Host-to-plugin API

- Two directions share one go-plugin connection. The server calls hooks on the plugin, and the plugin calls the `API` interface on the server. Each `OnActivate` call opens two more multiplexed streams, one for the API and one for a database driver (`server/public/plugin/client_rpc.go:327-357`).
- `API` holds hundreds of methods, from `LoadPluginConfiguration` to user, channel, post, file, and KV operations. Each method documents a "Minimum server version" (`server/public/plugin/api.go:16-60`). At the tag, 264 such annotations exist, from 5.2 to 12.1.
- `make pluginapi` generates the RPC glue from the Go interfaces (`docs/develop/contribute/more-info/server/plugins.md:30`).

### How the API evolves

- Hook IDs are a wire protocol. The source says to add new IDs but never change existing ones (`server/public/plugin/hooks.go:15-18`). Six retired hooks keep their numbers with a `deprecated` prefix (`server/public/plugin/hooks.go:48-53`).
- At start, the server asks the plugin which hooks it implements. The plugin answers by reflection over its own methods, matching names and signatures. The server then calls only those hooks (`server/public/plugin/client_rpc.go:261-311`, `server/public/plugin/supervisor.go:148-157`).
- The two sides negotiate optional features with extra fields in RPC structs. `OnActivate` carries a `ContextHTTP` flag in both directions, and "older peers treat the missing field as false" (`server/public/plugin/client_rpc.go:313-357`). This works because `gob` ignores unknown fields (inferred).
- A plugin that calls a method an older server lacks gets an RPC error at call time. Authors are told to raise `min_server_version` instead (inferred from the per-method annotations, and stated for context cancellation in `docs/develop/integrate/plugins/components/server/best-practices.md:22-26`).
- Since v8.0 the `model` and `plugin` packages are a separate Go module, `server/public`. The notes say it "has not yet shipped a v1 and will introduce breaking changes before stabilizing the API", and also call it "forwards and backwards compatible with the Mattermost server itself" (`docs/main/administration-guide/upgrade/important-upgrade-notes.mdx:928`, `:937`).
- The `Driver` interface gives raw SQL access to the Mattermost database. Its comment says it "is not subject to backward compatibility guarantees" and is meant for Mattermost's own plugins (`server/public/plugin/driver.go:20-25`).

### Deprecation and breaking changes

- The tag has no written deprecation window for the server API (inferred from a search of `docs/develop/integrate/plugins/` and the upgrade notes). Mattermost announces breaks in the upgrade notes, the deprecated features page, and forum posts (`docs/main/administration-guide/upgrade/important-upgrade-notes.mdx:661`, `docs/main/product-overview/deprecated-features.mdx:70-72`).
- v5.6 dropped automatic unmarshalling of plugin config onto the plugin struct. Old binaries kept the old behavior, and plugins changed only when rebuilt (`docs/develop/integrate/plugins/migration.md:6-8`).
- Web app plugins share the host's JavaScript runtime, so host library changes break them. Server-only plugins were unaffected by the React 19 change (`docs/main/administration-guide/upgrade/important-upgrade-notes.mdx:45`). v11.0 removed `registerPostDropdownMenuComponent` and stopped exporting Styled Components (`docs/main/administration-guide/upgrade/important-upgrade-notes.mdx:661`, `:664`). v12.0 moved to React 19, and "web app plugins that have not been rebuilt for v12.0 may have their activation suppressed" (`docs/main/administration-guide/upgrade/important-upgrade-notes.mdx:30`, `:45`, `docs/develop/integrate/plugins/components/webapp/react-jsx-runtime.md:6-10`).

## Isolation and permissions

- A server plugin can "launch threads or subprocesses of its own, interact with third-party services or do anything else a regular program can do" (`docs/develop/integrate/plugins/components/server/index.md:36`). It runs under the server's OS account, because the server starts it with `exec.Command` and no other user (inferred from `server/public/plugin/supervisor.go:59`).
- There is no permission manifest. Every plugin gets the whole `API`, including `GetUnsanitizedConfig`, which returns the full config with every secret, and `SaveConfig`, which replaces it (`server/public/plugin/api.go:54-64`, `server/channels/app/plugin_api.go:111-118`).
- A plugin can install other plugins through the API when uploads are enabled. This path checks `Enable` and `EnableUploads` only, not `RequirePluginSignature` (`server/channels/app/plugin_api.go:1277-1287`).
- The plugin ID scopes the KV store. Each KV call passes the caller's ID to the store, so one plugin's keys do not collide with another's (`server/channels/app/plugin_api.go:1292-1302`).
- Plugins can call each other over HTTP with `PluginHTTP`. The server sets `Mattermost-Plugin-ID` to the caller (`server/public/plugin/api.go:1145`, `server/channels/app/plugin_requests.go:94-102`).

### Configuration and secrets

- Plugin settings sit in `PluginSettings.Plugins[<id>]` inside the main config, next to the server's own settings (`server/public/model/config.go:3686`).
- The server masks a setting with `"secret": true` when the System Console or API reads config. Masking happens on read only. Storage is the same plain config (`server/public/model/manifest.go:93-95`, `server/public/model/config.go:3779-3815`; plain storage inferred from the map type).
- The plugin pulls its settings with `LoadPluginConfiguration`, which merges schema defaults with stored values, unmasked (`server/channels/app/plugin_api.go:51-80`). The server pushes `OnConfigurationChange` whenever config changes (`server/channels/app/plugin.go:238-251`).

### HTTP routes

- Requests to `/plugins/<id>/...` reach the plugin's `ServeHTTP`. The server strips `Mattermost-User-Id`, `Mattermost-Plugin-ID`, the session cookie, `Referer`, and the `access_token` query parameter. It strips the `Authorization` header only when the token resolves to a session. It sets `Mattermost-User-Id` only after the session, MFA, and CSRF checks pass (`server/channels/app/plugin_requests.go:173-207`, `:233-288`).
- Unauthenticated requests still reach the plugin with no user header. The plugin must reject them itself (`docs/develop/integrate/plugins/components/server/best-practices.md:16`).
- The tag has an MFA check and a body size limit on this path (`server/channels/app/plugin_requests.go:258-281`). The limit applies only where the host reads the body for the CSRF check, which is a cookie-authenticated, non-GET request with no CSRF header (`server/channels/app/plugin_requests.go:293-313`). Advisories MMSA-2025-00444, MMSA-2026-00646, and MMSA-2026-00775 describe the missing MFA check and missing body limits. That this code is their fix is inferred, because the shallow clone has no history.

## UI contributions

### Declarative settings

- `settings_schema` holds a header, a footer, flat `settings`, and keyed `sections` (`server/public/model/manifest.go:98-136`).
- Setting types are `bool`, `dropdown`, `generated`, `radio`, `text`, `longtext`, `number`, `username`, and `custom` (`server/public/model/manifest.go:27-71`).
- `generated` produces a random string, and `regenerate_help_text` labels its button (`server/public/model/manifest.go:55-56`, `:75-76`).
- `hosting` hides a setting on cloud or on-premises. The check is client side only, so the plugin must validate input itself (`server/public/model/manifest.go:88-91`).
- `Manifest.IsValid` checks the schema (`server/public/model/manifest.go:354-359`, `:364-428`). The install path does not call it and validates only the plugin ID (`server/channels/app/plugin_install.go:256`, `:450-452`).

### Shipped code

- `custom` settings and `custom` sections render a React component that the plugin's web app bundle registers. A `fallback` section still renders its plain settings when the plugin is disabled (`server/public/model/manifest.go:68-71`, `:117-121`).
- The web app bundle runs in the main web app with the same Redux store, "giving the plugin access to the full state of the web app" (`docs/develop/integrate/plugins/components/webapp/index.md:38`).
- The server serves `ClientDirectory` under `/static/plugins/`, and the client manifest names the bundle `<id>/<id>_<hash>_bundle.js`, where the hash is a 64-bit FNV-1a of the file (`server/channels/web/static.go:40`, `:49`, `server/public/model/manifest.go:252-253`, `:269`).
- `GET /api/v4/plugins/webapp`, which lists web app plugins, needs no session in 12.1.0 (`server/channels/api4/plugin.go:38`). v12.2 adds a setting that limits pre-login loading to plugins that declare `pre_auth` (`https://docs.mattermost.com/administration-guide/configure/plugins-configuration-settings.html`, fetched 2026-11-12).

## Trust

### Signing and verification

- A signature is an OpenPGP detached signature over the whole `.tar.gz`, made with `gpg --detach-sign` (`docs/develop/integrate/plugins/using-and-managing-plugins.md:186-192`). The server verifies it with `golang.org/x/crypto/openpgp`, which the code imports with `nolint:staticcheck` (`server/channels/app/plugin_signature.go:14-15`, `:133-142`). The marker silences the linter's deprecation warning for that package (inferred).
- Verification tries the Mattermost key compiled into the server, then a second compiled-in key behind the `EnableMFIPluginSignaturePublicKey` feature flag, which defaults to on, then each admin-added key (`server/channels/app/plugin_signature.go:76-120`, `server/channels/app/plugin_public_keys.go:6`, `:32`, `server/public/model/feature_flags.go:168-170`, `:262`).
- Admins add keys as files listed in `SignaturePublicKeyFiles` (`server/channels/app/plugin_signature.go:36-56`, `docs/develop/integrate/plugins/using-and-managing-plugins.md:196-202`).
- Marketplace installs always verify. The catalog entry carries the signature in base64, and the bundle comes from a separate download URL (`server/channels/app/plugin_install.go:292-383`, `server/public/model/marketplace_plugin.go:18-30`).
- `buildPrepackagedPlugin` verifies prepackaged plugins before extraction. This became mandatory in v10.11 (`server/channels/app/plugin.go:1210-1229`, `docs/main/administration-guide/configure/plugins-configuration-settings.mdx:85`).

### Unsigned handling

- Uploaded and URL-installed bundles are never verified. The upload path passes no signature (`server/channels/app/plugin_install.go:170-179`, `:217-219`).
- `RequirePluginSignature` therefore turns uploads off entirely. With it on, the REST API installs only from the Marketplace and prepackaged plugins (`server/channels/api4/plugin.go:48-51`, `:101-106`, `docs/develop/integrate/plugins/using-and-managing-plugins.md:111-113`, `:239`).
- With `RequirePluginSignature` on, the server verifies again each time it syncs bundles from the file store, at startup and on cluster events (`server/channels/app/plugin.go:339-350`, `server/channels/app/plugin_install.go:122-135`).
- `RequirePluginSignature` defaults to `false` (`server/public/model/config.go:3765-3767`).

### Catalog review

- Mattermost developers review and code-sign community plugins before listing, so the signed bundle is the code they reviewed (`docs/develop/integrate/plugins/using-and-managing-plugins.md:30`).
- The review path is two core committer reviews, one focused on security, a QA pass, and a PM/UX review (`docs/develop/integrate/plugins/community_process.md:20-29`).
- Mattermost removes a listed plugin when a medium or higher security issue, or a bug that blocks use, stays unfixed 14 days after it is acknowledged (`docs/develop/integrate/marketplace-submissions/index.md:35-37`).

### Bundle extraction

- The extractor accepts only regular files and directories, skips links and other types, and rejects names that resolve outside the target (`server/channels/app/extract_plugin_tar.go:42-57`).
- It has no limit on decompressed size or file count (inferred from `server/channels/app/extract_plugin_tar.go:21-91`).

## Known failures and lessons

| Advisory or change | What happened | What Mattermost changed |
| --- | --- | --- |
| MMSA-2025-00503 (CVE-2025-9079), MMSA-2025-00500 (CVE-2025-36530) | Admins wrote unsigned bundles into the prepackaged plugin directory through the import directory, bypassing signature enforcement. | `installPlugin` refuses to run when the plugin and import directories conflict (`server/channels/api4/plugin.go:412-422`). That this check is the fix is inferred. |
| MMSA-2022-0098 | Users with plugin permissions uploaded plugins while uploads were off in `config.json`. | `EnableUploads` can only be set in the config file (`docs/develop/integrate/plugins/using-and-managing-plugins.md:227-229`). That this is the fix is inferred. |
| MMSA-2022-0095 | Installers picked old Marketplace versions with known flaws. | The fix is not visible in the server tree. `InstallMarketplacePlugin` still passes a requested version to the Marketplace (`server/channels/app/plugin_install.go:325`, `server/channels/app/plugin.go:637-642`). |
| MMSA-2025-00528 | Public CI instances with default credentials let anyone upload a plugin that read AWS and SMTP credentials. | The advisory names no server code change. It shows a plugin's reach once installed (see [Isolation and permissions](#isolation-and-permissions)). |
| MMSA-2025-00444 | Plugin HTTP routes skipped MFA enforcement. | MFA check in `servePluginRequest` (`server/channels/app/plugin_requests.go:258-269`, fix link inferred). |
| MMSA-2026-00646, MMSA-2026-00775 | Plugin routes read request bodies with no size limit. | Host-side body limit on the CSRF read (`server/channels/app/plugin_requests.go:271-280`, fix link inferred). |
| MMSA-2023-00227 | A member crashed the Todo plugin a few times and it stayed disabled. | Not shown in the server tree. The restart limit is what let crashes disable the plugin (`server/public/plugin/health_check.go:58-79`). |
| MMSA-2026-00638 | Malformed input to the plugin API caused a runtime panic. | Input validation in the API (advisory text only). |
| v12.0 React 19 | Web app plugins built for older React broke, and the server may suppress their activation. | Rebuild and set `min_server_version` 12.0.0 (`docs/develop/integrate/plugins/components/webapp/react-jsx-runtime.md:6-10`). |
| v10.5 Jira plugin 4.2.0 | A bundled plugin bug made plugin settings disappear. | Advice to skip 10.5.0 and 10.5.1 (`docs/main/administration-guide/upgrade/important-upgrade-notes.mdx:777`). |
| v10.11 prepackaged signatures | Prepackaged bundles had no verification. | Signatures required at startup, with custom keys for custom distributions (`docs/main/administration-guide/configure/plugins-configuration-settings.mdx:85`). |

Most plugin advisories in the feed are bugs in individual plugins, such as Boards, Playbooks, Jira, and Calls, that crash the plugin or skip a permission check. Of 84 advisories that mention plugins or sit under the plugin platforms, about 70 name one plugin (counted from the feed on 2026-11-12). The out-of-process model kept those crashes from taking down the server (inferred), but the plugins' own permission checks failed often.

## What changes in deployarr

1. **Keep the uploaded package as the source of truth and unpack it as a cache.** Copy the file store pattern from [Where files land](#where-files-land). deployarr stores each add-on package file in its content store and rebuilds the unpacked directory from it at startup. A half-written unpack then repairs itself on restart, which matches the crash-safe rule in `AGENTS.md`.
2. **Unpack each version beside the old one and switch only after it starts.** Avoid the delete-then-copy order in [Failure and rollback](#failure-and-rollback). deployarr unpacks to `addons/<id>/<version>/`, verifies, starts, and runs a health call, then records the new version as current. The old version stays on disk until the next successful upgrade, so rollback is one record change.
3. **Store enabled state apart from installed state and reconcile.** Copy `PluginStates` and `syncPluginsActiveState` from [Failure and rollback](#failure-and-rollback). deployarr keeps `enabled` in its SQLite database and runs one idempotent reconcile function at startup and after each change.
4. **Verify every package, including uploads, against trusted keys.** Avoid the split in [Unsigned handling](#unsigned-handling), where uploads skip verification and the only safe setting disables uploads. deployarr verifies every upload. A package signed by a trusted key installs. An unsigned or unknown-key package installs only when a server setting allows it and the admin confirms a warning. Verify before extraction, as `buildPrepackagedPlugin` does, and again at each startup.
5. **Use a modern detached signature, not OpenPGP.** Mattermost's OpenPGP library is deprecated upstream (inferred, [Signing and verification](#signing-and-verification)). deployarr signs the package file with Ed25519 (for example minisign or a raw signature file) and ships its own public key in the server binary, plus admin-added keys.
6. **Declare compatibility as a range of host API versions.** Avoid a minimum-only field ([Package format and manifest](#package-format-and-manifest)) and the v12.0 failure where unrebuilt plugins broke. The deployarr manifest names the add-on API version it was built for and a minimum server version. The server refuses an add-on whose API major version it does not support, at upload time, not at activation.
7. **Copy the per-platform executable map.** Use `os-arch` keys, as in `server.executables`, so one package covers `windows-x86_64` and `linux-x86_64` deployarr servers. If add-ons are WebAssembly components, one file covers both and the map is not needed.
8. **Negotiate capabilities once, at start.** Copy the `Implemented` list and the `ContextHTTP` flag ([How the API evolves](#how-the-api-evolves)). The add-on lists the extension points it provides, such as a lookup resolver, routes, or settings, and the host calls only those. New optional features are new fields that default to off. Hook and method IDs are append-only.
9. **Use a language-neutral interface, not Go `net/rpc` and `gob`.** Mattermost's protocol works only for Go plugins (inferred, because `net/rpc` encodes with Go's `gob`). deployarr defines the host API in an IDL (WIT for WebAssembly components, or protobuf for a process boundary), so add-ons can be written in Rust or another language and the host can check types at load.
10. **Grant permissions from the manifest.** Avoid an API that hands every add-on the whole config and every secret ([Isolation and permissions](#isolation-and-permissions), MMSA-2025-00528). A deployarr add-on declares what it needs, such as its own secrets, outbound hosts, or read access to identity facts. The host enforces the list and the admin sees it before enabling. No add-on reads deployarr's SQLite database directly, unlike Mattermost's `Driver`.
11. **Keep add-on secrets in the encrypted secret store.** Copy `"secret": true` in the settings schema, but avoid masking on read only ([Configuration and secrets](#configuration-and-secrets)). deployarr stores fields marked secret encrypted at rest, as [Server](../architecture.md#server) requires, never returns them to the UI, and passes them to the add-on only through a call scoped to that add-on.
12. **Render add-on settings from a declarative schema in Leptos.** Copy the typed setting list ([Declarative settings](#declarative-settings)). Avoid shipping add-on JavaScript into the admin UI, which shares the full app state and broke on React 19 ([Shipped code](#shipped-code)). If an add-on later needs a full page, load it in a sandboxed `iframe` on its own origin.
13. **Make agent-side contributions declarative too.** A Mattermost manifest names only a server part and a web app part (`server/public/model/manifest.go:218-222`). The SQL add-on's order-number prompt is data the server sends to `deployarr-agent`, such as a label, an input type, and validation rules. The agent never loads add-on code, so it stays one static executable for base WinPE.
14. **Authenticate in the host before an add-on route runs.** Copy the header stripping and avoid the gaps that advisories found ([HTTP routes](#http-routes)). deployarr applies sign-in, permission checks, CSRF, and a body size limit before forwarding to an add-on admin route, and passes a typed principal, never a spoofable header.
15. **Bound each add-on's resources and calls.** Mattermost sets no process limits and no host timeouts on hooks other than `OnDeactivate` ([Limits](#limits)). deployarr sets a timeout on every host-to-add-on call and a memory limit per add-on. A resolver that times out or crashes counts as an error, so the lookup chain moves to the next resolver, as [Data sources](../architecture.md#data-sources) already does for SQL errors.
16. **Show add-on state and restart with backoff, but do not let users trigger a silent disable.** Copy the explicit `FailedToStayRunning` state ([Health checks and restarts](#health-checks-and-restarts)). Avoid MMSA-2023-00227, where crashes from user input turned a plugin off. deployarr shows the state and last error in the UI, raises an admin alert, and keeps retrying with backoff.
17. **Refuse downgrades by default and show the direction.** Copy the conflict dialog that shows upgrade or downgrade. Avoid MMSA-2022-0095. A downgrade needs an explicit admin action.
18. **Keep the add-on directory apart from every upload path.** Avoid CVE-2025-9079. At startup, deployarr checks that the add-on directory does not overlap the content store, import folders, or any other folder that a UI action can write to.
19. **Limit extraction.** Copy the file type and path checks from [Bundle extraction](#bundle-extraction), and add limits on total decompressed size and file count.
20. **Version the add-on docs with the API.** Mattermost's live docs already describe a feature newer than the latest release. deployarr publishes add-on API docs per API version.

## Open questions

- Where Mattermost fixed MMSA-2022-0095. The server still sends a requested version to the Marketplace, so the fix may be in the Marketplace service.
- How the server or web app decides to suppress an unrebuilt web app plugin on v12.0. The web app code was not checked out.
- Whether deployarr add-ons are WebAssembly components or native processes. Items 7, 9, and 15 change shape with that choice.
