#!/usr/bin/env bash
set -uo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
out=${1:?usage: reproduce.sh <output-dir>}
mkdir -p "$out/logs" "$out/runs"
out=$(cd "$out" && pwd)
jobs=${JOBS:-2}
pg_addr=${PG_ADDR:-127.0.0.1:5432}
my_addr=${MY_ADDR:-127.0.0.1:3306}
ca_file=${CA_FILE:-/etc/probe-certs/ca.pem}
wrong_ca=${WRONG_CA_FILE:-/etc/ssl/certs/ssl-cert-snakeoil.pem}
wasi_include=${WASI_INCLUDE:-/usr/include/wasm32-wasi}

probes=(tiberius tiberius-notls sqlx-postgres sqlx-mysql tokio-postgres tokio-postgres-raw mysql-async wasi-pg-client rustls-aws-lc)

rustup target add wasm32-wasip2 >"$out/logs/rustup.log" 2>&1 || { cat "$out/logs/rustup.log"; exit 1; }

first_error() {
	grep -E '^error' "$1" | grep -v 'could not compile' | sort -u | head -2 | cut -c1-110 | paste -sd'|' | sed 's/|/ | /'
}

build_probe() {
	local probe=$1 cfg=$2 flags=$3
	local log="$out/logs/build-$probe-$cfg.log"
	CARGO_TARGET_DIR="$out/target-$cfg" RUSTFLAGS="$flags" \
		cargo build -j "$jobs" --release --target wasm32-wasip2 \
		--manifest-path "$here/probes/$probe/Cargo.toml" >"$log" 2>&1
}

declare -A plain_build unstable_build plain_err unstable_err
for probe in "${probes[@]}"; do
	if build_probe "$probe" plain ""; then
		plain_build[$probe]=yes
	else
		plain_build[$probe]=no
		plain_err[$probe]=$(first_error "$out/logs/build-$probe-plain.log")
	fi
	if build_probe "$probe" unstable "--cfg tokio_unstable"; then
		unstable_build[$probe]=yes
	else
		unstable_build[$probe]=no
		unstable_err[$probe]=$(first_error "$out/logs/build-$probe-unstable.log")
	fi
	echo "built $probe: plain=${plain_build[$probe]} tokio_unstable=${unstable_build[$probe]}" >&2
done

declare -A extra_build extra_err
build_with_wasi_libc() {
	local name=$1 manifest=$2
	shift 2
	local log="$out/logs/build-$name.log"
	if CFLAGS_wasm32_wasip2="-isystem $wasi_include" CARGO_TARGET_DIR="$out/target-unstable" RUSTFLAGS="--cfg tokio_unstable" \
		cargo build -j "$jobs" --release --target wasm32-wasip2 --manifest-path "$manifest" "$@" >"$log" 2>&1; then
		extra_build[$name]=yes
	else
		extra_build[$name]=no
		extra_err[$name]=$(first_error "$log")
	fi
}

if [[ -d $wasi_include ]]; then
	build_with_wasi_libc rustls-aws-lc+wasi-libc "$here/probes/rustls-aws-lc/Cargo.toml"
	build_with_wasi_libc tiberius+wasi-libc "$here/probes/tiberius/Cargo.toml" --locked
	certs_version=$(awk '/^name = "rustls-native-certs"/ { getline; gsub(/"/, "", $3); print $3 }' "$here/probes/tiberius/Cargo.lock")
	certs_src=$(find "${CARGO_HOME:-$HOME/.cargo}/registry/src" -maxdepth 2 -type d -name "rustls-native-certs-$certs_version" | head -1)
	rm -rf "$out/patched"
	mkdir -p "$out/patched"
	cp -r "$certs_src" "$out/patched/rustls-native-certs"
	cp -r "$here/probes/tiberius" "$out/patched/tiberius"
	python3 - "$out/patched/rustls-native-certs/src/lib.rs" <<'PY'
