#!/bin/bash
# Does a read-only transaction stop a single write statement on MariaDB?
mysql -uroot -e "DROP DATABASE IF EXISTS opus_ro; CREATE DATABASE opus_ro; CREATE TABLE opus_ro.orders(id int); INSERT INTO opus_ro.orders VALUES (1),(2);"
for stmt in "INSERT INTO opus_ro.orders VALUES (3)" "DELETE FROM opus_ro.orders" "TRUNCATE TABLE opus_ro.orders" "DROP TABLE opus_ro.orders"; do
  echo "== START TRANSACTION READ ONLY; $stmt"
  mysql -uroot -e "START TRANSACTION READ ONLY; $stmt; ROLLBACK;" 2>&1
  mysql -uroot -N -e "SELECT 'tables:', COUNT(*) FROM information_schema.tables WHERE table_schema='opus_ro'; SELECT 'rows:', COUNT(*) FROM opus_ro.orders" 2>&1
done
mysql -uroot -N -e "SELECT VERSION()"
