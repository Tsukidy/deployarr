# Rerun the attacks on the add-on design

These probes rerun the 23 findings that four adversarial runs raised against the add-on design. [Add-on design attacks on wasmtime 50.0.1](../../docs/research/addon-design-attacks-wasmtime-50.0.1.md) records each finding, its result, and the design change it caused. The probes are throwaway, and deployarr shares no code with them.

## What it contains

| Path | Holds |
| --- | --- |
| `wasmtime/` | Host programs on `wasmtime` 50.0.1 and `wasmtime-wasi` 50.0.1, one per attack |
| `guest/` | A `wasm32-wasip2` component that the `wasi` and `starve` programs run |
| `probes/` | Programs that need no `wasmtime`: the `serde` checks, the URL check, and the `minisign` check |
| `model/` | Python models of the first draft's host rules, which run real code against them |
| `sql/` | Shell scripts that run statements against MariaDB and PostgreSQL |
| `setup-servers.sh` | Installs and starts PostgreSQL and MariaDB |
| `reproduce.sh` | Builds every probe and runs each finding in order |

Each Cargo package has an empty `[workspace]` table, so Cargo does not treat it as a member of the deployarr workspace. Each has its own `Cargo.lock`, and `reproduce.sh` builds with `--locked`.

These are the programs and the findings they rerun:

| Program | Findings |
| --- | --- |
| `wasmtime`: `memory-limits` | 1, 9 |
| `wasmtime`: `cache-edit` | 5 |
| `wasmtime`: `starve` | 8 |
| `wasmtime`: `wasi` | 14, and three supported claims |
| `wasmtime`: `compile` | 16 |
| `wasmtime`: `blocking-host-fn` | 22 |
| `wasmtime`: `semver` | supported claims |
| `probes`: `serde-rt` | 6 |
| `probes`: `serdecheck` | 12 |
| `probes`: `urljoin` | 17 |
| `probes`: `serde-action` | 23 |
| `probes`: `minisign-check` | supported claims |
| `model/named_query_migrate.py` | 3, 10 |
| `model/secret_step_file.py` | 11 |
| `model/waiting_op.py` | 18, and a supported claim |
| `sql/mysql-ro*.sh` | 2 |
| `sql/pg-ro.sh` | supported claims |

Finding 21 reruns as a `grep` of `docs/architecture.md`. Findings 4, 7, 13, 15, 19, and 20 read the first draft of the design, which is gone, so nothing here reruns them.

## Requirements

- Linux with `rustup` and Cargo. The script adds the `wasm32-wasip2` target. The repository's `rust-toolchain.toml` picks the Rust version.
- Python 3 with no extra packages. It was run with Python 3.13.
- Root and `apt-get` for `setup-servers.sh`. It was run on Ubuntu 24.04.
- About 4 GB of free memory. The largest probes hold 1.8 GiB, 2.3 GiB, and up to 2.9 GiB for a few seconds.
- About 1.5 GB of disk in the output folder. A full run from a clean copy took 7 minutes 30 seconds on a 4-CPU cloud VM, including 5 minutes 51 seconds to build the `wasmtime` probes with 2 jobs.

## Run the probes

Start the database servers, then pass a folder outside the repository for the build output:

```bash
sudo bash tools/addon-design-attacks/setup-servers.sh
```

```bash
bash tools/addon-design-attacks/reproduce.sh "$SCRATCH/addon-attacks" | tee "$SCRATCH/addon-attacks.out"
```

The script prints a section for each finding. Each section shows the command and its output. Without the servers, the script skips the SQL sections and says so. Each build uses 2 jobs. Set `JOBS` to change it.

The MariaDB scripts connect as `root` through the local socket and use a database named `opus_ro`. `pg-ro.sh` makes the role `fable_rw` and the database `fable_ro`. Both scripts drop and remake their own database on each run.
