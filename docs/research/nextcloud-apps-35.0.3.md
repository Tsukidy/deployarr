# Nextcloud apps 35.0.3 reference

This page records how Nextcloud Server 35.0.3 declares, checks, installs, migrates, signs, disables, and removes apps, and how its AppAPI runs External Apps (ExApps) as separate containers. Nextcloud is a self-hosted PHP server where an admin installs apps from an app store or drops an app folder on disk. Its apps own database tables and carry their migrations across server majors. deployarr uses this page to design add-on compatibility ranges, add-on storage and migrations, signing and revocation for offline servers, and the out-of-process add-on boundary. The design it informs is in [Components](../architecture.md#components), [Order lookup](../architecture.md#order-lookup), [Data sources](../architecture.md#data-sources), [Server](../architecture.md#server), and [Web UI](../architecture.md#web-ui).

## Sources

- Nextcloud Server tag `v35.0.3` of `https://github.com/nextcloud/server.git`. The annotated tag object is `aeb222e38c8c95fdbb324f4e29bf9dd926d83416` and points at commit `7764ef4927e5f85071e495ce7602ccd247bb6b30`, committed 2026-11-12. `git ls-remote --tags` on 2026-11-12 listed no tag above `v35.0.3` without an `rc` or `beta` suffix. `version.php:14-17` holds `[35, 0, 3, 2]` and `35.0.3`. Cloned with `git clone --depth 1 --branch v35.0.3 --filter=blob:none`.
- AppAPI tag `v35.0.3` of `https://github.com/nextcloud/app_api.git`, commit `3a67175b59f1d84fb110d35e99832ef706a6f310`, committed 2026-11-02. Its `appinfo/info.xml:40` says version `35.0.0` and `:60` pins Nextcloud 35 as both minimum and maximum. The server ships AppAPI and enables it by default (`core/shipped.json:5`, `:63`).
- Documentation branch `stable35` of `https://github.com/nextcloud/documentation.git`, commit `e7d4e1ba150dbea23df244e260b2cc8598318e8d`, committed 2026-11-07. The branch name marks it as the documentation for release 35 (inferred). Sparse clone of `developer_manual` and `admin_manual`.
- Advisories come from the NVD API, `https://services.nvd.nist.gov/rest/json/cves/2.0?keywordSearch=Nextcloud`, fetched 2026-11-12, which returned 395 records. Each cited record links a `GHSA` advisory in `nextcloud/security-advisories` or a HackerOne report. The GitHub advisory pages could not be read from this environment.
- Paths starting `lib/`, `core/`, `apps/`, `resources/`, or `config/` are in the server tag. Paths starting `app_api/` are in the AppAPI tag. Paths starting `developer_manual/` or `admin_manual/` are in the documentation branch. Line numbers are at those commits.

## Manifest and dependencies

Each app has `appinfo/info.xml`. The docs say an XML Schema validates it (`developer_manual/app_development/info.rst:11`), and the server tag carries a copy of the schema in `resources/app-info.xsd`. No PHP code in the server tag loads that file (inferred from a search for `app-info.xsd`).

| Element | Rule | Citation |
| --- | --- | --- |
| `dependencies/nextcloud` | Required, with required `min-version` and required `max-version` | `resources/app-info.xsd:630-631`, `:644-647` |
| `dependencies/php` | Optional `min-version`, `max-version`, and `min-int-size` (32 or 64) | `resources/app-info.xsd:677-695` |
| `dependencies/database` | `sqlite`, `mysql`, or `pgsql`, repeatable, with optional version range | `resources/app-info.xsd:414-420`, `:666-675` |
| `dependencies/lib`, `command`, `architecture` | PHP extension with version range, shell command name, CPU architecture | `resources/app-info.xsd:624-633`, `:697-704`, `lib/private/App/PlatformRepository.php:26-27` |
| `repair-steps` | `pre-migration`, `post-migration`, `live-migration`, `install`, `uninstall` | `resources/app-info.xsd:706-719` |
| `external-app` | ExApp container `registry`, `image`, `image-tag`, plus environment variables | `resources/app-info.xsd:721-756` |

- `DependencyAnalyzer::analyze` checks architecture, PHP, database, commands, libraries, OS, and server version, and returns a list of human-readable missing items (`lib/private/App/DependencyAnalyzer.php:34-49`).
- The analyzer reads only the database name. The schema declares a database `min-version` and `max-version`, but the analyzer never compares them (`lib/private/App/DependencyAnalyzer.php:162-185`).
- Version comparison truncates both versions to the shorter length before `version_compare`. So `max-version="35"` accepts every 35.x.y server (`lib/private/App/DependencyAnalyzer.php:67-90`).
- The docs say the server checks PHP, database, and library dependencies "at installation time (not on update time)" (`developer_manual/app_development/info.rst:291`). The code also runs the full analyzer on every app upgrade (`lib/private/App/AppManager.php:1144-1146`, `:1272-1289`).

## Compatibility and server upgrades

- Install refuses an app whose server range excludes the running version, then runs the full dependency check (`lib/private/Installer.php:79-91`).
- An admin can force-enable an incompatible app. The web API's `force` flag and `occ app:enable --force` add the app ID to the system value `app_install_overwrite`, which makes the analyzer skip the maximum only (`apps/appstore/lib/Controller/ApiController.php:166-171`, `core/Command/App/Enable.php:94-101`, `lib/private/App/AppManager.php:445-451`, `:646-648`, `lib/private/Installer.php:78-79`).
- A force-enable expires. When an upgrade crosses a major version, the updater deletes `app_install_overwrite` (`lib/private/Updater.php:217-220`). Disabling or removing the app also drops its override (`apps/appstore/lib/Controller/ApiController.php:209`, `:238`).
- The app list marks each app `isCompatible` from the maximum alone, and returns `missingDependencies` from the full analyzer (`apps/appstore/lib/Controller/ApiController.php:134-137`).
- At a server upgrade, `checkAppsRequirements` disables every enabled app whose range excludes the new version, and emits `incompatibleAppDisabled`. A shipped app that fails the check aborts the upgrade instead. The check does nothing on the `git` channel (`lib/private/Updater.php:362-381`).
- That check ignores `app_install_overwrite`, so it also disables a force-enabled app at every upgrade, minor or major (`lib/private/Updater.php:373`, `lib/private/App/AppManager.php:1218-1220`).
- The updater remembers each auto-disabled app's previous enable state, including group limits (`lib/private/App/AppManager.php:729-736`). It then asks the app store for a newer release of every enabled and every auto-disabled app and installs it. For an auto-disabled app with no newer release, it runs the app's own upgrade, which honors `app_install_overwrite`. If either succeeds, it restores the saved state. If either throws, it logs the error and the app stays disabled (`lib/private/Updater.php:259-265`, `:397-429`, `lib/private/App/AppManager.php:1144-1146`). So a force-enabled app comes back after a minor upgrade and stays off after a major one (inferred from the same lines).
- Before an upgrade, the update notification page lists enabled non-shipped apps that have no release for the target version. It needs the app store and answers HTTP 404 with `appstore_disabled` when the store is off (`apps/updatenotification/lib/Controller/APIController.php:72-131`).

## Install, update, disable, and remove

### From the app store

`Installer::downloadApp` runs these steps (`lib/private/Installer.php:162-350`):

1. Load the app's certificate from the store's catalog and check it against the bundled CRL. A revoked serial fails the install (`:178-200`).
2. Check that the bundled root issued the certificate (`:202-210`) and that its `CN` equals the app ID (`:212-230`).
3. Download the release `.tar.gz` and verify the release signature over the whole archive with SHA-512 (`:232-245`).
4. Extract to a temporary folder. Refuse an archive with no folder or more than one top-level folder (`:247-284`).
5. Refuse an `info.xml` whose `id` differs from the requested ID (`:286-307`), and refuse a lower version than the installed one (`:309-320`).
6. Delete the old app folder, then copy the new one in place (`:322-331`). There is no staging rename and no rollback.

`installApp` then checks compatibility and runs `installAppLastSteps`, which runs migrations, registers background jobs, runs `install` repair steps, and records `installed_version` (`lib/private/Installer.php:64-96`, `:536-592`).

- The web endpoints for enable, uninstall, update, and bundle install require a strict password confirmation. Disable requires a non-strict one (`apps/appstore/lib/Controller/ApiController.php:164-165`, `:204-205`, `:227-228`, `:255-256`, `:284-285`).
- Updating one app from the web UI turns on maintenance mode for the whole server until the update ends (`apps/appstore/lib/Controller/ApiController.php:257-271`).

### Without the app store

- The admin UI has no upload (inferred). The app store API has routes for categories, list, enable, disable, uninstall, update, and bundles, and none takes a file (`apps/appstore/lib/Controller/ApiController.php:71-286`). The app's other routes serve the page and the discover feed (`apps/appstore/lib/Controller/PageController.php:48-50`, `apps/appstore/lib/Controller/DiscoverController.php:63`, `:82`).
- `apps_paths` lists app folders, and `writable` marks the one where installs land (`admin_manual/apps_management.rst:128-156`). When an app folder is already present, `occ app:enable` and the web enable skip `downloadApp` and go straight to `installApp` (`core/Command/App/Enable.php:89-94`, `apps/appstore/lib/Controller/ApiController.php:173-178`).
- `installApp` checks only compatibility and dependencies. It verifies no signature, so a folder copied onto disk installs unsigned (`lib/private/Installer.php:64-96`).
- `appstoreenabled` false, or `has_internet_connection` false with the default store URL, empties the catalog (`lib/private/App/AppStore/Fetcher/Fetcher.php:131-140`). `appstoreurl` can point at a self-hosted store (`admin_manual/apps_management.rst:189-215`).
- `appsallowlist` limits the catalog to listed IDs, but only on instances with a valid subscription (`lib/private/App/AppStore/Fetcher/AppFetcher.php:162-170`).

### Disable and remove

- Disable sets `enabled` to `no` and then runs the app's `uninstall` repair steps (`lib/private/App/AppManager.php:725-752`). The docs confirm that `uninstall` steps run "when uninstalling the app, and when disabling the app" (`developer_manual/digging_deeper/repair.rst:114`).
- The updater's automatic disable of an incompatible app calls the same method, so it also runs `uninstall` steps (`lib/private/Updater.php:377`).
- Remove deletes the app folder only. "The function will not delete preferences, tables and the configuration" (`lib/private/Installer.php:438-467`). The web uninstall disables first and then removes (`apps/appstore/lib/Controller/ApiController.php:229-245`).
- `occ app:remove` disables first unless `--keep-data` is set. Its help says data removal depends on each app's `uninstall` steps (`core/Command/App/Remove.php:39-56`, `:78-99`).
- No remove path calls `IAppConfig::deleteApp`, and no path deletes the app's rows in the `migrations` table (inferred from a search of `lib`, `core`, `apps/appstore`, and `apps/settings` for `deleteApp(`).

## Database migrations and repair steps

- An app keeps migration classes in `lib/Migration`, named `Version<number>Date<timestamp>` (`lib/private/DB/MigrationService.php:67-68`, `:217-247`). The docs map app version `2.34.x` to number `2034` so parallel branches can add migrations (`developer_manual/basics/storage/migrations.rst:184-186`).
- Ordering sorts by the number, then by the date string (`lib/private/DB/MigrationService.php:200-212`).
- The shared `migrations` table has the primary key `(app, version)`. A step runs once, then the service inserts its row (`lib/private/DB/MigrationService.php:154-158`, `:254-292`).
- `IMigrationStep` has `preSchemaChange`, `changeSchema`, and `postSchemaChange`, and no down method (`lib/public/Migration/IMigrationStep.php:25-61`). Steps are forward-only. The docs tell authors never to edit a migration that has shipped (`developer_manual/basics/storage/migrations.rst:28-32`).
- `changeSchema` returns a desired schema, and the server diffs it against the live database. Before applying it, the server enforces a 63-character limit on new table, column, index, foreign key, and primary key names (`lib/private/DB/MigrationService.php:505-513`, `:526-610`). It also checks that index, foreign key, and primary key names are unique. A collision fails a fresh install but only logs a message during an upgrade (`lib/private/DB/MigrationService.php:729-774`).
- No transaction wraps a whole step. `Migrator::applySchema` wraps only the schema diff, and on MySQL it opens no transaction at all (`lib/private/DB/Migrator.php:113-132`). `preSchemaChange`, the schema change, `postSchemaChange`, and the `migrations` row run one after another (`lib/private/DB/MigrationService.php:492-524`). A failure in `postSchemaChange` leaves the schema change applied and the step unrecorded.
- A fresh install takes a different path. It folds every `changeSchema` into one target schema and skips every `preSchemaChange` and `postSchemaChange` (`lib/private/Installer.php:544-552`, `lib/private/DB/MigrationService.php:408-445`). Data-moving code in a post step never runs on a new install.
- Large index additions should not go in a migration. An app registers them through `AddMissingIndicesEvent`, and the admin applies them later (`developer_manual/basics/storage/migrations.rst:225`).
- Repair steps run around migrations at upgrade. `live-migration` steps become background jobs that run at an unknown later time (`lib/private/App/AppManager.php:1149-1160`, `developer_manual/digging_deeper/repair.rst:115-117`). Since 34, `IRepairStepExpensive` marks steps that run only when the admin asks (`lib/public/Migration/IRepairStepExpensive.php:10-20`).

## Code signing and integrity

- The server bundles a root and an intermediate CA in `resources/codesigning/root.crt`, both valid from 2016 to 2041. The intermediate has `pathlen:0`. A CRL in `resources/codesigning/root.crl`, issued by the intermediate, lists 141 revoked serials. Its `lastUpdate` is 2026-11-12 (read with `openssl` from the tag's files).
- The root issues each certificate for one scope, usually the app ID, through its `CN`, not for a developer. A developer posts a CSR with `/CN=<appid>` as a pull request to `nextcloud/app-certificate-requests` (`developer_manual/app_publishing_maintenance/code_signing.rst:61-64`, `:121-125`). For a lost key, Nextcloud issues a new certificate and revokes the old one (`:138-140`).
- `occ integrity:sign-app` writes `appinfo/signature.json`, which holds a SHA-512 hash for every file, the certificate, and a signature over the JSON of the hashes (`developer_manual/app_publishing_maintenance/code_signing.rst:66-99`, `:127-136`). The signature is RSA-PSS with a SHA-1 message hash, MGF1-SHA-512, and salt length 0 (`lib/private/IntegrityCheck/Checker.php:319-327`).
- Verification reports `FILE_MISSING`, `EXTRA_FILE`, and `INVALID_HASH` per file (`lib/private/IntegrityCheck/Checker.php:342-375`).
- The installed-code check never reads the CRL. It checks only the chain to the root and the `CN` (`lib/private/IntegrityCheck/Checker.php:275-312`). Only `downloadApp` loads `root.crl` (`lib/private/Installer.php:187`, the only match for `root.crl` in `lib`, `core`, and `apps`). A revoked certificate stops new downloads but not the check of an app already on disk.
- The `CN` check also accepts a certificate whose `CN` is `core` for any app (`lib/private/IntegrityCheck/Checker.php:306-311`).
- The check covers shipped apps and any app that contains `signature.json`. An unsigned third-party app is not checked (`lib/private/IntegrityCheck/Checker.php:545-564`).
- Enforcement is off on the `git` and empty release channels, and an undocumented `integrity.check.disabled` turns it off (`lib/private/IntegrityCheck/Checker.php:61-73`).
- The check runs after an upgrade, from a background job, and from the admin overview, and stores results for a setup warning (`lib/private/Updater.php:286-291`, `core/BackgroundJobs/CheckCodeIntegrityJob.php:62`, `apps/settings/lib/SetupChecks/CodeIntegrity.php:43-47`). No caller refuses to load an app that fails (inferred from those callers).
- The rule "signed once, signed forever" is an app store policy, not a server check (`developer_manual/app_publishing_maintenance/code_signing.rst:108-111`).

## ExApps (AppAPI)

- An ExApp is an app written in a language other than PHP that uses the AppAPI OCS API, and an orchestration daemon such as Docker deploys it (`developer_manual/exapp_development/Introduction.rst:4`, `:33-34`). AppAPI has been installed by default since 30.0.1 (`admin_manual/exapps_management/AppAPIAndExternalApps.rst:19-21`).
- A deploy daemon runs the containers. The recommended one is HaRP, a proxy container. Docker Socket Proxy and a manual mode also exist, and AppAPI has Kubernetes actions (`admin_manual/exapps_management/AppAPIAndExternalApps.rst:42-75`, `app_api/lib/DeployActions/KubernetesActions.php`).
- Lifecycle endpoints have fixed timeouts. `GET /heartbeat` must answer within 10 minutes, `POST /init` has a 40-minute default, and `PUT /enabled?enabled=1|0` has 30 seconds. The ExApp reports `/init` progress through `PUT /ocs/v2.php/apps/app_api/ex-app/status`. On `enabled=1` the ExApp registers its UI entries and `occ` commands, and on `enabled=0` it removes them (`developer_manual/exapp_development/development_overview/ExAppLifecycle.rst:37-95`, `:133-136`).
- Registration makes a 128-character random secret unless the app info already holds one (`app_api/lib/Command/ExApp/Register.php:146-147`) and passes it to the container as the environment variable `APP_SECRET` (`app_api/lib/DeployActions/DockerActions.php:1141-1152`).
- An ExApp authenticates with `AUTHORIZATION-APP-API`, a base64 `userid:secret` (`developer_manual/exapp_development/tech_details/Authentication.rst:36-41`). AppAPI compares the secret and then sets the session user to any user ID the ExApp names (`app_api/lib/Service/AppAPIService.php:298-351`).
- A request authenticated this way skips two-factor checks, CORS checks, and, with no user, rate limits. It may call endpoints marked `AppApiAdminAccessWithoutUser` (`lib/private/Authentication/TwoFactorAuth/Manager.php:272`, `lib/private/AppFramework/Middleware/Security/CORSMiddleware.php:58`, `lib/private/AppFramework/Middleware/Security/RateLimitingMiddleware.php:77-80`, `lib/private/AppFramework/Middleware/Security/SecurityMiddleware.php:124-134`).
- AppAPI once had per-ExApp API scopes. Later migrations drop the scope tables and column (`app_api/lib/Migration/Version2204Date20240403125002.php:29-30`, `app_api/lib/Migration/Version2207Date20240502145029.php:29-30`, `app_api/lib/Migration/Version3200Date20240905080316.php:34-35`). The `--force-scopes` option remains, marked deprecated (`app_api/lib/Command/ExApp/Register.php:53`).
- The store archive with `info.xml` gets the same CRL, root, `CN`, and SHA-512 checks as a PHP app (`app_api/lib/Fetcher/ExAppArchiveFetcher.php:38-56`, `:135-197`). The manifest names the container image by registry, image, and tag only (`resources/app-info.xsd:721-727`). The deploy code has no digest or image-signature check (inferred from a search of `app_api/lib/DeployActions/` for `digest`, `sha256`, and `cosign`, which finds nothing).
- `occ app_api:app:register --info-xml` reads a local file or URL, and `--json-info` takes the app info inline. Neither path checks a signature (`app_api/lib/Command/ExApp/Register.php:54-55`, `app_api/lib/Service/ExAppService.php:254-285`).
- Unregister keeps the data volume unless `--rm-data` is set (`admin_manual/exapps_management/ManagingExApps.rst:50-63`, `app_api/lib/Command/ExApp/Unregister.php:53-54`).
- During maintenance mode with no upgrade pending, the server keeps serving `/apps/app_api` traffic so ExApps keep running (`lib/OC.php:1154-1164`).

## Security advisories

| ID | Component | What happened |
| --- | --- | --- |
| CVE-2026-45545, GHSA-x43f-gmgh-vvjj | Tables app | Stored SQL injection, first limited to 20 bytes, then escaped the limit |
| CVE-2026-45722, GHSA-5h2w-c7px-hp4j | Tables app | SQL injection through an unsanitized `ORDER BY` column |
| CVE-2024-52523, GHSA-42w6-r45m-9w9j | Server, external storage | The API returned stored storage credentials to the browser in plain text |
| CVE-2024-52518, GHSA-vrhf-532w-99rg | Server, external storage | A stolen session could change storage settings without password confirmation |
| CVE-2026-77164 | Circles app | Unauthenticated blind SSRF. Signature verification fetched an attacker URL and allowed private addresses, bypassing the core SSRF guard |
| CVE-2026-45156, GHSA-qqgv-fqwp-mjpp | User OIDC app | Missing signature verification let an ID4me authority sign in as any user |

The table's descriptions come from the NVD records named in [Sources](#sources). The Tables `ORDER BY` case matters for SQL lookups. A sort column is an identifier, and SQL placeholders bind values, not identifiers (inferred, general SQL behavior).

## What changes in deployarr

These changes follow from the findings above. They extend the earlier pages on [Grafana](grafana-plugins-13.3.1.md) and [Jellyfin](jellyfin-plugins-12.4.md) and do not repeat their points on signed file manifests, staged installs, or declarative settings pages.

### Copy

1. Require both ends of the server range in the manifest, and compare only to the precision the add-on states, so `max = "1"` means every 1.x. Nextcloud requires `max-version` and truncates the comparison. See [Manifest and dependencies](#manifest-and-dependencies).
2. Let an admin force-enable an add-on past its maximum, record the override per add-on ID, and clear it at the next server major. Show the reason on the add-on's page. See [Compatibility and server upgrades](#compatibility-and-server-upgrades).
3. At a server upgrade, disable each incompatible add-on with its previous state saved, and restore that state once a compatible version is installed. Report the list in the upgrade log. See [Compatibility and server upgrades](#compatibility-and-server-upgrades).
4. Keep a per-add-on migration ledger keyed by `(addon_id, migration_id)`, forward-only, with no down steps. Order migrations by an explicit number, not by file name. See [Database migrations and repair steps](#database-migrations-and-repair-steps).
5. Have the host own add-on tables. An add-on declares its schema, and the host applies it under a prefix built from the add-on ID and checks names. Then the host can list and purge an add-on's data itself. Nextcloud's own name-length checks show the host can validate a declared schema. See [Database migrations and repair steps](#database-migrations-and-repair-steps).
6. Scope each signing certificate to one add-on ID, as Nextcloud does with `CN`. A leaked key then cannot sign a different add-on. Leave out Nextcloud's exception that trusts a `core` certificate for every app. See [Code signing and integrity](#code-signing-and-integrity).
7. Require password confirmation on the server for add-on install, update, enable, and remove, and on any change to add-on secrets or outbound hosts. CVE-2024-52518 shows the cost of a missing confirmation. See [Security advisories](#security-advisories).
8. Give long add-on setup a timeout and a progress report, as AppAPI does for `/init`. Show progress in the add-on's status. See [ExApps (AppAPI)](#exapps-appapi).
9. Keep add-on data on removal by default, and make purge a separate, confirmed action that lists what it deletes, like `--keep-data` and `--rm-data`. See [Disable and remove](#disable-and-remove).

### Avoid

1. Never run destructive add-on code on disable. Nextcloud runs `uninstall` steps on every disable, including the updater's automatic disable, which also hits force-enabled apps at every upgrade. A server upgrade could delete an incompatible add-on's data. deployarr disable stops the add-on and changes no data. See [Disable and remove](#disable-and-remove).
2. Do not run a migration outside a transaction. Nextcloud wraps only the schema diff, never the data steps or the ledger row, and wraps nothing on MySQL. deployarr stores its data in SQLite ([Server](../architecture.md#server)), where every SQL command except a few `PRAGMA` statements runs inside a transaction ([SQLite transactions](https://www.sqlite.org/lang_transaction.html)). Apply each add-on migration and its ledger row in one transaction, so a crash leaves neither. See [Database migrations and repair steps](#database-migrations-and-repair-steps).
3. Do not use a different migration path for a fresh install. Nextcloud skips data steps on a new install. Run the same ordered steps on install and on upgrade. See [Database migrations and repair steps](#database-migrations-and-repair-steps).
4. Do not let revocation depend on a download. Nextcloud checks its CRL only when it downloads an app, and the installed-code check ignores it. deployarr checks revocation when it loads an add-on, at server start and at each enable. See [Code signing and integrity](#code-signing-and-integrity).
5. Do not tie revocation to server upgrades alone. Nextcloud's CRL ships inside the server, so an offline server learns of a revoked key only by upgrading. Accept a signed revocation list as a file the admin imports through "Add add-on...", and keep the newest one by its sequence number. See [Code signing and integrity](#code-signing-and-integrity).
6. Do not skip signature checks for a file-based install. Nextcloud verifies only store downloads, so a copied folder or a local ExApp `info.xml` runs unsigned. deployarr has only the file path, so the signature check runs there, and an unsigned add-on needs an explicit admin override. See [Without the app store](#without-the-app-store).
7. Do not make the integrity check a report only. Nextcloud shows a warning and still loads a modified app. deployarr refuses to start an add-on whose files fail the check. See [Code signing and integrity](#code-signing-and-integrity).
8. Do not let "signed once, signed forever" depend on a store. Record the signing certificate's add-on ID and issuer at first install, and refuse an update that is unsigned or signed for another ID. See [Code signing and integrity](#code-signing-and-integrity).
9. Do not put the whole server in maintenance mode for one add-on update. deployarr is serving PXE and agents. Drain the add-on's in-flight calls, swap it, and leave other services running. See [From the app store](#from-the-app-store).
10. Do not give an out-of-process add-on one unscoped secret. An ExApp secret can act as any user and skips two-factor and rate limits, and AppAPI removed its scopes. Give each add-on a credential that allows only the host calls its manifest declares, never staff identities. See [ExApps (AppAPI)](#exapps-appapi).
11. Do not pass add-on credentials in environment variables. Any process that can read the container or process environment reads `APP_SECRET`. Hand the credential over the add-on's private channel at start (inferred risk, not a published advisory).
12. Do not sign metadata that names mutable code. AppAPI signs `info.xml` but pulls the image by tag. Put the SHA-256 of every executable or component in the signed manifest. See [ExApps (AppAPI)](#exapps-appapi).
13. Do not build SQL text from add-on settings or lookup input, including sort columns and table names. Bind values, and check identifiers against an allowlist the admin set. The Tables advisories show both cases. This applies to the SQL order lookup add-on in [Data sources](../architecture.md#data-sources). See [Security advisories](#security-advisories).
14. Do not return stored add-on secrets to the browser, as CVE-2024-52523 did. Send set or not set only. See [Security advisories](#security-advisories).
15. Route all add-on outbound requests through one host egress policy that the add-on cannot widen. The Circles SSRF bypassed the core guard from inside an app. This applies to the webhooks, Autopilot, and driver catalog add-ons. See [Security advisories](#security-advisories).

### Open questions

- Who runs the CA that issues per-add-on certificates, and who publishes the revocation list for offline servers? Nextcloud GmbH holds both roles for Nextcloud.
- Should deployarr let add-ons own SQL tables at all, or only a host key-value store? Nextcloud's table ownership is what makes removal leave data behind.
- Does a force-enabled add-on keep its override across a server minor release? Nextcloud keeps the override within a major, but still disables the app and runs its `uninstall` steps at each upgrade before it turns the app back on.
- Should the pre-upgrade compatibility report work offline from the add-on files already on disk? Nextcloud's report needs the app store.
