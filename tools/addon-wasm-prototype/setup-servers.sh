#!/usr/bin/env bash
set -euo pipefail

certs=/etc/probe-certs

DEBIAN_FRONTEND=noninteractive apt-get install -y postgresql mariadb-server openssl >/dev/null

mkdir -p "$certs"
cd "$certs"
if [[ ! -f server.pem ]]; then
	openssl req -x509 -newkey rsa:2048 -nodes -keyout ca.key -out ca.pem -days 30 -subj /CN=probe-ca 2>/dev/null
	openssl req -newkey rsa:2048 -nodes -keyout server.key -out server.csr -subj /CN=127.0.0.1 2>/dev/null
	printf 'subjectAltName=IP:127.0.0.1\nbasicConstraints=CA:FALSE\nextendedKeyUsage=serverAuth\n' >ext.cnf
	openssl x509 -req -in server.csr -CA ca.pem -CAkey ca.key -CAcreateserial -out server.pem -days 30 -extfile ext.cnf 2>/dev/null
	cp server.key server-mysql.key
fi
chmod 644 ca.pem server.pem
chgrp postgres server.key && chmod 640 server.key
chown mysql server-mysql.key && chmod 600 server-mysql.key

pg_version=$(ls /usr/lib/postgresql | sort -n | tail -1)
pg_ctlcluster "$pg_version" main start 2>/dev/null || true
su postgres -c "psql -q -o /dev/null" <<'SQL'
SELECT 'CREATE ROLE probe LOGIN PASSWORD ''probe''' WHERE NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'probe') \gexec
SELECT 'CREATE DATABASE probe OWNER probe' WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname = 'probe') \gexec
ALTER SYSTEM SET ssl = on;
ALTER SYSTEM SET ssl_cert_file = '/etc/probe-certs/server.pem';
ALTER SYSTEM SET ssl_key_file = '/etc/probe-certs/server.key';
SELECT pg_reload_conf();
SQL

printf '[mariadbd]\nssl_cert=%s/server.pem\nssl_key=%s/server-mysql.key\nssl_ca=%s/ca.pem\n' "$certs" "$certs" "$certs" \
	>/etc/mysql/mariadb.conf.d/99-probe-ssl.cnf
mariadb-admin -uroot shutdown 2>/dev/null || true
for _ in $(seq 20); do pgrep -x mariadbd >/dev/null || break; sleep 0.5; done
(mysqld_safe >/dev/null 2>&1 &)
for _ in $(seq 40); do mariadb -uroot -e 'SELECT 1' >/dev/null 2>&1 && break; sleep 0.5; done
mariadb -uroot <<'SQL'
CREATE USER IF NOT EXISTS 'probe'@'127.0.0.1' IDENTIFIED BY 'probe';
CREATE DATABASE IF NOT EXISTS probe;
GRANT SELECT ON probe.* TO 'probe'@'127.0.0.1';
SQL

PGPASSWORD=probe psql "host=127.0.0.1 user=probe dbname=probe sslmode=verify-full sslrootcert=$certs/ca.pem" \
	-Atc 'SELECT version(), ssl FROM pg_stat_ssl JOIN (SELECT 1) x ON true WHERE pid = pg_backend_pid()'
mariadb -h127.0.0.1 -uprobe -pprobe --ssl-ca="$certs/ca.pem" --ssl-verify-server-cert -N \
	-e "SELECT version(); SHOW SESSION STATUS LIKE 'Ssl_cipher'"
