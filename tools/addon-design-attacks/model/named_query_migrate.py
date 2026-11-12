#!/usr/bin/env python3
"""Findings 3 and 10, rewritten. fable/model/named_query_migrate.py did not survive.

A model of the first draft's rule for SQL in add-ons, as findings 3 and 10 state it:

- The admin writes each statement in a `sql_text` settings field. The host checks it when
  the admin saves: one statement, and only declared parameters.
- `sql.query(connection, query, params)` names the settings path of a `sql_text` field. The
  host reads the statement from the saved settings itself, so the guest never sends SQL.
- On an upgrade, the host calls the new version's `migrate.config("settings", from, doc)`
  and stores the document it returns.

The probe runs the statements in an in-memory SQLite database, which stands in for the
order database. The last lines run the design's change: the host compares every
`sql_text` value of the migrated document with the saved one, by settings path.
"""
import json
import re
import sqlite3

db = sqlite3.connect(":memory:")
db.executescript(
    """
    CREATE TABLE order_rows (board_serial TEXT, os_code TEXT);
    INSERT INTO order_rows VALUES ('MB-0001', 'W11-PRO');
    CREATE TABLE payroll (name TEXT, salary INTEGER);
    INSERT INTO payroll VALUES ('alice', 91000), ('bob', 87000);
    """
)

SQL_TEXT_PATHS = ["rules/0/statement"]  # the `sql_text` fields that the manifest declares


def get(doc, path):
    for part in path.split("/"):
        doc = doc[int(part)] if isinstance(doc, list) else doc.get(part)
        if doc is None:
            return None
    return doc


def save_check(statement):
    """The save-time check: one statement, and only the declared parameter `:value`."""
    if len([part for part in statement.split(";") if part.strip()]) > 1:
        return "more than one statement"
    if set(re.findall(r":([a-z_]+)", statement)) - {"value"}:
        return "an undeclared parameter"
    return None


def host_query(settings, path, value):
    """`sql.query`: the host reads the statement from the saved settings and binds `:value`."""
    statement = get(settings, path)
    try:
        return [row[0] for row in db.execute(statement, {"value": value})], ""
    except sqlite3.Error as e:
        return [], str(e)


class AddOnV1:
    @staticmethod
    def resolve(settings, serial):
        return host_query(settings, "rules/0/statement", serial)


class AddOnV2:
    """The new version. Its settings migration puts its own statement where the admin's was."""

    @staticmethod
    def migrate_config(scope, from_version, doc):
        assert scope == "settings" and from_version == 1
        doc = json.loads(doc)
        doc["rules"][0]["statement"] = "SELECT name || ' earns ' || salary FROM payroll WHERE :value <> ''"
        return json.dumps(doc)

    resolve = AddOnV1.resolve


def first_draft_upgrade(saved, addon):
    """The first draft's host: store whatever `migrate.config` returns."""
    return json.loads(addon.migrate_config("settings", 1, json.dumps(saved)))


def design_upgrade(saved, addon):
    """The design's host: every `sql_text` value must be the same text at the same path,
    or absent, or empty at a path the old document lacked."""
    migrated = json.loads(addon.migrate_config("settings", 1, json.dumps(saved)))
    for path in SQL_TEXT_PATHS:
        old, new = get(saved, path), get(migrated, path)
        if new is None or new == old or (old is None and new == ""):
            continue
        return None, f"refused: the migration changes the admin's statement at {path}"
    return migrated, "accepted"


admin_statement = "SELECT os_code FROM order_rows WHERE board_serial = :value"
saved = {"rules": [{"statement": admin_statement}]}
print("admin typed:            ", admin_statement)
print("v1 resolve returns:     ", AddOnV1.resolve(saved, "MB-0001"))
print("save-time check works:  ", "two statements refused:", save_check(admin_statement + "; DELETE FROM order_rows"))

upgraded = first_draft_upgrade(saved, AddOnV2)
held = get(upgraded, "rules/0/statement")
print("after the upgrade, the statement the host holds:", held)
print("admin typed it:         ", held == admin_statement)
print("v2 resolve returns:     ", AddOnV2.resolve(upgraded, "MB-0001"))

_, verdict = design_upgrade(saved, AddOnV2)
print("with the design's path check, the upgrade is", verdict)
