#!/usr/bin/env bash
# Installs and starts PostgreSQL and MariaDB for the SQL probes in sql/.
# Needs root and apt-get. It was run on Ubuntu 24.04. It can run again.
set -euo pipefail

if ! command -v mariadb >/dev/null || ! command -v pg_ctlcluster >/dev/null; then
	apt-get update -qq
	DEBIAN_FRONTEND=noninteractive apt-get install -y -qq postgresql mariadb-server >/dev/null
fi

pg_version=$(ls /usr/lib/postgresql | sort -n | tail -1)
pg_ctlcluster "$pg_version" main start 2>/dev/null || true

if ! mariadb -uroot -e 'SELECT 1' >/dev/null 2>&1; then
	(mysqld_safe >/dev/null 2>&1 &)
	for _ in $(seq 40); do
		mariadb -uroot -e 'SELECT 1' >/dev/null 2>&1 && break
		sleep 0.5
	done
fi

su postgres -c "psql -Atc 'SELECT version()'"
mariadb -uroot -N -e 'SELECT VERSION()'
