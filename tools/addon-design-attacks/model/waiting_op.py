#!/usr/bin/env python3
"""Finding 18, rewritten, and the supported claim about one open operation per add-on.
sonnet/journal/waiting-op.py did not survive.

A model of the first draft's operation journal in SQLite. Each install, upgrade, roll back,
or uninstall writes one row. A partial unique index allows one open row per add-on, where a
row is open until it reaches `Committed` or `Abandoned`. In the first draft, `Abandoned` is
set only by a failure before `Committing`. An upgrade with `WhenBusy::Wait` writes its row
in `Approved` and waits while deployments run.

Steps 1 to 4 replay the finding. The step 5 of the original, which listed the first draft's
`AddOnHost` methods, read the first draft's sketch, which is gone. The last lines run the
design's change: `cancel` abandons a waiting operation, and `Force` for the same package
takes the waiting row over.
"""
import sqlite3

db = sqlite3.connect(":memory:")
db.executescript(
    """
    CREATE TABLE addon_ops (id INTEGER PRIMARY KEY, addon_id TEXT NOT NULL, kind TEXT NOT NULL,
                            target TEXT, phase TEXT NOT NULL);
    CREATE UNIQUE INDEX one_open_op ON addon_ops(addon_id)
        WHERE phase NOT IN ('Committed', 'Abandoned');
    """
)
running_deployments = 2


def open_op(addon, kind, target, phase):
    try:
        with db:
            cur = db.execute(
                "INSERT INTO addon_ops (addon_id, kind, target, phase) VALUES (?, ?, ?, ?)",
                (addon, kind, target, phase),
            )
        return f"row written in phase {phase}", cur.lastrowid
    except sqlite3.IntegrityError as e:
        return f"Conflict: {e}", None


def apply(addon, target, when_busy):
    phase = "Approved" if running_deployments and when_busy == "Wait" else "Preparing"
    return open_op(addon, "upgrade", target, phase)


def finish(row):
    with db:
        db.execute("UPDATE addon_ops SET phase = 'Committed' WHERE id = ?", (row,))


print("--- the supported claim: one open operation per add-on")
msg, first = open_op("acme.a", "install", "1.0.0", "Preparing")
print("first open row for acme.a:", msg)
print("second open row for acme.a:", open_op("acme.a", "upgrade", "1.1.0", "Preparing")[0])
print("open row for another add-on, acme.b:", open_op("acme.b", "install", "1.0.0", "Preparing")[0])
finish(first)
print("after the first row commits, a new row for acme.a:", open_op("acme.a", "upgrade", "1.1.0", "Preparing")[0])

print("--- finding 18: the first draft")
msg, waiting = apply("acme.join", "1.1.0", "Wait")
print(f"1. upgrade to 1.1.0 with WhenBusy::Wait: {msg} ({running_deployments} deployments are running)")
print("2. admin retries the upgrade with WhenBusy::Force ->", apply("acme.join", "1.1.0", "Force")[0])
print("3. admin clicks Roll back ->", open_op("acme.join", "roll_back", "1.0.0", "Preparing")[0])
print("4. admin clicks Uninstall ->", open_op("acme.join", "uninstall", None, "Preparing")[0])

print("--- the design's change")


def force_takes_over(addon, target):
    with db:
        n = db.execute(
            "UPDATE addon_ops SET phase = 'Preparing' WHERE addon_id = ? AND phase = 'Approved' AND target = ?",
            (addon, target),
        ).rowcount
    return f"took over {n} waiting row(s)" if n else apply(addon, target, "Force")[0]


def cancel(addon):
    with db:
        n = db.execute("UPDATE addon_ops SET phase = 'Abandoned' WHERE addon_id = ? AND phase = 'Approved'", (addon,)).rowcount
    return f"abandoned {n} waiting row(s)"


print("Force for the same package (1.1.0) ->", force_takes_over("acme.join", "1.1.0"))
finish(waiting)
msg, _ = apply("acme.join", "1.2.0", "Wait")
print("upgrade to 1.2.0 with WhenBusy::Wait ->", msg)
print("cancel ->", cancel("acme.join"))
print("then Roll back ->", open_op("acme.join", "roll_back", "1.0.0", "Preparing")[0])
print("SQLite", sqlite3.sqlite_version)
