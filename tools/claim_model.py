#!/usr/bin/env python3
"""Exhaustive check of the claim protocol in docs/agents/issue-tracker.md.

The model holds one issue: an append-only comment log and an assignee flag.
Each session runs the documented steps, one tool call per atomic action, and
may crash before any action. The owner may free a crashed holder by posting a
release comment and then unassigning. The checker explores every interleaving
and checks three properties:

  a. At most one session believes it holds the ticket, and it is the holder
     that the comment log names.
  b. With no crash, the ticket ends free and unassigned.
  c. Once nothing can move, the ticket is free, unless a crashed session holds
     it and the owner has not acted.

Usage: claim_model.py   (exits 1 and prints a trace if a property fails)
"""
import itertools
import sys
from collections import deque

CLAIM, RELEASE, WITHDRAW = "Claimed", "Released", "Withdrawn"


def holder(log, honor_release=True):
    """Replay the comment log. Returns the branch that holds the ticket, or None."""
    held = None
    for kind, branch in log:
        if kind == CLAIM and held is None:
            held = branch
        elif kind == RELEASE and held == branch and honor_release:
            held = None
    return held


# Session states. A session in HOLDING believes it holds the ticket.
START, COMMENT, RECHECK, WITHDRAWING, ASSIGN, HOLD, RELEASE_COMMENT, UNASSIGN, DONE, CRASHED = range(10)
HOLDING = {ASSIGN, HOLD, RELEASE_COMMENT}


class Model:
    def __init__(self, sessions, attempts, owner, log=(), honor_release=True, recheck=True):
        self.sessions = sessions
        self.attempts = attempts
        self.owner = owner
        self.honor_release = honor_release
        self.recheck = recheck
        self.initial = (tuple(log), False, tuple((START, 0) for _ in range(sessions)), False)

    def holder(self, log):
        return holder(log, self.honor_release)

    def successors(self, state):
        log, assigned, sessions, owner_unassign_next = state
        out = []
        for i, (pc, tries) in enumerate(sessions):
            if pc in (DONE, CRASHED):
                continue
            me = f"s{i}"

            def go(label, new_pc, new_log=log, new_assigned=assigned, new_tries=tries):
                updated = list(sessions)
                updated[i] = (new_pc, new_tries)
                out.append(((new_log, new_assigned, tuple(updated), owner_unassign_next), f"{me} {label}"))

            go("crashes", CRASHED)
            if pc == START:
                free = self.holder(log) is None and tries < self.attempts
                go("reads comments", COMMENT if free else DONE, new_tries=tries + 1 if free else tries)
            elif pc == COMMENT:
                go("comments Claimed", RECHECK, new_log=log + ((CLAIM, me),))
            elif pc == RECHECK:
                won = self.holder(log) == me or not self.recheck
                go("rereads comments", ASSIGN if won else WITHDRAWING)
            elif pc == WITHDRAWING:
                go("comments Withdrawn", START, new_log=log + ((WITHDRAW, me),))
            elif pc == ASSIGN:
                go("assigns", HOLD, new_assigned=True)
            elif pc == HOLD:
                go("decides to release", RELEASE_COMMENT)
            elif pc == RELEASE_COMMENT:
                go("comments Released", UNASSIGN, new_log=log + ((RELEASE, me),))
            elif pc == UNASSIGN:
                go("unassigns", START, new_assigned=False)
        if self.owner:
            held = self.holder(log)
            if owner_unassign_next:
                out.append(((log, False, sessions, False), "owner unassigns"))
            elif held is not None and self.crashed(held, sessions):
                out.append(((log + ((RELEASE, held),), assigned, sessions, True), f"owner comments Released {held}"))
        return out

    @staticmethod
    def crashed(branch, sessions):
        return branch.startswith("s") and branch[1:].isdigit() and sessions[int(branch[1:])][0] == CRASHED

    def violations(self, state, stuck):
        log, assigned, sessions, _ = state
        held = self.holder(log)
        believers = [i for i, (pc, _) in enumerate(sessions) if pc in HOLDING]
        found = []
        if len(believers) > 1 or any(held != f"s{i}" for i in believers):
            found.append("a")
        if stuck:
            if not any(pc == CRASHED for pc, _ in sessions) and (held is not None or assigned):
                found.append("b")
            if held is not None and not (self.crashed(held, sessions) and not self.owner):
                found.append("c")
        return found

    def check(self):
        """Explore every interleaving. Returns (states explored, first violation or None)."""
        parents = {self.initial: None}
        queue = deque([self.initial])
        while queue:
            state = queue.popleft()
            nxt = self.successors(state)
            found = self.violations(state, stuck=not nxt)
            if found:
                return len(parents), (found, self.trace(parents, state))
            for new_state, label in nxt:
                if new_state not in parents:
                    parents[new_state] = (state, label)
                    queue.append(new_state)
        return len(parents), None

    @staticmethod
    def trace(parents, state):
        steps = []
        while parents[state] is not None:
            state, label = parents[state]
            steps.append(label)
        return list(reversed(steps))


HISTORIES = {
    "no comments": (),
    "an old released claim": ((CLAIM, "old"), (RELEASE, "old")),
    "an old lost race": ((CLAIM, "old"), (CLAIM, "other"), (WITHDRAW, "other"), (RELEASE, "old")),
}

CONFIGS = [(2, 1), (2, 2), (3, 1)]


def run_all(**variant):
    """Check every configuration. Returns (total states, list of failures)."""
    total, failures = 0, []
    for (sessions, attempts), owner, (name, log) in itertools.product(CONFIGS, (False, True), HISTORIES.items()):
        states, failure = Model(sessions, attempts, owner, log, **variant).check()
        total += states
        if failure:
            failures.append((f"{sessions} sessions, {attempts} attempts, owner={owner}, {name}", failure))
    return total, failures


def main():
    total, failures = run_all()
    for config, (props, trace) in failures:
        print(f"property {', '.join(props)} fails with {config}:")
        print("  " + " -> ".join(trace or ["(initial state)"]))
    print(f"{total} states explored, {len(failures)} failing configurations")
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()