import sys
path = sys.argv[1]
text = open(path).read()
anchor = '#[cfg(target_os = "macos")]\nuse macos as platform;\n'
assert anchor in text, "rustls-native-certs layout changed"
text = text.replace(anchor, anchor + '\n#[cfg(target_os = "wasi")]\nmod platform {\n    pub fn load_native_certs() -> super::CertificateResult {\n        super::CertificateResult::default()\n    }\n}\n')
open(path, "w").write(text)
PY
	build_with_wasi_libc tiberius+wasi-libc+certs-patch "$out/patched/tiberius/Cargo.toml" \
		--config "patch.crates-io.rustls-native-certs.path=\"$out/patched/rustls-native-certs\""
fi

CARGO_TARGET_DIR="$out/target-host" cargo build -j "$jobs" --release \
	--manifest-path "$here/host/Cargo.toml" >"$out/logs/build-host.log" 2>&1 \
	|| { echo "host build failed, see $out/logs/build-host.log"; exit 1; }
host="$out/target-host/release/addon-host"

component() {
	local dir="$out/target-unstable/wasm32-wasip2/release"
	if [[ -f $dir/probe-$1.wasm ]]; then
		echo "$dir/probe-$1.wasm"
	else
		echo "$dir/probe_${1//-/_}.wasm"
	fi
}

pg_url_for() {
	case $1 in
	sqlx-postgres) echo "postgres://probe:probe@$pg_addr/probe?sslmode=disable" ;;
	wasi-pg-client) echo "postgresql://probe:probe@$pg_addr/probe?sslmode=disable" ;;
	tokio-postgres-raw) echo "host=${pg_addr%:*} port=${pg_addr#*:} user=probe password=probe dbname=probe sslmode=${2:-disable}" ;;
	esac
}

my_url() {
	case $1 in
	sqlx-mysql) echo "mysql://probe:probe@$my_addr/probe?ssl-mode=disabled" ;;
	mysql-async) echo "mysql://probe:probe@$my_addr/probe" ;;
	esac
}

run_case() {
	local name=$1 wasm=$2 addr=$3
	shift 3
	local log="$out/runs/$name.log"
	timeout 120 "$host" "$wasm" --allow "$addr" --env "DB_ADDR=$addr" "$@" >"$log" 2>&1
	local code=$?
	echo "$code" >"$out/runs/$name.exit"
	return "$code"
}

summarize_run() {
	local log="$out/runs/$1.log"
	if [[ $(cat "$out/runs/$1.exit") == 0 ]]; then
		local tls
		tls=$(grep -m1 '^tls = ' "$log" | cut -d' ' -f3)
		echo "ok${tls:+ tls=$tls}"
	else
		local reason
		reason=$(grep -m1 -oE 'UnknownIssuer|PermissionDenied|panicked.*' "$log" || grep -m1 -vE '^host: ' "$log")
		echo "FAIL: ${reason:0:60}"
	fi
}

declare -A plain_run tls_run wrong_ca_run
have_ca=no
[[ -r $ca_file ]] && have_ca=yes
for probe in "${probes[@]}"; do
	[[ ${unstable_build[$probe]} == yes ]] || continue
	wasm=$(component "$probe")
	case $probe in
	sqlx-postgres | tokio-postgres-raw)
		run_case "$probe-plain" "$wasm" "$pg_addr" --env "DB_URL=$(pg_url_for "$probe")"
		plain_run[$probe]=$(summarize_run "$probe-plain")
		if [[ $have_ca == yes ]]; then
			run_case "$probe-tls" "$wasm" "$pg_addr" --env "DB_URL=$(pg_url_for "$probe" require)" --env "CA_PEM=$(cat "$ca_file")"
			tls_run[$probe]=$(summarize_run "$probe-tls")
			run_case "$probe-wrongca" "$wasm" "$pg_addr" --env "DB_URL=$(pg_url_for "$probe" require)" --env "CA_PEM=$(cat "$wrong_ca")"
			wrong_ca_run[$probe]=$(summarize_run "$probe-wrongca")
		fi
		;;
	wasi-pg-client)
		run_case "$probe-plain" "$wasm" "$pg_addr" --p3 --env "DB_URL=$(pg_url_for "$probe")"
		plain_run[$probe]=$(summarize_run "$probe-plain")
		;;
	sqlx-mysql | mysql-async)
		run_case "$probe-plain" "$wasm" "$my_addr" --env "DB_URL=$(my_url "$probe")"
		plain_run[$probe]=$(summarize_run "$probe-plain")
		if [[ $have_ca == yes ]]; then
			run_case "$probe-tls" "$wasm" "$my_addr" --env "DB_URL=$(my_url "$probe")" --env "CA_PEM=$(cat "$ca_file")"
			tls_run[$probe]=$(summarize_run "$probe-tls")
			run_case "$probe-wrongca" "$wasm" "$my_addr" --env "DB_URL=$(my_url "$probe")" --env "CA_PEM=$(cat "$wrong_ca")"
			wrong_ca_run[$probe]=$(summarize_run "$probe-wrongca")
		fi
		;;
	esac
