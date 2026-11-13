#!/bin/bash
# Rewritten. Finding 2 says root wrote a file with SELECT ... INTO OUTFILE inside a read-only
# transaction on MariaDB. That command did not survive. This script runs it under each guard
# and then looks for the file. It uses the same database as the scripts beside it.
mysql -uroot -e "DROP DATABASE IF EXISTS opus_ro; CREATE DATABASE opus_ro; CREATE TABLE opus_ro.orders(id int); INSERT INTO opus_ro.orders VALUES (1),(2);"
for guard in "START TRANSACTION READ ONLY" "SET SESSION TRANSACTION READ ONLY"; do
  f=/tmp/opus_ro_outfile_$$_$RANDOM.txt
  echo "== $guard; SELECT * FROM opus_ro.orders INTO OUTFILE '<file>'"
  mysql -uroot -e "$guard; SELECT * FROM opus_ro.orders INTO OUTFILE '$f';" 2>&1
  if [[ -f $f ]]; then
    echo "file written by the server: $(wc -l <"$f") lines"
    rm -f "$f"
  else
    echo "no file"
  fi
done
mysql -uroot -N -e "SELECT 'secure_file_priv:', IFNULL(@@secure_file_priv, 'NULL')"
mysql -uroot -e "DROP DATABASE IF EXISTS opus_ro"
