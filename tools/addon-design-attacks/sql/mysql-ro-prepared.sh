#!/bin/bash
# Same test with a server-side prepared statement, as a driver's binary protocol sends it.
mysql -uroot -e "DROP DATABASE IF EXISTS opus_ro; CREATE DATABASE opus_ro; CREATE TABLE opus_ro.orders(id int); INSERT INTO opus_ro.orders VALUES (1),(2);"
mysql -uroot -e "START TRANSACTION READ ONLY; PREPARE s FROM 'DROP TABLE opus_ro.orders'; EXECUTE s; ROLLBACK;" 2>&1
mysql -uroot -N -e "SELECT 'tables after prepared DROP in READ ONLY tx:', COUNT(*) FROM information_schema.tables WHERE table_schema='opus_ro'"
mysql -uroot -e "DROP DATABASE IF EXISTS opus_ro"
