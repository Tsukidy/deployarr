# Jellyfin plugins 12.4 reference

This page records how Jellyfin 12.4 packages, installs, loads, isolates, and updates its server plugins, and what has gone wrong with that design. Jellyfin is a free software media server that streams media from a dedicated server to client apps ([site] `docs/general/about.md:9`). Its plugins are .NET assemblies that the server loads into its own process, from a catalog of JSON repositories or from a folder. deployarr uses this page as the in-process counterexample when it designs its add-on system. See [Components](../architecture.md#components) and [Server](../architecture.md#server) for the parts of deployarr that add-ons extend.

## Sources

Citations of the form `path:lines` refer to the repository named in brackets before the path. A citation with no bracket refers to `jellyfin`.

| Name | What | Version | How it was obtained |
| --- | --- | --- | --- |
| `jellyfin` | `https://github.com/jellyfin/jellyfin.git` | tag `v12.4`, commit `83d0d0e7f76e23e2e085c8ff67d28b772d35967d`, committed 2026-10-26 ("Bump version to 12.4") | `git clone --depth 1 --branch v12.4 --filter=blob:none`, then history deepened to 2023-01-01 for `git log` |
| `jellyfin-web` | `https://github.com/jellyfin/jellyfin-web.git` | tag `v12.4`, commit `0a580ce8df89282dfd120b6d5efb8e106da23bd5`, committed 2026-10-26 | blobless clone of the tag |
| `template` | `https://github.com/jellyfin/jellyfin-plugin-template.git` | default branch, commit `c93225a0a5a76d3843db05b4b5b77fcfc482fba3`, committed 2026-09-17. The repository has no release tags. | blobless clone |
| `jprm` | `https://github.com/oddstr13/jellyfin-plugin-repository-manager.git`, the packaging tool the 2020 blog post recommends | tag `v1.1.1`, commit `9497a0a499416cc572ed2e07a391d9f943a37b4d`, committed 2024-05-06 | blobless clone of the tag |
| `site` | `https://github.com/jellyfin/jellyfin.org.git`, the source of `https://jellyfin.org/docs` and `https://jellyfin.org/posts` | default branch, commit `e9d4ac098493f3f075b2fb80a23d3c3d3d58f65b`, committed 2026-11-12 | sparse clone of `docs` and `blog` |
| catalog | `https://repo.jellyfin.org/files/plugin/manifest.json` | fetched 2026-11-12. It redirected to `https://tor1.mirror.jellyfin.org/files/plugin/manifest.json`. The server reported last-modified 2026-10-19. The file is 201,833 bytes with SHA-256 `c1d1bb0c37d91708ee771176ee7f6151040baac284913fc2d41eea834aa6c555`. | `curl -L` |
| advisory | `GHSA-9p5f-5x8v-x65m` (CVE-2023-30626) | OSV record modified 2026-02-04 | `https://api.osv.dev/v1/vulns/GHSA-9p5f-5x8v-x65m`. `github.com` web pages and its API are blocked from this environment, so the advisory list on GitHub was not read. OSV queries for the NuGet packages `Jellyfin.Controller`, `Jellyfin.Common`, and `Jellyfin.Model` returned only this advisory and `GHSA-qwp3-5fw3-5wgv`, which does not concern plugins. |
| .NET docs | `https://learn.microsoft.com/en-us/dotnet/standard/threading/exceptions-in-managed-threads`, `https://learn.microsoft.com/en-us/dotnet/api/system.appdomain.unhandledexception?view=net-10.0`, and `https://learn.microsoft.com/en-us/dotnet/api/system.runtime.loader.assemblyloadcontext.loadunmanageddll` | fetched 2026-11-12 | `curl` |

Jellyfin 12.4 is the newest tag in `git ls-remote` on 2026-11-12. The tag is lightweight, so its date is the commit date. Version 12.0 replaced the old `10.x` numbering. "What would have been 10.12.0 is simply 12.0" ([site] `blog/2026/09-07-jellyfin-release-12.0/index.mdx:53`).

## Package format and manifest

### Catalog

- A repository is one JSON file at any URL. It holds an array of packages, and each package holds a list of versions ([site] `blog/2020/07-17-plugin-updates.mdx:18-45`).
- An admin adds a repository as a name, a URL, and an enabled flag (`MediaBrowser.Model/Updates/RepositoryInfo.cs:12-24`). The API replaces the whole list in one call (`Jellyfin.Api/Controllers/PackageController.cs:176-183`).
- Package fields are `guid`, `name`, `description`, `overview`, `owner`, `category`, `imageUrl`, and `versions` (`MediaBrowser.Model/Updates/PackageInfo.cs:29-81`).
- Version fields are `version`, `changelog`, `targetAbi`, `sourceUrl`, `checksum`, and `timestamp` (`MediaBrowser.Model/Updates/VersionInfo.cs:17-63`). The server adds `repositoryName` and `repositoryUrl` after it fetches the file (`VersionInfo.cs:68-75`, `Emby.Server.Implementations/Updates/InstallationManager.cs:118-124`).
- `checksum` is an MD5 hex digest. The installer compares it to the MD5 of the downloaded bytes (`InstallationManager.cs:582-595`). Every one of the 326 versions in the official catalog has a 32-character checksum (catalog, counted with a script).
- `sourceUrl` must end in `.zip`. Otherwise the installer logs an error and returns without installing (`InstallationManager.cs:525-529`). The caller then logs the plugin as installed, publishes the installed event, and asks for a restart (`InstallationManager.cs:326-344`, `InstallationManager.cs:644-652`).
- Neither the catalog nor the package has a field for operating system, CPU architecture, signature, publisher key, dependencies on other plugins, or a maximum host version (the model files above).

### Package contents

- A package is a zip of the plugin's DLLs, an optional image, and a `meta.json`. `jprm` builds it from a `build.yaml` ([jprm] `jprm/__init__.py:396-473`). The template's `build.yaml` has `name`, `guid`, `version`, `targetAbi`, `framework`, `overview`, `description`, `category`, `owner`, `artifacts`, and `changelog` ([template] `build.yaml:1-16`).
- `jprm` writes the zip's MD5 to a `.md5sum` file next to it. It writes no signature ([jprm] `jprm/__init__.py:463-469`).
- On disk, `meta.json` holds `category`, `changelog`, `description`, `guid`, `name`, `overview`, `owner`, `targetAbi`, `timestamp`, `version`, `status`, `autoUpdate`, `imagePath`, and `assemblies` (`MediaBrowser.Common/Plugins/PluginManifest.cs:33-124`). `status` and `autoUpdate` are host state that the server writes into the same file as the publisher's metadata.
- `assemblies` is an allow list of DLL paths. When it is empty, the server loads every `.dll` under the plugin folder, subfolders included (`Emby.Server.Implementations/Plugins/PluginManager.cs:891-934`).
- A plugin is .NET IL, so one zip serves Windows, Linux, and macOS. No per-platform packaging exists. The load context does not override `LoadUnmanagedDll` (`PluginLoadContext.cs:9-33`). The default implementation returns `IntPtr.Zero`, and the runtime then loads the library with its default policy (.NET docs, `LoadUnmanagedDll`). So a plugin's native libraries get no per-platform resolution from the host (inferred. Not tested).

### Compatibility

- `targetAbi` is a four-part version that the server compares with its own application version. The rule is "host version >= `targetAbi`" at load time (`PluginManager.cs:745-755`) and in the catalog filter (`InstallationManager.cs:131-143`, `InstallationManager.cs:200-204`, `InstallationManager.cs:269-271`).
- No upper bound exists. A plugin built for an old host passes the check on every newer host.
- A missing or unparsable `targetAbi` becomes `0.0.0.1`, which passes (`PluginManager.cs:745-748`, `InstallationManager.cs:131-134`).
- The template tells authors that a package reference version that does not match the server makes the plugin show as `NotSupported` ([template] `README.md:54-68`). The server sets the `NotSupported` status in one place, after a `TypeLoadException` or `ReflectionTypeLoadException` from `Assembly.GetTypes()` at startup (`PluginManager.cs:165-177`). A failed `targetAbi` check sets no status. It makes `IsEnabledAndSupported` false, and the loader skips the plugin (`PluginManager.cs:755`, `LocalPlugin.cs:73`, `PluginManager.cs:124-128`). The plugin then reports the status saved in its `meta.json` (`LocalPlugin.cs:116-121`).
- A plugin that calls a removed method but defines no type that fails to load passes both checks. It fails later, at the call (inferred from .NET binding rules. Not tested here).

## Install, upgrade, disable, and uninstall

### What the admin sees

- The dashboard lists the catalog. Installing sends `POST /Packages/Installed/{name}` with an optional GUID, version, and repository URL (`Jellyfin.Api/Controllers/PackageController.cs:105-140`). The plugin and package endpoints require an administrator (`PackageController.cs:22`, `PluginsController.cs:25`).
- The API has no upload endpoint. The other way to install is to copy files into the plugins folder and restart ([site] `docs/general/server/plugins/index.mdx:36-38`).
- The web client shows a warning before it installs a version whose repository URL does not start with `https://repo.jellyfin.org/` ([jellyfin-web] `src/apps/dashboard/routes/plugins/plugin.tsx:44-46`, `plugin.tsx:234-248`). The check is a string prefix in the browser. The server applies no such rule.
- The strings file has a warning for adding a repository, `MessageRepositoryInstallDisclaimer` ([jellyfin-web] `src/strings/en-us.json:1221`). No source file under `src` other than the strings files refers to it at this tag (`grep`).
- Every install, update, uninstall, enable, and disable takes effect only after a server restart. The installer calls `NotifyPendingRestart` (`InstallationManager.cs:344`, `InstallationManager.cs:408`), and the status `Restart` exists only in memory (`MediaBrowser.Model/Plugins/PluginStatus.cs:8-14`, `LocalPlugin.cs:116-121`). Since 10.9 the restart happens inside the process ([site] `blog/2024/05-11-jellyfin-release-10.9.0/index.mdx:75`).

### Where files land

- Plugins live in `<data>/plugins`, and plugin settings in `<data>/plugins/configurations` (`Emby.Server.Implementations/AppBase/BaseApplicationPaths.cs:58-61`). The settings folder sits inside the folder the loader scans. The loader skips it only because it holds no DLLs (`PluginManager.cs:870-871`).
- Each version gets its own folder, `<name>_<version>`. A source comment gives the reason as "Version folder as they cannot be overwritten in Windows" (`InstallationManager.cs:597-598`).
- The folder name comes from the package's display name. Since 12.0 the installer rejects names with path separators, invalid file name characters, `.`, or `..`, and checks that the resolved path stays under the plugins folder (`InstallationManager.cs:531-550`, `InstallationManager.cs:624-642`). The fix is commit `5104497331c0519c551e1af6b3999f0da0d65058`, "Reject unsafe plugin package names in installer", and the 12.0 notes list it as a security fix ([site] `blog/2026/09-07-jellyfin-release-12.0/index.mdx:154`).

### Order of install steps

`PerformPackageInstallation` (`InstallationManager.cs:523-622`) runs these steps:

1. Check that the URL ends in `.zip` and that the name is safe.
2. Download the whole package into memory, with a 10-minute budget (`InstallationManager.cs:36-38`, `InstallationManager.cs:552-577`).
3. Compare the MD5 with the catalog's checksum.
4. Delete the target version folder if it exists, and ignore any error (`InstallationManager.cs:600-612`).
5. Extract the zip straight into the final folder (`InstallationManager.cs:614-615`).
6. Merge the catalog metadata with the package's `meta.json` and write `meta.json` (`PluginManager.cs:388-430`, `PluginManager.cs:502-556`).
7. Add the plugin to the in-memory list and mark the older version `Superseded` (`PluginManager.cs:245-267`, `PluginManager.cs:940-967`).

### Failure during install

- A failed download, checksum, or extraction publishes a failure event and rethrows (`InstallationManager.cs:360-376`). Nothing removes a partly extracted folder.
- The server writes `meta.json` with `File.WriteAllText`, with no temporary file and rename (`PluginManager.cs:372-385`). Plugin images, by contrast, go to a temporary file and move into place (`PluginManager.cs:449-488`).
- After a crash during extraction, the next startup finds the partial folder. A partial folder with no `meta.json` gets an invented manifest with status `Active` and, as its GUID, an MD5 of the folder name without its `_<version>` suffix (`PluginManager.cs:759-786`). The loader then loads whatever DLLs it finds (inferred from these paths. Not tested).
- No rollback step exists. The previous version's folder survives until the next startup. Then the loader deletes every older version of a plugin whose newest version is enabled and supported (`PluginManager.cs:809-868`). If the new version then fails to load, the old one is already gone (inferred from the order. `Malfunctioned` is set after discovery, at `PluginManager.cs:136-189`).
- Through 10.10, a server setting, `RemoveOldPlugins`, controlled that cleanup. Commit `f1e020c0b061dce8af1a0483eaccd6eed22f9399` (2024-12-02) removed the setting. The commit is in tag `v10.11.0` and not in `v10.10.7` (`git merge-base --is-ancestor`).

### Disable and uninstall

- Disable and enable write the new status into `meta.json` and take effect after a restart (`PluginManager.cs:325-350`, `Jellyfin.Api/Controllers/PluginsController.cs:69-125`).
- Uninstall calls the plugin's `OnUninstalling`, then deletes its folder. When the delete fails, as with a loaded DLL on Windows, it writes status `Deleted`, and each startup retries the delete (`InstallationManager.cs:388-409`, `PluginManager.cs:108-117`, `PluginManager.cs:274-293`, `PluginStatus.cs:46-50`).
- Uninstall leaves the settings file in `plugins/configurations` (inferred. No code in `RemovePlugin` or `DeletePlugin` touches it). The docs rely on this when they tell users to reinstall a plugin to update it and keep its settings ([site] `docs/general/server/plugins/index.mdx:21-22`).
- A plugin in the server's own folder cannot be uninstalled (`MediaBrowser.Common/Plugins/BasePlugin.cs:54-55`).

### Automatic updates

- The scheduled task `PluginUpdates` runs at startup and every 24 hours (`Emby.Server.Implementations/ScheduledTasks/Tasks/PluginUpdateTask.cs:60-72`). It installs the newest compatible version of every plugin whose `autoUpdate` is true and whose status is not `Disabled` or `Deleted` (`InstallationManager.cs:501-521`).
- `autoUpdate` defaults to true for catalog installs (`PluginManifest.cs:99-100`, `PluginManager.cs:419`). A folder with no `meta.json` gets false (`PluginManager.cs:780`).
- An update that fails with an HTTP, I/O, timeout, or invalid-data error does not stop the others. The task catches only those exception types, so any other exception ends the run (`PluginUpdateTask.cs:86-118`).

### Status values

| Status | Value | Meaning in code | Set by |
| --- | --- | --- | --- |
| `Restart` | 1 | Memory only. A change waits for a restart. | `LocalPlugin.cs:119` |
| `Active` | 0 | Loaded, or will load. | `PluginManager.cs:325-334`, `PluginManager.cs:648` |
| `Disabled` | -1 | Admin turned it off. | `PluginManager.cs:340-350` |
| `NotSupported` | -2 | A type in the plugin's assemblies failed to load at startup. A `targetAbi` above the host does not set it (see [Compatibility](#compatibility)). | `PluginManager.cs:172-176` |
| `Malfunctioned` | -3 | A DLL failed to load, a constructor or service registration threw, the allow list named a missing file, or the package GUID did not match `meta.json`. | `PluginManager.cs:136-157`, `PluginManager.cs:223-237`, `PluginManager.cs:661-676`, `PluginManager.cs:820-825`, `PluginManager.cs:524-528` |
| `Superseded` | -4 | Another version of the same GUID became active. | `PluginManager.cs:940-967` |
| `Superceded` | -4 | Deprecated misspelling, kept as an alias of `Superseded`. | `PluginStatus.cs:41-44` |
| `Deleted` | -5 | Delete failed. Each startup retries it. | `PluginManager.cs:284-290`, `PluginManager.cs:864-867` |

`IsEnabledAndSupported` is true only for a status of 0 or more and a passing `targetAbi` check (`LocalPlugin.cs:73`). A `Malfunctioned` plugin stays off for the rest of the process, and the server has no retry action. The status does not always last, though. Each catalog refresh calls `PopulateManifest` for every installed version that a catalog lists, with that plugin's current status. `PopulateManifest` writes any status other than `Disabled` to disk as `Active` (`InstallationManager.cs:190-198`, `PluginManager.cs:406-429`). So a `Malfunctioned` plugin that a catalog lists loads again at the next restart (inferred from these paths. Not tested).

## Runtime model

- Plugins run inside the server process. Each enabled plugin gets a collectible `AssemblyLoadContext` (`PluginLoadContext.cs:17-20`, `PluginManager.cs:130-131`). The contexts unload only when the plugin manager is disposed at shutdown (`PluginManager.cs:433-439`).
- The load context returns null, and so falls back to the default context, for any assembly that its `AssemblyDependencyResolver` does not resolve (`PluginLoadContext.cs:23-31`). Plugins therefore share the server's copies of `Jellyfin.Controller`, `Jellyfin.Model`, and the framework. The template excludes those packages' runtime assets for this reason ([template] `Jellyfin.Plugin.Template/Jellyfin.Plugin.Template.csproj:12-19`, [template] `README.md:54-55`).
- Startup order is this:
  1. Load assemblies and call `GetTypes()` on each (`PluginManager.cs:106-191`).
  2. Collect every exported concrete type (`Emby.Server.Implementations/ApplicationHost.cs:754-788`).
  3. Run each `IPluginServiceRegistrator`, which adds services to the dependency injection container (`PluginManager.cs:206-239`, `ApplicationHost.cs:492`).
  4. Create each `IPlugin` (`PluginManager.cs:196-199`).
  5. Create every implementation of each extension interface, such as metadata providers, resolvers, and search providers (`ApplicationHost.cs:720-749`).
- An exception while the host builds a plugin type marks the plugin `Malfunctioned`. A dependency injection loop does the same (`ApplicationHost.cs:312-348`).
- After startup, no boundary exists. The only process-wide handler logs an unhandled exception as critical (`Jellyfin.Server/Program.cs:109-111`). In .NET, an unhandled exception on a thread "causes the application to terminate" in most cases (.NET docs, exceptions in managed threads). So a plugin's background thread that throws stops the whole server (inferred from these two sources).
- A failure inside a scheduled task, including a plugin's, is caught and recorded as `Failed` (`Emby.Server.Implementations/ScheduledTasks/ScheduledTaskWorker.cs:330-345`).
- No health check, watchdog, memory limit, CPU limit, or per-plugin timeout exists in the plugin code read for this page.
- Plugins log through the server's `ILogger`, into the server log (inferred from the dependency injection model in [template] `README.md:179`).

## Host-to-plugin API

- The transport is direct .NET calls. A plugin implements interfaces from `Jellyfin.Controller` and `Jellyfin.Model`, and receives server services through constructor injection ([template] `README.md:150-191`).
- A plugin can add HTTP endpoints. The host adds every assembly that contains a `ControllerBase` subclass to the main ASP.NET application (`ApplicationHost.cs:1010-1022`, `Jellyfin.Server/Extensions/ApiServiceCollectionExtensions.cs:160`). Nothing places plugin routes under a plugin prefix (inferred. No prefix appears in that code).
- A plugin can add hosted services that run for the life of the process ([template] `README.md:175`).
- No version negotiation exists. The only version signal is `targetAbi`, a minimum, compared with the host's product version (see [Compatibility](#compatibility)).
- The plugin interfaces change in major releases, and plugins must be rebuilt:
  - 10.6 renamed members such as `IExternalID.Name` and moved `User` to another namespace ([site] `blog/2020/07-17-plugin-updates.mdx:75-88`).
  - 10.9 added a parameter to `IPluginServiceRegistrator.RegisterServices`, commit `1e1e1560a47439c02931e67736bcd87b606cf35c` (2023-11-09).
  - 10.11 required all database access to go through EF Core and refused raw SQL ([site] `blog/2025/10-19-jellyfin-release-10.11.0/index.mdx:148`).
  - 12.0 moved to .NET 10 and changed `ISearchEngine`, `IAuthenticationProvider.HasPassword`, parts of `IItemRepository`, and `IUserManager` members ([site] `blog/2026/09-07-jellyfin-release-12.0/index.mdx:222-225`).
- The HTTP API has a stated policy. "Deprecations will normally be marked for an entire major release cycle before removal" ([site] `blog/2026/09-07-jellyfin-release-12.0/index.mdx:218`). The sources read for this page state no such policy for plugin interfaces.
- The project ships a separate catalog for pre-release servers "due to compatibility issues", and asks testers to add it by hand ([site] `blog/2024/03-25-testing-10.9.0/index.mdx:51`, [site] `blog/2026/05-24-state-of-the-fin/index.mdx:59-64`).

## Isolation and permissions

- None. A plugin runs with the server's identity and can reach every file, network host, and server service the process can. It can resolve services such as `IUserManager`, `IServerConfigurationManager`, and `ITaskManager` ([template] `README.md:179-191`).
- The project says so. "Plugins may also grant even more access, including full shell access should the plugin decide to" ([site] `blog/2023/11-23-jellyfin-security-and-you/index.mdx:20`).
- Plugin settings are an XML file per plugin, by default `plugins/configurations/<assembly name>.xml` (`MediaBrowser.Common/Plugins/BasePluginOfT.cs:125-131`). The server serializes the settings object to that file as plain XML (`BasePluginOfT.cs:143-171`). No secret type, encryption, or per-plugin access rule exists in this code.
- An admin saves settings through `POST /Plugins/{pluginId}/Configuration`. The server deserializes the body into the plugin's own configuration type and calls the plugin (`PluginsController.cs:188-206`).
- Plugins can reach each other's types through the shared default load context and the shared container (inferred from `PluginLoadContext.cs:23-31` and `PluginManager.cs:206-239`).

## UI contributions

- A plugin implements `IHasWebPages` and lists pages, each with a name and the path of an embedded resource (`MediaBrowser.Model/Plugins/IHasWebPages.cs:7-10`, `MediaBrowser.Model/Plugins/PluginPageInfo.cs:6-37`). The template embeds `configPage.html` ([template] `Jellyfin.Plugin.Template/Plugin.cs:39-50`, [template] `Jellyfin.Plugin.Template/Jellyfin.Plugin.Template.csproj:27-30`).
- The server streams that resource as HTML or JavaScript (`Jellyfin.Api/Controllers/DashboardController.cs:73-94`).
- The web client fetches the page and inserts it into the dashboard with `innerHTML`. When the page has a `<script>`, the client appends it through jQuery so the script runs ([jellyfin-web] `src/components/ServerContentPage.tsx:38-41`, [jellyfin-web] `src/components/viewContainer.js:53-76`, [jellyfin-web] `src/components/viewContainer.js:122-132`). The script runs in the web client's origin with the signed-in admin's session (inferred from same-origin insertion).
- This is shipped code, not a declarative schema. The host keeps code that looks for jQuery calls in plugin pages ([jellyfin-web] `src/components/viewContainer.js:149-157`). The 10.6 release removed two jQuery extensions that plugin pages used, and authors had to rewrite their pages ([site] `blog/2020/07-17-plugin-updates.mdx:66-73`).

## Trust

- Integrity is an MD5 from the same catalog that names the download URL (`InstallationManager.cs:582-595`). The catalog URL may use any scheme that the HTTP client accepts (`InstallationManager.cs:104-174`). The official catalog redirects to a mirror (catalog headers).
- No signing exists in the server, the package format, or `jprm` (the sources above).
- Loading checks nothing about origin. The loader loads every DLL it finds in each plugin folder, or the allow listed ones when `meta.json` names them (`PluginManager.cs:793-934`). A folder without `meta.json` still loads (`PluginManager.cs:759-786`).
- The only trust signal is the web client's repository URL prefix check and its warning text ([jellyfin-web] `src/strings/en-us.json:1216`).
- The catalog has no stated review process in the sources read. Anyone can host a repository ([site] `blog/2020/07-17-plugin-updates.mdx:12-19`).

## Known failures and lessons

### A file write became code execution

CVE-2023-30626 (`GHSA-9p5f-5x8v-x65m`, fixed in 10.8.10) chained a path traversal in `/ClientLog/Document`, a stored XSS, and the FFmpeg path setting. The uploaded file ran when the XSS payload set it as the encoder path, and it created a plugin folder with a DLL in it. After the server started again, it loaded the DLL as a plugin, which registered an endpoint that ran system commands (advisory). The reporter notes that the server scans plugin subfolders only for `.dll` files, so the `.log` file that the traversal wrote could not load as a plugin by itself (advisory). A DLL written into a plugin subfolder loads at the next start, because the loader verifies nothing about a plugin's origin (`PluginManager.cs:759-786`, `PluginManager.cs:891-934`).

### Plugins break on every major upgrade

- The 12.0 notes tell users to remove third-party plugins before they upgrade, because "Plugins built for 10.11 will not load on 12.0" ([site] `blog/2026/09-07-jellyfin-release-12.0/index.mdx:41`). The project said the same before the 12.0 database migration ([site] `blog/2026/05-24-state-of-the-fin/index.mdx:59`).
- The server cannot tell a 10.11 plugin from a 12.x plugin by `targetAbi`, because the check has no upper bound. On 2026-11-12 the official catalog offered these versions to a 12.4 server, by the server's own selection rule:
  - Bookshelf `13.0.0.0` with `targetAbi` `10.11.0.0`. The 12.0 notes say this plugin is deprecated ([site] `blog/2026/09-07-jellyfin-release-12.0/index.mdx:113`).
  - AniSearch `6.0.0.0` with `targetAbi` `10.11.0.0`.
  - Folio `1.0.0.0` with `targetAbi` `10.12.0.0`, a version number that no server release uses after the switch to 12.0.
  - These come from a script that applied the rule in `InstallationManager.cs:269-282` to the catalog file. The other 33 packages offer a version with `targetAbi` 12.0 or later.
- The template still pins `Jellyfin.Controller` 10.11.5, `net9.0`, and `targetAbi` `10.11.0.0` two months after 12.0 moved to .NET 10 ([template] `Jellyfin.Plugin.Template/Jellyfin.Plugin.Template.csproj:4-18`, [template] `build.yaml:5-6`).

### State bugs in the plugin manager

- Disabled plugins came back on after a restart until 12.0 ([site] `blog/2026/09-07-jellyfin-release-12.0/index.mdx:179`). The fix, commit `a272efb9a21720293199a0e3740f8529fd2abf46` (2026-08-03), shows the cause. The code had written the memory-only `Restart` status and `autoUpdate = false` into the same manifest object that it saves to disk, and an update re-enabled a disabled plugin. The fix keeps a disabled plugin disabled when an update imports it, moves the restart flag to its own field, `LocalPlugin.RestartRequired`, and deletes older versions only when the newest one is enabled and supported (`PluginManager.cs:259-264`, `PluginManager.cs:829-833`, `PluginManager.cs:951-953`).
- Catalog lookups filtered on the name and the GUID together. Commit `4dc826644d213ee47f4181009be3c928448b0879` (backport of PR 15197, "Filter plugins by id instead of name", 2025-10-27) made the lookup use the GUID alone when one is given, and the name only when no GUID is given (`InstallationManager.cs:236-243`).
- A repository URL with an unsupported scheme crashed the server. Commit `cf9051c27773ffa764a785fddd045b001f5861ad` (backport of PR 15961, 2026-01-10) added the missing `NotSupportedException` handler (`InstallationManager.cs:164-168`).
- `GetCompatibleVersions` still calls `Version.Parse` on catalog text, while the other paths use `TryParse` (`InstallationManager.cs:269-271`). A malformed `targetAbi` in a catalog can therefore throw during an install or an update check (inferred. Not tested).
- The catalog refresh rewrites the `meta.json` of every installed plugin version that a catalog lists (`InstallationManager.cs:190-198`). A source comment says the refresh runs on every dashboard visit. A 12.0 change stopped it from downloading every image each time (`PluginManager.cs:398-403`, commit `e1e319cb5c5bf3ccfd8a6a81ab616aa05299522b`, "Only download missing plugin images", 2026-09-07).

### The default repository migrations

Three migrations manage the default repository. The first replaces the whole repository list with the official one (`Jellyfin.Server/Migrations/Routines/20250420090000_AddDefaultPluginRepository.cs:35`). The second adds it back only when the list is empty (`20250420110000_ReaddDefaultPluginRepository.cs:36-38`). The third rewrites the old official URL to the new one (`20250420170000_UpdateDefaultPluginRepository.cs:14-15`, `20250420170000_UpdateDefaultPluginRepository.cs:29-44`). Admins keep a hard-coded catalog URL in their settings, so a move of the project's own hosting needs a server release (inferred from the third migration).

### Features moved between the server and plugins

10.9 moved DLNA out of the server into a plugin, and its notes tell DLNA users to install the plugin after they upgrade ([site] `blog/2024/05-11-jellyfin-release-10.9.0/index.mdx:47`). 12.0 moved most of Bookshelf into the server, split the rest into GoogleBooks and ComicVine providers, and deprecated the plugin ([site] `blog/2026/09-07-jellyfin-release-12.0/index.mdx:91-113`).

## What changes in deployarr

1. Run add-ons outside the server's own code. In Jellyfin, one plugin's unhandled exception on a background thread ends the process (see [Runtime model](#runtime-model)). The deployarr server serves PXE, TFTP, and the agent API while PCs image. An add-on crash must not stop a deployment. Copy Jellyfin's auto-disable on a startup failure. Avoid its shared process.
2. Give the add-on API its own version, separate from the product version, and declare a range. Jellyfin compares `targetAbi` with the product version and checks only a minimum. Its own catalog offered 10.11 plugins to a 12.4 server, and the 10.12-to-12.0 rename left a plugin marked `10.12.0.0` (see [Plugins break on every major upgrade](#plugins-break-on-every-major-upgrade)). A deployarr manifest names the API major version it was built against. The server refuses a package outside the range it supports, before install and again at load.
3. Verify every add-on at load, not only at install. CVE-2023-30626 turned a file write into code execution because the loader trusts any DLL in a plugin folder (see [A file write became code execution](#a-file-write-became-code-execution)). Record each installed add-on's SHA-256 in the server's database and refuse to start an add-on whose files do not match. Never discover add-ons by scanning a folder.
4. Sign packages and verify the signature on the server. Jellyfin's only integrity check is an MD5 from the catalog that also names the download (see [Trust](#trust)). When an admin uploads a package file, no catalog vouches for it. Check a publisher signature over a SHA-256 digest in the server, not in the browser. Let an admin install an unsigned package only after an explicit confirmation that the server records.
5. Make install a staged, crash-safe step. Jellyfin extracts into the final folder, writes `meta.json` in place, and deletes the old version before the new one is known to load (see [Failure during install](#failure-during-install)). Extract to a staging folder, verify, rename into a folder named by add-on ID and version, then commit one database row. At startup, delete any folder that no committed row names. Keep the previous version until the new one has started, so the admin can roll back. This follows the rule that every state change is safe to run again after a crash.
6. Keep the admin's choice apart from the runtime state. Jellyfin stored a memory-only status in its saved manifest and re-enabled disabled plugins until 12.0. Its catalog refresh still writes `Active` over a saved failure status (see [State bugs in the plugin manager](#state-bugs-in-the-plugin-manager) and [Status values](#status-values)). Store the desired state (installed version, enabled) in the database. Keep the observed state (running, failed with a reason, incompatible) in memory and show both.
7. Copy the status distinctions, with reasons. `NotSupported`, `Malfunctioned`, `Superseded`, and `Deleted` tell an admin what happened. Add the error text and the time to each failure, and add a retry action.
8. Install, enable, disable, and remove add-ons without a server restart. Jellyfin needs a restart for each change (see [What the admin sees](#what-the-admin-sees)). A deployarr restart drops PXE and agent sessions. A separate add-on runtime can start and stop one add-on at a time, after its in-flight calls finish.
9. Turn automatic updates off by default. Jellyfin updates every plugin at startup and every 24 hours unless the admin opts out (see [Automatic updates](#automatic-updates)). A changed order lookup add-on in the middle of an imaging day changes which PCs get wiped. Offer updates, and never apply one while a deployment that uses the add-on is running.
10. Render add-on settings from a declarative schema in the Leptos UI. Jellyfin injects plugin HTML and script into the admin dashboard, and the host carries jQuery shims for old plugin pages (see [UI contributions](#ui-contributions)). A deployarr add-on declares its settings fields, with types and validation, and the UI draws them with the same typed forms as the rest of the [Web UI](../architecture.md#web-ui). No add-on script runs in the admin origin.
11. Store add-on secrets with the server's other secrets. Jellyfin writes plugin settings, passwords included, to plain XML in a folder that every plugin can read (see [Isolation and permissions](#isolation-and-permissions)). An SQL order lookup add-on holds database passwords. The manifest marks secret fields. The server encrypts them with the server key from [Server](../architecture.md#server) and gives each add-on only its own values, at call time.
12. Grant capabilities from the manifest. A Jellyfin plugin can resolve any server service and add any HTTP route. Give a deployarr add-on only the hooks it declares, such as a lookup resolver, a settings page, an agent prompt, or outbound hosts. Mount its admin routes under a prefix with the add-on's ID, behind the server's own sign-in check.
13. Identify add-ons by a validated ID, never by display name. Jellyfin needed a 12.0 security fix for unsafe package names and a fix to match catalog packages on GUID alone (see [Install, upgrade, disable, and uninstall](#install-upgrade-disable-and-uninstall) and [State bugs in the plugin manager](#state-bugs-in-the-plugin-manager)). Parse the ID with a strict grammar where the package enters the server, and derive folder names from the parsed ID and version.
14. Choose a runtime with one portable artifact, or make per-target artifacts a manifest field. Jellyfin needs no per-platform packaging because .NET IL runs everywhere (see [Package contents](#package-contents)). deployarr's server runs on Windows and Linux. A WebAssembly add-on gives the same single-artifact property. A native-process add-on needs a list of artifacts keyed by target in the manifest.
15. Check add-on compatibility before a server upgrade. Jellyfin tells users to remove plugins by hand before a major upgrade. The deployarr installer can read the installed add-ons, compare them with the new server's supported API range, and name the add-ons that the upgrade will disable before it proceeds.
16. Ship the add-on SDK and template from the server workspace, tested in CI against the current API. Jellyfin's template lagged a major release behind the server (see [Plugins break on every major upgrade](#plugins-break-on-every-major-upgrade)).
17. Publish a deprecation policy for the add-on API, as Jellyfin does for its HTTP API but not for plugins. Keep an old API major working for at least one server major release after its replacement ships, so an admin can upgrade the server and the add-ons separately.

## Open questions

- Whether the runtime's default native library policy finds a library inside a plugin folder is untested. The load context never asks its `AssemblyDependencyResolver` for native libraries, because it does not override `LoadUnmanagedDll`.
- Whether a 10.11 plugin on 12.4 fails at load with `NotSupported` or later at a call depends on which members it uses. This page loaded no plugin.
- Whether Folio's `targetAbi` of `10.12.0.0` means "built for 12.0" is unknown.
- The GitHub security advisory list for `jellyfin/jellyfin` could not be read from this environment. An advisory for the unsafe package name fix may exist.
