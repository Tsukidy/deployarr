# Add-on design attacks on wasmtime 50.0.1

This page records the adversarial runs against deployarr's add-on design. A design session wrote the design that [#83](https://github.com/Tsukidy/deployarr/issues/83) applies to the architecture. Four runs then each tried to break it with real code and raised 23 findings. The design session reran each finding, merged the 23 into 15 items, and changed the design for every item. Its scratch files are gone. This page reruns the findings again with the probes in [`tools/addon-design-attacks/`](../../tools/addon-design-attacks/README.md). Use it when you check an add-on claim in the architecture. The design cites these runs as "(run, item N)" and "(run, supported claims)". Each item below keeps the design's number.

[Wasmtime 50.0.1 SQL add-on reference](wasmtime-50.0.1.md) records an earlier prototype on the same `wasmtime` version, in [`tools/addon-wasm-prototype/`](../../tools/addon-wasm-prototype/README.md). Items 1, 5, 8, and 12 and the claim about the cost of each call cover ground that page measured or left open, and each one links to it.

## Sources

Each source has a short label. Citations name the label, then a path, a line range, or a heading in the output.

| Label | Source | Date | How it was obtained |
| --- | --- | --- | --- |
| findings | [First comment on #82](https://github.com/Tsukidy/deployarr/issues/82#issuecomment-6832512545): the design session's `interrogate-findings.md`, then its listing of the 23 findings with each one's folder and command | recovered 2026-11-12 | the owner recovered it from the archived session `session_01CqN69UTDFg6W77ALchYUG1` |
| sources | [Second comment on #82](https://github.com/Tsukidy/deployarr/issues/82#issuecomment-6832574008): which probe sources survive, and their text | recovered 2026-11-12 | the same |
| reruns | [Third comment on #82](https://github.com/Tsukidy/deployarr/issues/82#issuecomment-6832605181): the design session's reruns, with commands and output. Each section starts with `### [N]`. | recovered 2026-11-12 | the same |
| design | [First comment on #83](https://github.com/Tsukidy/deployarr/issues/83#issuecomment-6832653114): `design.md` after the changes | recovered 2026-11-12 | the same |
| wit | [The comment on #87](https://github.com/Tsukidy/deployarr/issues/87#issuecomment-6832822615): `sketch/wit/addon.wit`, `sketch/wit/host.wit`, and `sketch/host/lib.rs` after the changes | recovered 2026-11-12 | the same |
| core | [First comment on #86](https://github.com/Tsukidy/deployarr/issues/86#issuecomment-6832729154): `sketch/core/addon.rs` after the changes | recovered 2026-11-12 | the same |
| triage77 | [The triage comment on #77](https://github.com/Tsukidy/deployarr/issues/77#issuecomment-6833124673) | 2026-11-12 | a separate triage of the MySQL read-only guard |
| lever | `tools/addon-design-attacks/` on this page's branch | 2026-11-12 and 2026-11-13 | written for this page |
| run | `$SCRATCH/t82/run.out` and the build logs in `$SCRATCH/t82/run/` | 2026-11-13 | `bash tools/addon-design-attacks/reproduce.sh $SCRATCH/t82/run`, run from a `git archive` of commit `9f11bf9`, which holds the lever's final probes and scripts. The pull request's Evidence section holds the whole output. |
| crates | crate sources under `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/` | the versions named below | downloaded by Cargo |

`$SCRATCH` is a scratch directory outside the repository, as in [Reproduce the MDT research](../../AGENTS.md#reproduce-the-mdt-research). Each `### [N]` heading in reruns and each section heading in run names the finding it holds.

The design session ran its probes in folders named `opus`, `fable`, `sonnet`, and `haiku`, one per adversarial run (findings listing). This page names each finding's run by its folder.

The probes and servers had these versions. `### build` in run prints the crate versions from each `Cargo.lock`. `### environment` prints `rustc` and Python, and the SQL sections and `model/waiting_op.py` print the server and SQLite versions.

| Item | Version |
| --- | --- |
| `wasmtime`, `wasmtime-wasi` | 50.0.1 |
| `tokio` | 1.54.1 |
| `wat` | 1.263.0 |
| `rand` | 0.10.3 |
| `serde`, `serde_json` | 1.0.229, 1.0.154 |
| `toml` | 1.2.1 |
| `url` | 2.5.8 |
| `minisign`, `minisign-verify` | 0.10.0, 0.3.0 |
| `rustc` | 1.99.1 |
| Python, and its SQLite | 3.13.18, 3.53.1 |
| MariaDB | 10.11.14, from Ubuntu 24.04's packages |
| PostgreSQL | 16.15, from Ubuntu 24.04's packages |

The machine ran Linux 6.18 with 4 CPUs and 16,086 MiB of memory, shared with other agents (run, `### environment`). Times and memory figures on this page are single runs on that machine. They are not benchmarks.

## How the probes were rebuilt

- The sources label lists which probe sources survive. These are copied unchanged, apart from a note that says where each came from. The note is a header comment in the Rust files and a paragraph added to the docstring of the Python file:
  - `sql/mysql-ro.sh`, `sql/mysql-ro-prepared.sh`, `sql/mysql-ro-session.sh`, and `sql/pg-ro.sh`
  - `probes/src/bin/serde-action.rs` and `probes/src/bin/serdecheck.rs`
  - the 80 lines of `model/secret_step_file.py` that survive
- `wasmtime/src/bin/blocking-host-fn.rs` follows the surviving cases A to D. It moves the blocking host function into a helper that case E also uses, and it drops the call to `Config::async_support`, which `wasmtime` 50 made a no-op.
- Every other probe is rewritten from its finding, its command, and the output of the design session's rerun. A rewritten probe tests the same attack. Its code and its exact numbers can differ from the lost one.
- Some probes also run the design's change, so the page can say whether the change works. Those lines are marked as the design's change in each probe's output.
- `reproduce.sh` builds every probe and runs them in the order of this page. `setup-servers.sh` installs and starts MariaDB and PostgreSQL first.
- Six findings read the first draft of the design, its plan, or its sketch. The design session's `design.md`, `plan.md`, and `sketch/` were edited after the findings, and the first draft is gone. Those findings are marked "not rerun". Their output on this page comes from reruns.
- No SQL Server ran, because a cloud session cannot run it.

## Findings by item

| Item | What it found | Findings | This page |
| --- | --- | --- | --- |
| [1](#item-1-memory-limits-do-not-bound-an-add-on) | Memory limits do not bound an add-on | 1 (`opus`), 9 (`fable`) | rerun |
| [2](#item-2-read-only-enforcement-depends-on-the-engine) | Read-only enforcement depends on the engine | 2 (`opus`), 21 (`sonnet`) | rerun, except SQL Server |
| [3](#item-3-an-upgrade-can-replace-the-admins-sql) | An upgrade can replace the admin's SQL | 3 (`opus`), 10 (`fable`) | rerun |
| [4](#item-4-wipe-authority-cannot-come-from-free-text) | Wipe authority cannot come from free text | 4 (`opus`), 13 (`fable`) | not rerun |
| [5](#item-5-a-compiled-cache-is-code-on-disk) | A compiled cache is code on disk | 5 (`opus`), 15 (`fable`), 19 (`sonnet`) | rerun for 5, not rerun for 15 and 19 |
| [6](#item-6-stored-steps-lose-bytes-and-fields) | Stored steps lose bytes and fields | 6 (`opus`), 23 (`haiku`) | rerun |
| [7](#item-7-the-permit-scopes-blocked-the-sketched-add-ons) | The permit scopes blocked the sketched add-ons | 7 (`opus`) | not rerun |
| [8](#item-8-add-on-calls-can-starve-the-servers-runtime) | Add-on calls can starve the server's runtime | 8 (`fable`), 22 (`haiku`) | rerun |
| [9](#item-9-an-add-on-can-read-its-own-secret-file) | An add-on can read its own secret file | 11 (`fable`) | rerun |
| [10](#item-10-the-manifest-parser-accepts-unknown-keys) | The manifest parser accepts unknown keys | 12 (`fable`) | rerun |
| [11](#item-11-a-pure-call-sees-the-clock-and-fresh-randomness) | A pure call sees the clock and fresh randomness | 14 (`fable`) | rerun |
| [12](#item-12-compiling-a-hostile-component-exhausts-the-server) | Compiling a hostile component exhausts the server | 16 (`sonnet`) | rerun |
| [13](#item-13-a-guest-path-can-redirect-credentials) | A guest path can redirect credentials | 17 (`sonnet`) | rerun |
| [14](#item-14-a-waiting-upgrade-blocks-every-other-operation) | A waiting upgrade blocks every other operation | 18 (`sonnet`) | rerun, except one step |
| [15](#item-15-stage-cannot-compare-exports-with-the-manifest) | `stage` cannot compare exports with the manifest | 20 (`sonnet`) | not rerun |

### Item 1. Memory limits do not bound an add-on

The earlier prototype did not test memory or CPU limits on the guest ([Wasmtime 50.0.1 SQL add-on reference](wasmtime-50.0.1.md#not-tested)).

#### Finding 1. `StoreLimits` caps each memory, not the store

- **Attack.** One component holds 16 core instances of one module. Each instance has a 1000-page memory, 62.5 MiB, and a start function that writes a byte to every 4 KiB page. The store has `StoreLimitsBuilder::new().memory_size(64 << 20)`.
- **Command.** The design session ran `opus-probe 16` in `opus/wt` (findings listing, [1]). Its source is lost (sources). This page reruns `memory-limits multi-instance 16`, rewritten (lever `wasmtime/src/bin/memory-limits.rs`).
- **Result.** One store held 1000 MiB under the 64 MiB limit. The design session measured the same, `RSS 14 MiB -> 1014 MiB (+1000 MiB held by one store)` (reruns, `### [1]`).

  ```text
  multi-instance: 16 core instances x 62.5 MiB under a 64 MiB StoreLimits: instantiate OK; RSS 14 MiB -> 1014 MiB (+1000 MiB held by one store)
  one-big-memory: 1 core instance x 68.75 MiB under a 64 MiB StoreLimits: instantiate refused: memory minimum size of 1100 pages exceeds memory limits; RSS 14 MiB -> 14 MiB
  grow-1gib: one memory.grow of 16384 pages (1 GiB) under a 64 MiB StoreLimits returned -1
  ```

  The second line is a control. It shows that the limit holds for one memory. The third line shows why the plan's test of one 1 GiB allocation passes either way (run, `### findings 1 and 9`).
- **Root cause.** The doc comment of `StoreLimitsBuilder::memory_size` says that the limit "is applied to each linear memory individually" (crates `wasmtime-50.0.1/src/runtime/limits.rs:230-241`). A store allows 10,000 instances and 10,000 memories by default (`limits.rs:3-8`, `:323-333`).
- **Design change.** A host `ResourceLimiter` sums every memory and every table in the store against one budget. It also caps the number of instances, memories, tables, and table elements. The plan's hostile tests include the multi-instance case and the table case (findings, item 1). The sketch has this as `Budget` (wit, `sketch/host/lib.rs`).

#### Finding 9. Tables are memory that the limit does not count

- **Attack.** One core module declares 8 tables of 30,000,000 `funcref` elements each, under the same 64 MiB limit.
- **Command.** The design session ran `wt table-bomb 8` and `wt table-bomb 1` in `fable` (findings listing, [9]). `fable/wt` is lost apart from a few lines (sources). This page reruns `memory-limits table-bomb 8` and `memory-limits table-bomb 1`, rewritten. Each runs the module twice, once with no code that touches the tables, and once with a start function that runs `table.fill` with a null reference over every element.
- **Result.** Untouched tables added no resident memory. Filled tables held 1831 MiB for 8 tables and 229 MiB for 1 table, under the 64 MiB limit. A `table.grow` by 50,000,000 elements succeeded and added 381 MiB (run, `### findings 1 and 9`). The design session measured 16 MiB to 1847 MiB and 16 MiB to 245 MiB at instantiation (reruns, `### [9]`), and +382 MiB for the same `table.grow` (reruns, `### [1]`).

  ```text
  table-grow: table.grow by 50000000 elements under a 64 MiB StoreLimits grew to 50000001 funcref elements (returned 1); RSS 14 MiB -> 395 MiB (+381 MiB)
  table-bomb: 8 tables x 30,000,000 funcref under a 64 MiB StoreLimits, untouched: instantiate OK; RSS 10 MiB -> 10 MiB (+0 MiB)
  table-bomb: 8 tables x 30,000,000 funcref under a 64 MiB StoreLimits, filled with null by a start function: instantiate OK; RSS 14 MiB -> 1845 MiB (+1831 MiB)
  table-bomb: 1 tables x 30,000,000 funcref under a 64 MiB StoreLimits, untouched: instantiate OK; RSS 10 MiB -> 10 MiB (+0 MiB)
  table-bomb: 1 tables x 30,000,000 funcref under a 64 MiB StoreLimits, filled with null by a start function: instantiate OK; RSS 14 MiB -> 243 MiB (+229 MiB)
  ```

  The untouched run did not reproduce the design session's result. Its tables were resident right after instantiation, and this page's untouched tables added nothing. Only tables that a start function filled reached the same size. The lost probe must have touched its tables or used other settings, and its source is gone, so this cannot be checked (inferred). The finding still holds, because a guest can fill its own tables.
- **Root cause.** `memory_size` covers linear memories only. `StoreLimits` limits tables through `table_elements`, which applies to each table alone and is unlimited by default (crates `wasmtime-50.0.1/src/runtime/limits.rs:243-253`, `:323-333`). `wasmtime` initializes tables lazily by default (`wasmtime-50.0.1/src/config.rs:2217-2226`). That fits the untouched run, where a table's memory became resident only when code wrote to it (inferred, not read in the allocator).
- **Design change.** The same budget as finding 1 counts table bytes. Because a guest can fill an untouched table later, the budget has to count a table's declared size, not its resident memory (inferred from the two runs).

### Item 2. Read-only enforcement depends on the engine

#### Finding 2. A MariaDB read-only transaction lets DDL and file writes run

- **Attack.** As `root` on MariaDB, run single write statements inside `START TRANSACTION READ ONLY`, then count the tables and rows. Repeat with a server-side prepared statement, with `SET SESSION TRANSACTION READ ONLY` instead, and with `SELECT ... INTO OUTFILE`.
- **Command.** The design session ran `bash mysql-ro.sh; bash mysql-ro-prepared.sh; bash mysql-ro-session.sh` in `opus/sql` (findings listing, [2]). All three survive and are unchanged (sources). The `INTO OUTFILE` command did not survive. This page reruns the three scripts and `sql/mysql-ro-outfile.sh`, rewritten.
- **Result.** On MariaDB 10.11.14 (run, `### finding 2`):

  | Statement | `START TRANSACTION READ ONLY` | `SET SESSION TRANSACTION READ ONLY` |
  | --- | --- | --- |
  | `INSERT` | refused, `ERROR 1792` | not tried |
  | `DELETE` | refused, `ERROR 1792` | refused, `ERROR 1792` |
  | `TRUNCATE TABLE` | ran, rows went from 2 to 0 | refused, `ERROR 1792` |
  | `DROP TABLE` | ran, the table is gone | refused, `ERROR 1792` |
  | `DROP TABLE` through `PREPARE` and `EXECUTE` | ran, the table is gone | not tried |
  | `SELECT ... INTO OUTFILE` | the server wrote a file of 2 lines | the server wrote a file of 2 lines |

  `ERROR 1792` reads `Cannot execute statement in a READ ONLY transaction`. `secure_file_priv` was not set (`NULL`). The design session's rerun shows the same last lines of `mysql-ro.sh` and `mysql-ro-session.sh` (reruns, `### [2]`). It did not rerun `mysql-ro-prepared.sh`.

  The design marked `INTO OUTFILE` under the session setting as untested (design, "What stops a write"). triage77 had already found the file written under that setting and under the same setting made before each query, on MariaDB 10.11.14 and MySQL 8.0.46. This run confirms it on MariaDB.
- **Root cause.** A DDL statement commits the open transaction and then runs, so it runs outside the read-only transaction (triage77, which found the same on MySQL 8.0.46; not read in MariaDB's source here). `INTO OUTFILE` writes a file on the database server, not a row, so neither guard applies to it. Only the `FILE` privilege and `secure_file_priv` stop it (inferred, and triage77 reached the same conclusion).
- **Design change.** The design no longer calls "never writes" a property of the interface. The read-only account is the guard that holds on every engine. On MySQL and MariaDB, the host adds `SET SESSION TRANSACTION READ ONLY`. The M5 test runs `TRUNCATE` and `DROP` with an account that may write (findings, item 2). The recovered `host.wit` says this in its comment on `deployarr:sql` (wit, `sketch/wit/host.wit`). triage77 later found that a setting made once per connection can be undone by a named query, so it sets the guard before each query and rolls back after it. It also requires that the read-only account not hold the `FILE` privilege. #77 owns both changes.

#### Finding 21. SQL Server has no engine-level read-only setting

- **Attack.** Compare the design's claim, "There is no `execute`, so 'deployarr never writes to the order database' is a property of the interface", with the architecture's sentence for SQL Server and with the `query` function in `host.wit`.
- **Command.** The design session ran `bash sqlserver-readonly.sh` in `sonnet/journal` (findings listing, [21]). The script survives (sources). It greps the first draft's `design.md` and `host.wit`, which are gone, and `docs/architecture.md`. This page reruns only the grep of `docs/architecture.md`, and it looks for the phrase "relies on the read-only account alone", so a rewording around the phrase still matches. If the phrase is gone, the section says so and `reproduce.sh` exits 1. The rest is not rerun, because its input is gone and no SQL Server can run in a cloud session.
- **Result.** In `docs/architecture.md` on `main` at `6113add`, the sentence is at line 343 (run, `### finding 21`):

  ```text
  343:- The order database is read-only to deployarr. [...] On SQL Server, deployarr relies on the read-only account alone.
  ```

  The design session's grep found the claim at `design.md:39`, the sentence at `docs/architecture.md:277`, and `query: func(connection: string, query: string, params: ...)` at `host.wit:73` (reruns, `### [21]`).
- **Root cause.** The interface limits which statements run, not what they do (findings, item 2). SQL Server offers no read-only transaction mode to fall back on. That is the design's statement, and no run checked it (inferred).
- **Design change.** The same as finding 2. On SQL Server the design states that the read-only account holds. The host runs each statement in a transaction and always rolls it back, which is inferred to undo DDL and untested until M5 (design, "What stops a write").

### Item 3. An upgrade can replace the admin's SQL

#### Finding 3. A settings migration rewrites the admin's statement

- **Attack.** An add-on's new version returns, from `migrate.config("settings")`, a settings document with its own SQL in place of the admin's `sql_text` value.
- **Command.** The design session listed `bash named-query-bypass.sh` in `opus` (findings listing, [3]). The script is lost, and reruns has no section for it. The design session covered it with finding 10's model. This page reruns the attack with the same model as finding 10.
- **Result.** See finding 10.
- **Root cause.** The first draft's host stored whatever `migrate.config("settings")` returned. It did not treat `sql_text` values as the admin's (findings, item 3).
- **Design change.** See finding 10.

#### Finding 10. The add-on supplies SQL after all

- **Attack.** The admin saves `SELECT os_code FROM order_rows WHERE board_serial = :value`. The host checks it at save, and the add-on can only name it by settings path. Version 2's settings migration puts `SELECT name || ' earns ' || salary FROM payroll WHERE :value <> ''` at the same path.
- **Command.** The design session ran `bash sql/pg-ro.sh >/dev/null 2>&1; python3 model/named_query_migrate.py` in `fable` (findings listing, [10]). The model is lost (sources). Which database it queried is not known. This page reruns `python3 -I model/named_query_migrate.py`, rewritten. It runs the statements in an in-memory SQLite database that stands in for the order database.
- **Result.** The upgrade replaced the statement, and the next resolve returned the payroll rows (run, `### findings 3 and 10`). The design session's output has the same lines (reruns, `### [10]`).

  ```text
  admin typed:             SELECT os_code FROM order_rows WHERE board_serial = :value
  v1 resolve returns:      (['W11-PRO'], '')
  save-time check works:   two statements refused: more than one statement
  after the upgrade, the statement the host holds: SELECT name || ' earns ' || salary FROM payroll WHERE :value <> ''
  admin typed it:          False
  v2 resolve returns:      (['alice earns 91000', 'bob earns 87000'], '')
  with the design's path check, the upgrade is refused: the migration changes the admin's statement at rules/0/statement
  ```

- **Root cause.** The same as finding 3.
- **Design change.** `sql_text` values belong to the admin. The host compares every `sql_text` value in a migration's result with the saved document, by settings path. A value must be the same text at the same path, absent, or empty at a path the old document lacked. Any other result blocks the upgrade, and the dialog names the statement (findings, item 3; design, "A statement belongs to the admin across upgrades"; wit, `migrate.config` in `addon.wit`). The model's last line runs that check.

### Item 4. Wipe authority cannot come from free text

#### Finding 4. The resolver's match names its fact in free text

- **Attack.** Check whether the host can build wipe authority from what a resolver returns.
- **Command.** The design session ran `bash wipe-authority.sh` in `opus` (findings listing, [4]). The script is lost, and it read the first draft's sketch, which is gone. Not rerun.
- **Result.** The design session's rerun found that the reference add-on fills `matched_on` with `format!("{} = {value}", rule.key_name())`, that the core sketch had no `WipeAuthority` type, and that `LookupMatch` was an empty placeholder struct (reruns, `### [4]`).
- **Root cause.** The first draft's `matched` record had `matched-on: string`. Free text cannot prove which identity fact a match rests on (findings, item 4).
- **Design change.** `matched-on` becomes `option<identity-kind>`, an enum. A core function builds `WipeAuthority` only from a declared input kind plus a fact that the server sent and that is not a placeholder (findings, item 4). The recovered sketch has `matched-on: option<identity-kind>` (wit, `resolver` in `addon.wit`) and `WipeAuthority::from_addon_match` (core).

#### Finding 13. The level 1 WIT gives a match no typed fact

- **Attack.** Parse the first draft's WIT and read the `matched` record.
- **Command.** The design session ran `witcheck $S/arena/synthesis/sketch/wit | tail -8` in `fable` (findings listing, [13]). `witcheck` is lost, and its input was the first draft's WIT, which is gone. Not rerun.
- **Result.** The design session's rerun printed `record matched` with `task-sequence: String`, `variables`, and `matched-on: String` (reruns, `### [13]`).
- **Root cause.** The same as finding 4.
- **Design change.** The same as finding 4.

### Item 5. A compiled cache is code on disk

#### Finding 5. A 4-byte edit to a serialized component changes what it runs

- **Attack.** The first draft's sketch kept a compiled-component cache on disk. Serialize a component whose function returns `0x1234567`, change the 4 bytes of that constant in the file, load the file, and call the function.
- **Command.** The design session ran a `grep` of the first draft's `design.md` and `sketch/host/*.rs`, then `opus-probe 16 | tail -1` (findings listing, [5]). The `grep` half is not rerun, because its input is gone. This page reruns the cache half as `cache-edit $out/cache`, rewritten (lever `wasmtime/src/bin/cache-edit.rs`). It edits the constant inside the file's `.text` section.
- **Result.** `Component::deserialize_file` loaded the edited file, and the function returned the new value (run, `### finding 5`). The design session got the same two values (reruns, `### [1]`, last line).

  ```text
  cache file: 13848 bytes; the constant occurs 1 time(s) in the file, 1 time(s) in .text; edited 4 bytes at offset 4101
  cache file: component returns 0x1234567; after editing 4 bytes of the cache file it returns 0xbadc0de
  ```

- **Root cause.** A serialized component is native code that `wasmtime` maps and runs. Its docs say the blob is "only lightly validated", that arbitrary input "could, for example, replace valid compiled code with any other valid compiled code", and that a file loaded with `deserialize_file` is not validated (crates `wasmtime-50.0.1/src/runtime/module.rs:385-396`, `:456-461`). `Component::deserialize_file` has the same unsafety (`wasmtime-50.0.1/src/runtime/component/component.rs:266-279`).
- **Design change.** The sketch and the plan drop the cache. The server compiles each component in memory when it loads it (findings, item 5). The recovered sketch says "No compiled code is ever written to disk" (wit, `Deps` in `sketch/host/lib.rs`), and the design says that no cache is planned (design, "Package, manifest, signing, and unsigned packages"). This settles a question the earlier prototype left open. It compiled each component on every start, did not measure loading a serialized one, and noted that a server could cache compiled code ([Wasmtime 50.0.1 SQL add-on reference](wasmtime-50.0.1.md#sizes-and-times), and its "What changes in deployarr").

#### Finding 15. The design rules out a cache that the sketch and plan keep

- **Attack.** Compare the first draft's `design.md` with its `plan.md` and its host sketch.
- **Command.** The design session ran a `grep` for `machine code`, `compiled-component cache`, and `stray cache file` over the first draft's `design.md`, `plan.md`, `sketch/host/lib.rs`, and `sketch/host/lifecycle.rs` (findings listing, [15]). Its input is gone. Not rerun.
- **Result.** `design.md:46` and `:164` said no compiled machine code is kept on disk. `plan.md:64` checked for "no stray cache file". `sketch/host/lib.rs:28` and `sketch/host/lifecycle.rs:5`, `:119`, and `:132` kept a compiled-component cache (reruns, `### [5/15/19]`).
- **Root cause.** The first draft contradicted itself (findings, item 5).
- **Design change.** The same as finding 5.

#### Finding 19. The same contradiction, found separately

- **Attack.** The same comparison as finding 15.
- **Command.** The design session ran `bash cache-contradiction.sh` in `sonnet/journal` (findings listing, [19]). The script is lost, and its input is gone. Not rerun. The design session reran findings 5, 15, and 19 as one `grep` (reruns, `### [5/15/19]`).
- **Result.** The same lines as finding 15.
- **Root cause.** The same as finding 15.
- **Design change.** The same as finding 5.

### Item 6. Stored steps lose bytes and fields

#### Finding 6. A stored add-on step does not round-trip

- **Attack.** Parse a stored add-on step with the first draft's shapes, which hold the step's config as a `serde_json::Value`, serialize it again, and compare the bytes.
- **Command.** The design session ran `opus-serde-rt` with two steps in `opus/serde-rt` (findings listing, [6]). Only its `Cargo.toml` survives (sources). This page reruns `serde-rt` with the same two steps, rewritten with the first draft's shapes as `haiku/serde-action` copied them (lever `probes/src/bin/serde-rt.rs`).
- **Result.** Keys came back in another order, and a 23-digit integer came back as a float (run, `### finding 6`). The design session's output has the same `saved` and `loaded` lines (reruns, `### [6]`).

  ```text
  saved : {"type":"AddOn","addon":"acme.entra-join","kind":"join","config_version":1,"config":{"tenant":"contoso","group_tag":"lab","max_wait":120}}
  loaded: {"type":"AddOn","addon":"acme.entra-join","kind":"join","config_version":1,"config":{"group_tag":"lab","max_wait":120,"tenant":"contoso"}}
  byte-identical: false
  Box<RawValue> in the #[serde(tag)] enum: does not parse: invalid type: newtype struct, expected any valid JSON value
  Box<RawValue> in a plain struct (the design): byte-identical: true
  saved : {"type":"AddOn","addon":"acme.x","kind":"k","config_version":1,"config":{"big":12345678901234567890123}}
  loaded: {"type":"AddOn","addon":"acme.x","kind":"k","config_version":1,"config":{"big":1.2345678901234568e+22}}
  byte-identical: false
  ```

- **Root cause.** Without its `preserve_order` feature, a `serde_json` map is a `BTreeMap`, which sorts keys (crates `serde_json-1.0.154/src/map.rs:3-4`). Without `arbitrary_precision`, a number is a `u64`, an `i64`, or an `f64` (`serde_json-1.0.154/src/number.rs:26-34`). The run also shows that a `Box<RawValue>` field does not parse inside a `#[serde(tag = "type")]` enum, while it does in a plain struct.
- **Design change.** The config of an add-on step is kept as the raw JSON text, a `Box<RawValue>`. Built-in actions and the add-on envelope refuse unknown fields (findings, item 6). The recovered sketch reads the `type` tag by hand and parses the raw text as exactly one shape, because a derived tagged enum cannot hold raw text (core, `Action` and `AddOnStep`). The run's last line for each step uses that shape.

#### Finding 23. Unknown fields on stored steps vanish

- **Attack.** Parse stored steps with unknown fields and unknown tags through the first draft's shapes.
- **Command.** The design session ran `cargo run --release` in `haiku/serde-action` (findings listing, [23]). Its `main.rs` survives and is unchanged (sources). This page reruns it as `serde-action`.
- **Result.** A built-in `SetVariable` with an extra `sneaky` field parsed, and the field was gone when it was written back. An unknown tag, a step without `config_version`, and a `Restart` step, which the trimmed copy of the shapes lacks, each failed with `data did not match any variant of untagged enum Action`. A stored step of an unknown add-on kind round-tripped as an equal value (run, `### finding 23`). The design session's output has the same lines (reruns, `### [23]`).

  ```text
  7 builtin SetVariable with extra field: parsed as Builtin(SetVariable { name: "a", value: "b" })
      reserialized: {"type":"SetVariable","name":"a","value":"b"}
  ```

- **Root cause.** The first draft's derived enums did not refuse unknown fields, and its `#[serde(untagged)]` fallback hid the real error behind one generic message (inferred from the output).
- **Design change.** The same as finding 6. A step saved by a newer schema is an error, not a silent loss (findings, item 6).

### Item 7. The permit scopes blocked the sketched add-ons

#### Finding 7. Scopes deny calls that the sketched add-ons make

- **Attack.** Compare the first draft's scope sentence with the host calls that the sketched add-ons make.
- **Command.** The design session ran `bash permit-scopes.sh` in `opus` (findings listing, [7]). The script survives (sources), but it greps the first draft's `design.md`, `sketch/wit/addon.wit`, and `sketch/guest/autopilot.rs`, which are gone. Not rerun.
- **Result.** The first draft said `Pure` "allows no capability at all", while `steps.expand` was "a pure function of its arguments and the add-on's settings". `steps.call` was in no scope, and the Autopilot add-on called `store::put_json` and `jobs::enqueue` from it (reruns, `### [7]`).
- **Root cause.** The scopes were a sentence, not a table that covers every export (findings, item 7).
- **Design change.** One table maps every export to its scope and to the host interfaces that scope allows. `Pure` may read the add-on's own non-secret settings. `steps.call` is `Granted` (findings, item 7). The recovered `addon.wit` opens with that table, and `Scope::of` is one exhaustive match (wit).

### Item 8. Add-on calls can starve the server's runtime

The earlier prototype did not test epoch interruption ([Wasmtime 50.0.1 SQL add-on reference](wasmtime-50.0.1.md#not-tested)).

#### Finding 8. Looping guests stall every `tokio` worker

- **Attack.** A `tokio` runtime with 4 worker threads runs a heartbeat task that sleeps 50 ms in a loop and records the longest gap between wake-ups. 1 or 8 concurrent calls run a guest that loops forever, with a 3 s `tokio` timeout and an epoch deadline of 30 ticks of 100 ms.
- **Command.** The design session ran `starve $G 1` and `starve $G 8` in `fable` (findings listing, [8]). Lines 60 to 100 of `starve.rs` survive (sources). This page reruns `starve` with 1 and 8 calls, rewritten (lever `wasmtime/src/bin/starve.rs`). It adds four runs with 8 calls that test the design's change:
  - `yield`: an epoch callback returns `UpdateDeadline::Yield(1)` on each tick and `UpdateDeadline::Interrupt` at the deadline.
  - `yield-now`: the callback yields with `tokio::task::yield_now` through `UpdateDeadline::YieldCustom`.
  - `separate`: as `yield-now`, but the calls run on a second runtime with its own 4 worker threads.
  - `separate-yield`: as `yield`, but the calls run on the second runtime.
- **Result.** From run, `### finding 8`:

  | Calls | Where the calls run, and how they yield | How each call ended | Longest heartbeat gap |
  | --- | --- | --- | --- |
  | 1 | server's runtime, no yield | epoch trap after 3.0 s | 51 ms |
  | 8 | server's runtime, no yield | epoch trap after 2.9 to 3.0 s | 5964 ms |
  | 8 | server's runtime, `UpdateDeadline::Yield` | 5 by timeout after 4.8 to 4.9 s, 3 by epoch trap after 2.9 to 3.0 s | 4757 ms |
  | 8 | server's runtime, `tokio::task::yield_now` | timeout after 3.0 to 3.2 s | 452 ms |
  | 8 | a second runtime, `tokio::task::yield_now` | timeout after 3.1 to 3.4 s | 54 ms |
  | 8 | a second runtime, `UpdateDeadline::Yield` | epoch trap after 2.9 to 5.9 s, none by the 3 s timeout | 54 ms |

  The design session measured 57 ms for 1 call and 6042 ms for 8 calls (reruns, `### [8]`).

  The last row shows what the separate runtime does not fix. It keeps the server's heartbeat on time with either yield. With the plain `Yield`, the add-on runtime's own timers fire late. No call ended at its 3 s timeout. Five ran for 4.4 to 5.9 s, until the epoch deadline trapped them. The epoch deadline counts only ticks in which the guest runs, so a call that waits in the queue reaches it later (inferred from the callback, which counts its own calls).
- **Root cause.** A guest that never yields holds its worker thread until the epoch trap. 8 calls on 4 workers run in two waves of 3 s, and nothing else on the runtime runs meanwhile (inferred, because the longest gap is about two deadlines long). The sources suggest why the plain `Yield` lets timers fire late and `tokio::task::yield_now` does not. No run traced it, so this is inferred:
  - `UpdateDeadline::Yield` uses `wasmtime`'s own yield (crates `wasmtime-50.0.1/src/runtime/vm/libcalls.rs:771-780`). It wakes the task while the task is still running (`wasmtime-50.0.1/src/runtime/store.rs:2306-2327`).
  - `tokio` marks a running task that wakes itself as notified (`tokio-1.54.1/src/runtime/task/state.rs:253`, `:264-269`). When the poll returns, `tokio` schedules the task again as a yield (`tokio-1.54.1/src/runtime/task/harness.rs:153-161`, `src/runtime/scheduler/multi_thread/handle.rs:122-123`). A yield goes to the back of the worker's local queue (`src/runtime/scheduler/multi_thread/worker.rs:1393-1396`).
  - The other looping calls keep that queue full. A worker polls its timer driver when its queues run empty (`worker.rs:607-620`), or at maintenance, once every 61 turns of its run loop (`worker.rs:587-591`, `:810-818`, `src/runtime/builder.rs:293-296`). Each turn can run a guest for up to 100 ms. So the timers of every task on that runtime fire late. On the shared runtime that is the heartbeat and the calls' 3 s timeouts. On a separate runtime it is the calls' timeouts.
  - `tokio::task::yield_now` does not wake the task at once. It hands the waker to the scheduler (`tokio-1.54.1/src/task/yield_now.rs:49-55`). The worker wakes the deferred tasks only after it has polled its driver, when its queues run empty or at maintenance (`worker.rs:616-620`, `:816-818`, `:870-906`). `wasmtime`'s docs recommend `tokio::task::yield_now` for `YieldCustom` (`wasmtime-50.0.1/src/runtime/store.rs:402-416`).
- **Design change.** Add-on calls run on their own `tokio` runtime with its own threads, apart from PXE, TFTP, and the agent API. Calls use `wasmtime`'s async support with epoch yields. Every host function is async, and blocking driver work runs under `spawn_blocking` with its own timeout. The plan's check measures the main runtime's heartbeat under looping and blocking add-on calls (findings, item 8). This page's runs add one detail. The epoch callback has to yield through `UpdateDeadline::YieldCustom` with `tokio::task::yield_now`. On the shared runtime, the plain `Yield` stalled the heartbeat almost as long as no yield. On a separate runtime, no call ended at its 3 s timeout, and five ran 1.4 to 2.9 s past it. The plan's check should therefore also measure when each add-on call ends, not only the main runtime's heartbeat.

#### Finding 22. A `tokio` timeout does not bound a blocking host function

- **Attack.** On a 2-worker runtime, a guest calls a host function that blocks its thread for 3000 ms, inside a 100 ms `tokio` timeout. Then two such calls run at once beside an unrelated task that ticks every 10 ms.
- **Command.** The design session ran `./probe-multi` in `haiku/probe-timeout` (findings listing, [22]). Lines 1 to 120 of its `main.rs` survive, with cases A to D and the start of case E (sources). This page reruns `blocking-host-fn`, which follows cases A to D and completes case E (lever `wasmtime/src/bin/blocking-host-fn.rs`). `wasmtime` 50 made `Config::async_support` a deprecated no-op (crates `wasmtime-50.0.1/src/config.rs:426-431`), so the rewrite does not call it.
- **Result.** From run, `### finding 22`. The design session got the same results, with 100 ms in case B and 46 ms in case C (reruns, `### [22]`).

  ```text
  A blocking_host_fn deadline_ms=100 block_ms=3000 elapsed_ms=3000 result=Ok(42)
  B async_host_fn deadline_ms=100 block_ms=3000 elapsed_ms=102 result=Elapsed (timeout fired)
  C pure_loop epoch_ticks=5 elapsed_ms=47 result=Err(wasm trap: interrupt)
  D memory_grow_128MiB_under_64MiB_cap result=Ok(-1)
  D memory_grow_32MiB_under_cap result=Ok(1)
  E two_blocking_calls_2_workers unrelated_task_max_gap_ms=3000 results=[Ok(42), Ok(42)]
  ```

- **Root cause.** A `tokio` timeout fires only when its task is polled. A synchronous host function holds the thread, so the task is not polled until the function returns (inferred from cases A and B).
- **Design change.** The same as finding 8.

### Item 9. An add-on can read its own secret file

#### Finding 11. The step an add-on expands sends its secret back

- **Attack.** An add-on's `steps.expand` returns a `RunCommand` that attaches its own `secret_file` field, base64-encodes it, and sets a variable. A `ServerCall` then sends that variable to the add-on's `steps.call`.
- **Command.** The design session ran `python3 model/secret_step_file.py` in `fable` (findings listing, [11]). Its first 80 lines survive (sources). This page reruns `python3 -I model/secret_step_file.py`, with `AddOn.call` and the run code rewritten. Each host and agent function quotes the first draft's sentence that it follows. The agent runs a real shell command in a real step folder.
- **Result.** The add-on received the secret's bytes. Its debug log carried them in base64, and a `grep` for the secret's text did not find them (run, `### finding 11`). The design session's output has the same lines (reruns, `### [11]`).

  ```text
  settings.get as the add-on sees it: {'enrollment_package': {'set': True}}
  lowered program accepted by the host: ['RunCommand', 'ServerCall']
  bytes the add-on's `steps.call` received: b'BULK-ENROLLMENT-TOKEN-7f3a91c2'
  equal to the stored secret: True
  debug log: ['[acme.join] debug: QlVMSy1FTlJPTExNRU5ULVRPS0VOLTdmM2E5MWMy', '[acme.join] debug: [redacted]']
  plan check, grep of the debug log for the secret finds it: False
  ```

- **Root cause.** The first draft let an add-on's own command read a secret file, and a `ServerCall` returns whatever that command sets (findings, item 9).
- **Design change.** A secret reaches the PC only as input to a built-in action that the base defines, such as one that adds a provisioning package with DISM. An add-on's own `RunCommand` or `RunPowerShell` gets no secret file and no secret variable (findings, item 9). The recovered sketch has `AddProvisioningPackage` as the only action that takes a secret, and `StepFile` holds content only (core, `BuiltinAction` and `StepFile`).

### Item 10. The manifest parser accepts unknown keys

#### Finding 12. `FieldKind` accepts unknown keys on kinds without fields

- **Attack.** Parse form fields with unknown keys through the first draft's `FieldKind`, a `#[serde(tag = "kind", deny_unknown_fields)]` enum, both nested as sketched and flattened as the manifests use it.
- **Command.** The design session ran `fable-serdecheck` in `fable/serdecheck` (findings listing, [12]). Its `main.rs` and `Cargo.toml` survive, and `main.rs` is unchanged (sources). This page reruns it as `serdecheck`.
- **Result.** Unknown keys were accepted on `secret`, `sql_text`, and `bool` when flattened, and on `secret` and `sql_text` when nested. They were refused on `text`, which has fields of its own (run, `### finding 12`). The design session's output has the same lines (reruns, `### [12]`). The finding's title names five kinds (findings listing, [12]), and the surviving probe tries four.

  ```text
  4. unknown key on a sql_text field
      input: key = "statement"  label = "Statement"  kind = "sql_text"  allow_write = true
      Field as sketched (nested kind): refused: invalid type: string "sql_text", expected internally tagged enum FieldKind
      Field with flattened kind:       ACCEPTED as SqlText
  7. sketched Field from JSON {"key":"q","label":"Statement","help":null,"required":false,"kind":{"kind":"sql_text","allow_write":true}}
      ACCEPTED as SqlText
  ```

- **Root cause.** `deny_unknown_fields` on the tagged enum refused unknown keys only on the variants that have fields (inferred from the output).
- **Design change.** Every table and every field kind in the manifest refuses an unknown key, with a test per kind (findings, item 10). A field parses through one flat struct with `deny_unknown_fields`, and a conversion gives each kind only its own keys (core, `Field` and `FieldWire`).

### Item 11. A pure call sees the clock and fresh randomness

#### Finding 14. An empty WASI context still hands out time and randomness

- **Attack.** Run a guest twice with an empty `WasiCtx`. The guest prints the wall clock, a 50 ms sleep measured on the monotonic clock, and a hash with a random seed. It also tries to open a file, list a folder, and connect a socket.
- **Command.** The design session ran `wt ambient fable-guest-ambient.wasm` in `fable` (findings listing, [14]). The guest and the host are lost (sources). This page reruns `wasi ambient` with the guest in `guest/`, rewritten. The context has no preopened folder, no environment, and no network. Only the guest's arguments are set, and stdout passes through so the guest can print. The run adds `wasi pure`, which is the design's change. It gives the context a fixed wall clock, a fixed monotonic clock, and seeded random generators.
- **Result.** With the empty context, both calls saw the real wall clock and the monotonic clock, and each got a different hash. With the fixed clocks and seeded generators, both calls printed the same lines. Files and sockets failed in both modes (run, `### finding 14`). The design session's output shows the same pattern (reruns, `### [14]`).

  ```text
  guest: wall clock (unix seconds) = Ok(1794548464)
  guest: monotonic clock measured a 50 ms sleep as 51 ms
  guest: random-seeded hash = cc6e46c62d12288a
  guest: File::open("/etc/passwd") = Err(Os { code: 44, kind: NotFound, message: "No such file or directory" })
  guest: read_dir(".") = Err(Os { code: 44, kind: NotFound, message: "No such file or directory" })
  guest: TcpStream::connect("127.0.0.1:5432") = Err(Os { code: 2, kind: PermissionDenied, message: "Permission denied" })
  guest: environment variables = 0
  ```

- **Root cause.** By default `WasiCtxBuilder` uses the host's wall clock and monotonic clock, and a random generator with a fresh seed per context (crates `wasmtime-wasi-50.0.1/src/ctx.rs:319-332`, `:366-380`).
- **Design change.** A `Pure` call gets a fixed clock and seeded randomness, or the design stops calling it pure (findings, item 11). The design took the first option (wit, the scope table in `addon.wit`). The finding marked the `wasmtime-wasi` part inferred. This page's `wasi pure` run shows that `wall_clock`, `monotonic_clock`, `secure_random`, `insecure_random`, and `insecure_random_seed` make two calls print the same output.

### Item 12. Compiling a hostile component exhausts the server

#### Finding 16. A small component takes seconds and gigabytes to compile

- **Attack.** Build a component with one function that has n blocks and k locals, and compile it with `Component::new` and the default `Config`. Repeat under an address-space limit.
- **Command.** The design session ran `compile 6400 6400`, `compile 9600 9600`, and `compile 20000 20000`, then `compile 40000 40000` under `ulimit -v 6000000`, in `sonnet/wt` (findings listing, [16]). Only the 40000 case ran under that limit. Its rerun ran 6400, 20000, and 40000, and skipped 9600 (reruns, `### [16]`). The program is lost (sources). This page reruns `compile`, rewritten with its own generator (lever `wasmtime/src/bin/compile.rs`). Each local gets a distinct value, each block may branch out early and otherwise adds 1 to one local, and the function returns the sum of every local. Its sizes are smaller than the design session's, so that it fits beside the other agents on this machine.
- **Result.** From run, `### finding 16`:

  | Blocks and locals | Binary | Compile time | Peak resident memory |
  | --- | --- | --- | --- |
  | 4000 | 115,480 bytes | 4.4 s | 615 MiB |
  | 8000 | 231,480 bytes | 20.8 s | 2285 MiB |
  | 12000, under `ulimit -v 3000000` | not printed | `memory allocation of 192008 bytes failed`, aborted with exit 134 | not printed |

  Doubling the blocks and locals multiplied the compile time by 4.7 and the peak memory by 3.7. The design session's rerun measured these (reruns, `### [16]`):

  | Blocks and locals | Binary | Compile time | Peak resident memory |
  | --- | --- | --- | --- |
  | 6400 | 146,681 bytes | 2.7 s | 450 MiB |
  | 20000 | 485,757 bytes | 28.7 s | 4153 MiB |
  | 40000, under `ulimit -v 6000000` | not printed | aborted with exit 134 | not printed |

  For scale, the earlier prototype's SQL add-on components compiled in 0.48 to 1.04 seconds in its second run, and in up to 1.22 seconds in its first ([Wasmtime 50.0.1 SQL add-on reference](wasmtime-50.0.1.md#sizes-and-times)).
- **Root cause.** The compile cost of one function grows faster than its size. Which `wasmtime` or Cranelift pass takes the time and memory was not profiled (inferred from the measured growth).
- **Design change.** The host compiles only after the trust checks pass. It validates with `wasmparser` first and refuses a component over caps on its size, on one function body's size, and on one function's locals. One thread compiles one component at a time. The spike measures the caps (findings, item 12; design, "Package, manifest, signing, and unsigned packages").

### Item 13. A guest path can redirect credentials

#### Finding 17. Both natural ways to join a path change the host

- **Attack.** An HTTP connection's base URL is `https://graph.microsoft.com`, and the host adds the connection's credentials to each request. The guest supplies a path. The host builds the URL with `Url::join` or with `format!("{base}{path}")` and `Url::parse`.
- **Command.** The design session ran `bash run.sh` in `sonnet/urljoin` (findings listing, [17]). It is lost (sources). This page reruns `urljoin`, rewritten (lever `probes/src/bin/urljoin.rs`). Each case also builds the URL the design's way, with the guest's text as one percent-encoded path segment.
- **Result.** From run, `### finding 17`:

  | Guest path | `Url::join` sends to | `format!` sends to | One encoded segment sends to |
  | --- | --- | --- | --- |
  | `https://attacker.example/x` | `attacker.example` | `graph.microsoft.comhttps` | `graph.microsoft.com` |
  | `//attacker.example/x` | `attacker.example` | `graph.microsoft.com` | `graph.microsoft.com` |
  | `@attacker.example/x` | `graph.microsoft.com` | `attacker.example` | `graph.microsoft.com` |
  | `/../../x` | `graph.microsoft.com` | `graph.microsoft.com` | `graph.microsoft.com` |
  | `\\attacker.example\x` | `attacker.example` | `graph.microsoft.com` | `graph.microsoft.com` |
  | `..` | `graph.microsoft.com` | `graph.microsoft.com..` | refused |

  The design session's output has the same hosts for `@attacker.example/x`, `/../../x`, and `\\attacker.example\x` (reruns, `### [17]`).
- **Root cause.** `Url::join` resolves its input as a URL reference, so an absolute or scheme-relative input replaces the host (crates `url-2.5.8/src/lib.rs:443-456`), and a relative input that starts with `\` counts as `/` (`url-2.5.8/src/parser.rs:763-764`). With `format!`, the `@` makes the base host the user name of a URL whose host is the attacker's (inferred from the output).
- **Design change.** The guest gives a list of path segments and query pairs. The host percent-encodes each segment, appends it to the base path, never parses guest text as a URL, and checks that the final host is the connection's host. The plan tests these payloads (findings, item 13). The recovered `host.wit` has `path: list<string>` and also refuses an empty segment, `.`, and `..` (wit, `deployarr:http`).

### Item 14. A waiting upgrade blocks every other operation

#### Finding 18. Force, Roll back, and Uninstall all conflict with a waiting upgrade

- **Attack.** With two deployments running, an admin starts an upgrade with `WhenBusy::Wait`, which waits for the deployments. Then the admin tries `WhenBusy::Force`, Roll back, and Uninstall.
- **Command.** The design session ran `python3 -I waiting-op.py` in `sonnet/journal` (findings listing, [18]). It is lost (sources). This page reruns `python3 -I model/waiting_op.py`, rewritten. It models the first draft's journal in SQLite, with a partial unique index that allows one open operation per add-on. Step 5 of the original listed the first draft's `AddOnHost` methods. Its input is gone, so that step is not rerun.
- **Result.** Each later operation failed on the unique index. The design's change let Force take the waiting row over, and let a `cancel` free the add-on for a roll back (run, `### finding 18`). Steps 1 to 4 match the design session's output (reruns, `### [18]`). Its step 5 listed 19 methods and found no method that cancels or abandons an open operation.

  ```text
  1. upgrade to 1.1.0 with WhenBusy::Wait: row written in phase Approved (2 deployments are running)
  2. admin retries the upgrade with WhenBusy::Force -> Conflict: UNIQUE constraint failed: addon_ops.addon_id
  3. admin clicks Roll back -> Conflict: UNIQUE constraint failed: addon_ops.addon_id
  4. admin clicks Uninstall -> Conflict: UNIQUE constraint failed: addon_ops.addon_id
  ```

- **Root cause.** A waiting operation holds the one open row, and only a failure before `Committing` set `Abandoned` (reruns, `### [18]`).
- **Design change.** A `cancel` method abandons a waiting operation, and Force on the same change takes the waiting row over (findings, item 14). The recovered sketch has `AddOnHost::cancel` and the Force rule in `apply` (wit, `sketch/host/lib.rs`).

### Item 15. `stage` cannot compare exports with the manifest

#### Finding 20. Every component has the same exports

- **Attack.** List what a compiled component reveals to `stage` about the kinds it implements.
- **Command.** The design session ran `exports $S/arena/synthesis/sketch/wit` in `sonnet/wt` (findings listing, [20]). The program is lost, and its input was the first draft's WIT, which is gone. Not rerun.
- **Result.** The world `addon-level-1` exported 7 interfaces, `resolver`, `steps`, `events`, `jobs-handler`, `actions`, `importer`, and `migrate`, and the functions named the resolver, step kind, job, action, importer, or migration scope they were called for with a `string` (reruns, `### [20]`).
- **Root cause.** Every component exports the same interfaces, and kinds are runtime strings (findings, item 15).
- **Design change.** A `Pure` `describe` export returns the kinds that the component implements, and `stage` compares them with the manifest. A call for an unknown kind is a typed error (findings, item 15). The recovered `addon.wit` has `interface describe` and the `unknown-kind` error (wit).

## Supported claims

The adversarial runs also produced evidence for claims that the design keeps. The rows below come from two places:

- The first eight rows are the claims that the last paragraph of the findings lists. The design cites seven of them as "(run, supported claims)". It cites the import row to `spin-4.3.0.md` instead (design, "API versioning, compatibility, and the support window").
- The last two rows come from the notes in the findings listing on probes that found nothing (findings listing, `opus` and `fable` notes). The design does not cite the `memory.grow` row. It cites the PostgreSQL row as "(run, item 2)".

This page reruns each one.

| Claim | Design session | This page's run |
| --- | --- | --- |
| `wasmtime` finds an export across minor versions in both directions and refuses another major | `opus-probe 16` printed `found=true` for 1.0.0 to 1.1.0 and 1.2.0 to 1.0.0, and `found=false` for 1.0.0 to 2.0.0 (reruns, `### [1]`) | the same three results, and the function found through each match ran (run, `### supported claims: versioned exports and imports`) |
| An import links to a host that serves another minor version with more functions | `wt semver-imports` (findings listing, `fable` notes) | an import of 1.0.0 linked to a host serving 1.3.0, and an import of 1.3.0 linked to a host serving 1.0.0. An import of 2.0.0 was refused with `a matching implementation was not found in the linker` (run, the same section) |
| An epoch deadline traps a looping guest | `probe-multi` case C trapped with `interrupt` after 46 ms (reruns, `### [22]`) | case C trapped after 47 ms with 5 ticks of 10 ms (run, `### finding 22`). Every call in `starve` without yields ended in an epoch trap after 2.9 to 3.0 s (run, `### finding 8`) |
| A new store, a new `WasiCtx`, and an instantiation per call are cheap | median 25 us, p99 138 us, max 627 us over 2000 calls (findings listing, `fable` notes) | median 22 us, p99 67 us, max 2995 us over 2000 calls, with resident memory from 26 MiB to 27 MiB (run, `### supported claims: cost of a new instance, and memory after traps`). The earlier prototype's instantiations of its SQL add-ons took under 1 ms ([Wasmtime 50.0.1 SQL add-on reference](wasmtime-50.0.1.md#sizes-and-times)) |
| Memory stays flat over calls that trap | 3000 calls that each touch 8 MiB and abort left resident memory flat at 72 MiB (findings listing, `fable` notes) | resident memory went from 26 MiB to 35 MiB in the first 1000 calls, then stayed at 35 MiB through 3000. All 3000 calls trapped (run, the same section) |
| An empty WASI context refuses files and sockets | `wt ambient` (findings listing, `fable` notes) | `File::open` and `read_dir` failed with `NotFound`, and `TcpStream::connect` failed with `PermissionDenied` (run, `### finding 14`) |
| `minisign-verify` 0.3.0 verifies a prehashed signature with `allow_legacy = false` | `fable-mini` with signatures from `minisign` 0.10.0 (findings listing, `fable` notes) | a signature from the `minisign` crate 0.10.0 starts with `RUQ`, the base64 of `ED`, the prehashed algorithm. `verify` with `allow_legacy = false` and `verify_stream` returned `Ok(())`. One changed payload bit and an edited trusted comment each gave `InvalidSignature`, and another key gave `UnexpectedKeyId` (run, `### supported claims: minisign-verify 0.3.0`) |
| A SQLite partial unique index allows one open operation per add-on | refused a second open row (findings listing, `fable` notes) | refused a second open row with `UNIQUE constraint failed: addon_ops.addon_id`, allowed an open row for another add-on, and allowed a new row after the first committed (run, `### finding 18`) |
| One `memory.grow` of 1 GiB under a 64 MiB limit is refused | returned -1 (findings listing, `opus` notes) | returned -1, and case D of `blocking-host-fn` refused 128 MiB and allowed 32 MiB (run, `### findings 1 and 9` and `### finding 22`) |
| PostgreSQL's `BEGIN READ ONLY` refuses writes from an account that may write | `pg-ro.sh` (findings listing, `fable` notes) | refused `INSERT`, a data-modifying CTE, `nextval`, `DROP TABLE`, `SELECT ... FOR UPDATE`, `set_config('transaction_read_only','off')`, a `DELETE` through `query_to_xml`, and `CREATE TEMP TABLE ... AS`, and both rows were left. An advisory lock and a `search_path` change stayed in the session after `COMMIT` (run, `### supported claims: PostgreSQL read-only transaction`) |

The call cost and the memory after traps differ from the design session's numbers. Each is a single run, so the numbers bound the cost only roughly.

## What this page's runs add

- An untouched table costs no resident memory under `wasmtime`'s default lazy table setting. A guest that fills its tables makes them resident, so finding 9 still holds ([item 1](#item-1-memory-limits-do-not-bound-an-add-on)).
- `UpdateDeadline::Yield` does not protect a shared `tokio` runtime. `tokio::task::yield_now` cuts the stall from seconds to hundreds of milliseconds, and a separate runtime cuts it to the heartbeat's own period. On a separate runtime, the plain `Yield` still lets the add-on calls' own timeouts fire late, so the calls need `tokio::task::yield_now` there too ([item 8](#item-8-add-on-calls-can-starve-the-servers-runtime)).
- `SELECT ... INTO OUTFILE` wrote a file under `SET SESSION TRANSACTION READ ONLY` as well as inside a read-only transaction. The design had marked this case as untested. triage77 had already found the same on MariaDB and MySQL, and this run confirms it on MariaDB ([item 2](#item-2-read-only-enforcement-depends-on-the-engine)).
- A `Box<RawValue>` inside a `#[serde(tag = "type")]` enum does not parse. The recovered sketch already says that a derived tagged enum cannot hold raw text (core, `Action`), and this run confirms it ([item 6](#item-6-stored-steps-lose-bytes-and-fields)).
- A fixed clock and seeded generators make two calls of the same guest print the same output. The finding had marked this as inferred ([item 11](#item-11-a-pure-call-sees-the-clock-and-fresh-randomness)).
- The design's path check, the segment encoding, `cancel`, and the Force takeover each stopped their attack in the rewritten models and probes ([items 3](#item-3-an-upgrade-can-replace-the-admins-sql), [13](#item-13-a-guest-path-can-redirect-credentials), and [14](#item-14-a-waiting-upgrade-blocks-every-other-operation)). These are models of the host, not the host itself.

## Not rerun

- Findings 4, 7, 13, 15, 19, and 20, and the `grep` half of finding 5. Each read the first draft of the design, its plan, or its sketch, which the design session edited after the findings. The output on this page is the design session's.
- The `design.md` and `host.wit` part of finding 21, for the same reason.
- Step 5 of finding 18, for the same reason.
- SQL Server, for findings 2 and 21. A cloud session cannot run it.
- MySQL Server, as opposed to MariaDB. triage77 ran MySQL 8.0.46 and got the same results as MariaDB.

## What changes in deployarr

Each change comes from the item named after it. #83 applies them to the architecture.

1. A host `ResourceLimiter` sums the bytes of every memory and every table in a call's store against one budget, and caps the number of instances, memories, tables, and table elements. A table counts at its declared size, because a guest can fill it later. The hostile tests include the multi-instance case and the filled-table case ([item 1](#item-1-memory-limits-do-not-bound-an-add-on)).
2. The design does not call "never writes" a property of the interface. The read-only account is the guard on every engine. PostgreSQL runs each statement in `BEGIN READ ONLY`. MySQL and MariaDB use `SET SESSION TRANSACTION READ ONLY`, which #77 refines. SQL Server rolls each statement back, which stays untested until M5. On MySQL and MariaDB, the read-only account must not hold the `FILE` privilege, because `INTO OUTFILE` passes both guards. #77 owns that rule and the MySQL guard, so #83 builds on its text ([item 2](#item-2-read-only-enforcement-depends-on-the-engine)).
3. `sql_text` values belong to the admin. A settings migration that changes, adds, or moves one blocks the upgrade, and the dialog shows the statements ([item 3](#item-3-an-upgrade-can-replace-the-admins-sql)).
4. `matched-on` is `option<identity-kind>`, and `WipeAuthority` comes only from a declared input kind and a fact the server sent ([item 4](#item-4-wipe-authority-cannot-come-from-free-text)).
5. The server keeps no compiled component on disk ([item 5](#item-5-a-compiled-cache-is-code-on-disk)).
6. An add-on step's config is stored as its raw JSON text, and built-in actions and the add-on envelope refuse unknown fields ([item 6](#item-6-stored-steps-lose-bytes-and-fields)).
7. One table maps every export to its scope and the host interfaces it allows. `Pure` reads the add-on's own non-secret settings. `steps.call` is `Granted` ([item 7](#item-7-the-permit-scopes-blocked-the-sketched-add-ons)).
8. Add-on calls run on their own `tokio` runtime. Every host function is async, and blocking driver work runs under `spawn_blocking` with its own timeout. The epoch callback yields through `UpdateDeadline::YieldCustom` with `tokio::task::yield_now`, and traps at the deadline. The plan's check measures when each add-on call ends as well as the main runtime's heartbeat ([item 8](#item-8-add-on-calls-can-starve-the-servers-runtime)).
9. A secret reaches the PC only as input to a built-in action, such as `AddProvisioningPackage`. No add-on command gets a secret file or a secret variable ([item 9](#item-9-an-add-on-can-read-its-own-secret-file)).
10. The manifest refuses unknown keys in every table and on every field kind, with a test per kind ([item 10](#item-10-the-manifest-parser-accepts-unknown-keys)).
11. A `Pure` call gets a fixed wall clock, a fixed monotonic clock, and seeded random generators ([item 11](#item-11-a-pure-call-sees-the-clock-and-fresh-randomness)).
12. The host compiles a component only after the trust checks, validates it with `wasmparser` against caps on size, function body size, and locals, and compiles one component at a time. The spike sets the caps ([item 12](#item-12-compiling-a-hostile-component-exhausts-the-server)).
13. HTTP requests take path segments and query pairs. The host encodes each one, never parses guest text as a URL, and checks the final host ([item 13](#item-13-a-guest-path-can-redirect-credentials)).
14. `cancel` abandons a waiting operation, and Force on the same change takes the waiting row over ([item 14](#item-14-a-waiting-upgrade-blocks-every-other-operation)).
15. A `Pure` `describe` export returns the kinds a component implements, and `stage` compares them with the manifest ([item 15](#item-15-stage-cannot-compare-exports-with-the-manifest)).