done

wrong_port="${pg_addr%:*}:$((${pg_addr#*:} + 1))"
"$host" "$(component sqlx-postgres)" --allow "$wrong_port" \
	--env "DB_URL=$(pg_url_for sqlx-postgres)" >"$out/runs/deny.log" 2>&1
deny_exit=$?

printf '\n%-19s %-6s %-15s %-9s %-12s %-35s %s\n' probe default_cfg tokio_unstable wasm_kib plain_run tls_run untrusted_ca_run
for probe in "${probes[@]}"; do
	wasm=$(component "$probe")
	size=-
	[[ ${unstable_build[$probe]} == yes ]] && size=$(($(stat -c %s "$wasm") / 1024))
	printf '%-19s %-6s %-15s %-9s %-12s %-35s %s\n' "$probe" "${plain_build[$probe]}" "${unstable_build[$probe]}" \
		"$size" "${plain_run[$probe]:--}" "${tls_run[$probe]:--}" "${wrong_ca_run[$probe]:--}"
done

echo
echo "First build error per probe:"
for probe in "${probes[@]}"; do
	[[ -n ${plain_err[$probe]:-} ]] && echo "  $probe (default cfg): ${plain_err[$probe]}"
	[[ -n ${unstable_err[$probe]:-} ]] && echo "  $probe (tokio_unstable): ${unstable_err[$probe]}"
done

if ((${#extra_build[@]})); then
	echo
	echo "Builds with tokio_unstable and the WASI libc headers in $wasi_include:"
	for name in rustls-aws-lc+wasi-libc tiberius+wasi-libc tiberius+wasi-libc+certs-patch; do
		[[ -n ${extra_build[$name]:-} ]] || continue
		printf '  %-32s %-4s %s\n' "$name" "${extra_build[$name]}" "${extra_err[$name]:-}"
	done
fi

echo
echo "Network allow list check: allowed $wrong_port, connected to $pg_addr, exit $deny_exit"
grep -m2 'socket check' "$out/runs/deny.log" | sed 's/^/  /'

echo
echo "Timings per run (ms):"
for f in "$out"/runs/*-plain.log "$out"/runs/*-tls.log "$out"/runs/deny.log; do
	[[ -f $f ]] || continue
	printf '  %-28s %s\n' "$(basename "$f" .log)" "$(grep -m1 -o 'compile_ms=.*run_ms=[0-9.]*' "$f")"
done

echo
echo "Host binary: $(($(stat -c %s "$host") / 1024)) KiB ($host)"
echo "Resolved versions:"
for probe in "${probes[@]}" ../host; do
	lock="$here/probes/$probe/Cargo.lock"
	[[ -f $lock ]] || continue
	awk -v p="$(basename "$probe")" '
		/^name = / { n = $3; gsub(/"/, "", n) }
		/^version = / && n ~ /^(tokio|mio|rustls|ring|aws-lc-sys|tokio-rustls|sqlx|sqlx-core|tokio-postgres|mysql_async|tiberius|rustls-native-certs|wasmtime|wasmtime-wasi)$/ {
			v = $3; gsub(/"/, "", v); out = out " " n "@" v
		}
		END { printf "  %-20s%s\n", p, out }' "$lock"
done
