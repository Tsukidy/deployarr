#!/usr/bin/env bash
# Builds every probe in this folder and reruns each finding that
# docs/research/addon-design-attacks-wasmtime-50.0.1.md marks as rerun.
#
#   bash tools/addon-design-attacks/reproduce.sh <output-dir>
#
# The SQL probes need PostgreSQL and MariaDB on this machine. setup-servers.sh starts them.
set -uo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo=$(cd "$here/../.." && pwd)
out=${1:?usage: reproduce.sh <output-dir>}
mkdir -p "$out"
out=$(cd "$out" && pwd)
jobs=${JOBS:-2}
cd "$here" || exit 1

section() {
	printf '\n### %s\n' "$1"
}

# Prints the command with the output folder as $out and paths relative to this folder,
# then runs it.
run() {
	local shown="$*"
	shown=${shown//$out/\$out}
	shown=${shown//$here\//}
	shown=${shown//$repo\//}
	printf '$ %s\n' "$shown"
	"$@" 2>&1
}

section "environment"
date -u '+%Y-%m-%d %H:%M:%S UTC'
uname -sr
echo "CPUs: $(nproc), memory: $(free -m | awk '/^Mem:/ { print $2 }') MiB"
rustc --version
python3 --version

section "build"
rustup target add wasm32-wasip2 >"$out/rustup.log" 2>&1 || { cat "$out/rustup.log"; exit 1; }
build() {
	local name=$1
	shift
	if ! cargo build -j "$jobs" --release --locked "$@" >"$out/build-$name.log" 2>&1; then
		echo "build of $name failed, see $out/build-$name.log"
		tail -20 "$out/build-$name.log"
		exit 1
	fi
	echo "built $name: $(grep -m1 Finished "$out/build-$name.log" | sed 's/^ *//')"
}
build probes --manifest-path probes/Cargo.toml --target-dir "$out/target-probes"
build guest --manifest-path guest/Cargo.toml --target-dir "$out/target-guest" --target wasm32-wasip2
build wasmtime --manifest-path wasmtime/Cargo.toml --target-dir "$out/target-wasmtime"
P=$out/target-probes/release
W=$out/target-wasmtime/release
G=$out/target-guest/wasm32-wasip2/release/attack-guest.wasm
echo "guest component: $(stat -c %s "$G") bytes"
for lock in probes guest wasmtime; do
	awk -v p="$lock" '
		/^name = / { n = $3; gsub(/"/, "", n) }
		/^version = / && n ~ /^(wasmtime|wasmtime-wasi|tokio|wat|rand|serde|serde_json|toml|url|minisign|minisign-verify)$/ {
			v = $3; gsub(/"/, "", v); s = s " " n "@" v
		}
		END { printf "  %-9s%s\n", p, s }' "$lock/Cargo.lock"
done

have_mariadb=no
mariadb -uroot -e 'SELECT 1' >/dev/null 2>&1 && have_mariadb=yes
have_postgres=no
(cd / && su postgres -c "psql -Atc 'SELECT 1'") >/dev/null 2>&1 && have_postgres=yes

section "findings 1 and 9: memory and tables under StoreLimits"
run "$W/memory-limits" multi-instance 16
run "$W/memory-limits" one-big-memory
run "$W/memory-limits" grow-1gib
run "$W/memory-limits" table-grow 50000000
run "$W/memory-limits" table-bomb 8
run "$W/memory-limits" table-bomb 1

section "finding 2: MariaDB read-only transaction and session setting"
if [[ $have_mariadb == yes ]]; then
	run bash sql/mysql-ro.sh
	run bash sql/mysql-ro-prepared.sh
	run bash sql/mysql-ro-session.sh
	run bash sql/mysql-ro-outfile.sh
else
	echo "skipped: MariaDB is not running. Run setup-servers.sh first."
fi

section "findings 3 and 10: a settings migration replaces the admin's statement"
run python3 -I model/named_query_migrate.py

section "finding 5: a 4-byte edit to a serialized component"
mkdir -p "$out/cache"
run "$W/cache-edit" "$out/cache"

section "finding 6: stored steps through serde_json::Value"
run "$P/serde-rt" '{"type":"AddOn","addon":"acme.entra-join","kind":"join","config_version":1,"config":{"tenant":"contoso","group_tag":"lab","max_wait":120}}'
run "$P/serde-rt" '{"type":"AddOn","addon":"acme.x","kind":"k","config_version":1,"config":{"big":12345678901234567890123}}'

section "finding 8: looping guests on the server's tokio runtime"
run "$W/starve" "$G" 1
run "$W/starve" "$G" 8
run "$W/starve" "$G" 8 yield
run "$W/starve" "$G" 8 yield-now
run "$W/starve" "$G" 8 separate

section "finding 11: an add-on reads its own secret file"
run python3 -I model/secret_step_file.py

section "finding 12: unknown keys on field kinds"
run "$P/serdecheck"

section "finding 14: what an empty WASI context still gives"
run "$W/wasi" ambient "$G"
run "$W/wasi" pure "$G"

section "finding 16: compile time and memory of a small hostile component"
run "$W/compile" 4000 4000
run "$W/compile" 8000 8000
printf '$ (ulimit -v 3000000; %s 12000 12000)\n' "\$out/${W#"$out"/}/compile"
(
	ulimit -v 3000000
	"$W/compile" 12000 12000
	echo "exit=$?"
) 2>&1 | grep -v -e '^stack backtrace:' -e '^note: Some details'

section "finding 17: a guest path changes the host"
run "$P/urljoin"

section "finding 18: a waiting upgrade, and one open operation per add-on"
run python3 -I model/waiting_op.py

section "finding 21: the architecture's SQL Server sentence"
run grep -n "On SQL Server, deployarr relies on the read-only account alone" "$repo/docs/architecture.md"

section "finding 22: a blocking host function and a tokio timeout"
run "$W/blocking-host-fn"

section "finding 23: unknown fields on stored steps"
run "$P/serde-action"

section "supported claims: versioned exports and imports"
run "$W/semver"

section "supported claims: cost of a new instance, and memory after traps"
run "$W/wasi" call-cost "$G" 2000
run "$W/wasi" trap-leak "$G" 3000

section "supported claims: minisign-verify 0.3.0"
run "$P/minisign-check"

section "supported claims: PostgreSQL read-only transaction"
if [[ $have_postgres == yes ]]; then
	# The postgres user cannot always enter this folder, so the script runs from /.
	(cd / && run bash "$here/sql/pg-ro.sh")
else
	echo "skipped: PostgreSQL is not running. Run setup-servers.sh first."
fi
