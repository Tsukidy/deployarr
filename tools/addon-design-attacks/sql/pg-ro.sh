#!/bin/bash
# What one statement can still do inside BEGIN READ ONLY on PostgreSQL, with an account that may write.
# Uses its own database, fable_ro, and its own role, fable_rw.
su postgres -c "psql -q -At" <<'SQL' 2>&1 | grep -v "does not exist, skipping"
DROP DATABASE IF EXISTS fable_ro;
DROP ROLE IF EXISTS fable_rw;
CREATE ROLE fable_rw LOGIN PASSWORD 'fable';
CREATE DATABASE fable_ro OWNER fable_rw;
SQL
export PGPASSWORD=fable
P="psql -h 127.0.0.1 -U fable_rw -d fable_ro -X -q -At"
$P -c "CREATE TABLE orders(id int primary key, os_code text); INSERT INTO orders VALUES (1,'W11'),(2,'W10'); CREATE SEQUENCE s;"
run() { # one statement through the extended protocol (\bind), inside a read-only transaction
  printf '== %s\n' "$1"
  printf 'BEGIN READ ONLY;\n%s \\bind \\g\nCOMMIT;\n' "$1" | $P 2>&1 | sed 's/^/   /'
}
run "INSERT INTO orders VALUES (3,'X')"
run "WITH d AS (DELETE FROM orders RETURNING id) SELECT count(*) FROM d"
run "SELECT nextval('s')"
run "DROP TABLE orders"
run "SELECT * FROM orders FOR UPDATE"
run "DO \$\$ BEGIN PERFORM set_config('transaction_read_only','off',true); DELETE FROM orders; END \$\$"
run "SELECT set_config('transaction_read_only','off',true)"
run "SELECT query_to_xml('delete from orders returning id', true, false, '')"
run "CREATE TEMP TABLE scratch AS SELECT * FROM orders"
run "SELECT pg_advisory_lock(42)"
run "SELECT set_config('search_path','pg_temp',false)"
run "INSERT INTO orders VALUES (4,'two'); DELETE FROM orders"
echo "== rows left in orders, and session state the same connection would keep:"
$P -c "SELECT count(*) FROM orders"
printf "BEGIN READ ONLY;\nSELECT pg_advisory_lock(42) \\\\bind \\\\g\nSELECT set_config('search_path','pg_temp',false) \\\\bind \\\\g\nCOMMIT;\nSELECT 'after COMMIT: advisory locks held = ' || count(*) FROM pg_locks WHERE locktype='advisory' AND pid=pg_backend_pid();\nSHOW search_path;\n" | $P 2>&1 | sed 's/^/   /'
$P -c "SELECT version()"
