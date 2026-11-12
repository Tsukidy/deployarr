#!/bin/bash
# Alternative implementation: session-level read-only characteristic instead of START TRANSACTION READ ONLY.
mysql -uroot -e "DROP DATABASE IF EXISTS opus_ro; CREATE DATABASE opus_ro; CREATE TABLE opus_ro.orders(id int); INSERT INTO opus_ro.orders VALUES (1),(2);"
for stmt in "DELETE FROM opus_ro.orders" "TRUNCATE TABLE opus_ro.orders" "DROP TABLE opus_ro.orders"; do
  echo "== SET SESSION TRANSACTION READ ONLY; $stmt"
  mysql -uroot -e "SET SESSION TRANSACTION READ ONLY; $stmt;" 2>&1 | grep -v -- '^---\|^$'
  mysql -uroot -N -e "SELECT 'tables:', COUNT(*) FROM information_schema.tables WHERE table_schema='opus_ro'; SELECT 'rows:', COUNT(*) FROM opus_ro.orders" 2>&1 | grep -v -- '^---\|^$'
done
mysql -uroot -e "DROP DATABASE IF EXISTS opus_ro"
