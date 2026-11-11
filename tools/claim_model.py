#!/usr/bin/env python3
"""Exhaustive check of the claim protocol in docs/agents/issue-tracker.md and
the hand-off steps under "Take a ticket" in AGENTS.md.

The model holds one ticket: an append-only comment log, the assignee flag, the
state label, whether an open issue blocks it, and whether it is open. Each
session runs the documented steps, one tool call per atomic action, and may
crash before any action. A session that holds the ticket can finish it, release
it, hand it to a human, wait on an owner question, or close it as not planned.
The owner can answer an open question and free a gone holder. In some runs,
the ticket starts held by a branch whose session is gone, and the first session
is one the owner named in its prompt: it frees that branch, skips the label
test, and stops instead of moving to another ticket.

The checker explores every interleaving and checks these properties:

  a. At most one session believes it holds the ticket, and it is the holder
     that the comment log names.
  b. With no crash, a ticket that is still open ends unassigned and free.
  c. Once nothing can move, an open, ready, unblocked ticket is free, unless a
     crashed session holds it and the owner has not acted.
  d. A session works the ticket only while it is open, ready for agents, and
     unblocked. A named session needs only open and unblocked.
  e. A session works the ticket only while it is assigned, so GitHub shows it
     as taken.

The model assumes claim comments are never edited or deleted. It does not
cover reopening a closed issue or a pull request that waits for review.

Usage: claim_model.py   (exits 1 and prints a trace if a property fails)
"""
import itertools
import sys
from collections import deque

CLAIM, RELEASE, WITHDRAW = "Claimed", "Released", "Withdrawn"
GONE = "gone"  # the branch of a session that ended before the run
AGENT, HUMAN, WONTFIX = "ready-for-agent", "ready-for-human", "wontfix"


def holder(log, honor_release=True):
    """Replay the comment log. Returns the branch that holds the ticket, or None."""
    held = None
    for kind, branch in log:
        if kind == CLAIM and held is None:
            held = branch
        elif kind == RELEASE and held == branch and honor_release:
            held = None
    return held


# Session steps, named after the tool call each one makes.
(FRONTIER, READ_COMMENTS, COMMENT_CLAIM, REREAD_COMMENTS, COMMENT_WITHDRAW, VERIFY, LOST_UNASSIGN,
 COMMENT_LOST, ASSIGN, WORK, RELEASE_UNASSIGN, RELEASE_COMMENT, HUMAN_RELABEL, HUMAN_COMMENT,
 QUESTION_EDIT, QUESTION_COMMENT, NOT_PLANNED_CLOSE, NOT_PLANNED_COMMENT, NAMED_READ, NAMED_FREE,
 DONE, CRASHED) = range(22)
# Steps where the session believes it holds the ticket.
HOLDING = {LOST_UNASSIGN, COMMENT_LOST, ASSIGN, WORK, RELEASE_UNASSIGN, RELEASE_COMMENT, HUMAN_RELABEL, HUMAN_COMMENT,
           QUESTION_EDIT, QUESTION_COMMENT, NOT_PLANNED_CLOSE, NOT_PLANNED_COMMENT}


class Model:
    """`honor_release`, `recheck`, `verify`, `verify_unassign`, `relabel_first` and
    `unassign_first` exist so the tests can show that the checker catches a
    protocol without each safeguard."""

    def __init__(self, sessions, attempts, owner, log=(), assigned=False, blocked=False, named=False,
                 honor_release=True, recheck=True, verify=True, verify_unassign=True,
                 relabel_first=True, unassign_first=True):
        self.attempts = attempts
        self.owner = owner
        self.honor_release = honor_release
        self.recheck = recheck
        self.verify = verify
        self.relabel_first = relabel_first
        self.unassign_first = unassign_first
        self.verify_unassign = verify_unassign
        self.named = named
        starts = [(NAMED_READ if named and i == 0 else FRONTIER, 0) for i in range(sessions)]
        # (log, assigned, label, blocked, is_open, sessions, owner_pending_release)
        self.initial = (tuple(log), assigned, AGENT, blocked, True, tuple(starts), None)

    def holder(self, log):
        return holder(log, self.honor_release)

    def holding(self, step):
        """True when a session at this step believes it holds the ticket."""
        if step == RELEASE_UNASSIGN and not self.unassign_first:
            return False  # in that order the release comment came first
        if step == HUMAN_RELABEL and not self.relabel_first:
            return False
        return step in HOLDING

    def successors(self, state):
        log, assigned, label, blocked, is_open, sessions, pending = state
        ready = is_open and label == AGENT and not blocked
        out = []
        for i, (step, tries) in enumerate(sessions):
            if step in (DONE, CRASHED):
                continue
            me = f"s{i}"
            named = self.named and i == 0
            # A named session stops instead of going back to the frontier.
            back = DONE if named else FRONTIER

            def go(text, new_step, log=log, assigned=assigned, label=label, blocked=blocked,
                   is_open=is_open, tries=tries):
                updated = list(sessions)
                updated[i] = (new_step, tries)
                out.append(((log, assigned, label, blocked, is_open, tuple(updated), pending), f"{me} {text}"))

            go("crashes", CRASHED)
            if step == FRONTIER:
                if ready and tries < self.attempts:
                    go("reads the issue: on the frontier", READ_COMMENTS, tries=tries + 1)
                else:
                    go("reads the issue: not on the frontier", DONE)
            elif step == NAMED_READ:
                held = self.holder(log)
                if held is None:
                    go("reads comments: free", COMMENT_CLAIM)
                elif held == GONE:
                    go(f"reads comments: {GONE}, the branch its prompt names, holds", NAMED_FREE)
                else:
                    go("reads comments: another session holds it, stops", DONE)
            elif step == NAMED_FREE:
                # The prompt names the gone branch, so the session releases that
                # branch, never whoever holds the ticket by the time it comments.
                go(f"comments Released {GONE}", NAMED_READ, log=log + ((RELEASE, GONE),))
            elif step == READ_COMMENTS:
                go("reads comments", COMMENT_CLAIM if self.holder(log) is None else FRONTIER)
            elif step == COMMENT_CLAIM:
                go("comments Claimed", REREAD_COMMENTS, log=log + ((CLAIM, me),))
            elif step == REREAD_COMMENTS:
                won = self.holder(log) == me or not self.recheck
                go("rereads comments", (VERIFY if self.verify else ASSIGN) if won else COMMENT_WITHDRAW)
            elif step == COMMENT_WITHDRAW:
                go("comments Withdrawn", back, log=log + ((WITHDRAW, me),))
            elif step == VERIFY:
                fit = is_open and not blocked and (named or label == AGENT)
                lost = LOST_UNASSIGN if self.verify_unassign else COMMENT_LOST
                go("rereads the issue", ASSIGN if fit else lost)
            elif step == LOST_UNASSIGN:
                go("clears the assignee (issue changed)", COMMENT_LOST, assigned=False)
            elif step == COMMENT_LOST:
                go("comments Released (issue changed)", back, log=log + ((RELEASE, me),))
            elif step == ASSIGN:
                go("assigns itself", WORK, assigned=True)
            elif step == WORK:
                go("merges its pull request", DONE, is_open=False)
                go("starts a release", RELEASE_UNASSIGN if self.unassign_first else RELEASE_COMMENT)
                go("starts a hand-off to a human", HUMAN_RELABEL if self.relabel_first else HUMAN_COMMENT)
                go("starts waiting on a question", QUESTION_EDIT)
                go("starts a not-planned close", NOT_PLANNED_CLOSE)
            elif step == RELEASE_UNASSIGN:
                go("unassigns", RELEASE_COMMENT if self.unassign_first else back, assigned=False)
            elif step == RELEASE_COMMENT:
                go("comments Released", back if self.unassign_first else RELEASE_UNASSIGN,
                   log=log + ((RELEASE, me),))
            elif step == HUMAN_RELABEL:
                go("relabels ready-for-human and unassigns", HUMAN_COMMENT if self.relabel_first else DONE,
                   label=HUMAN, assigned=False)
            elif step == HUMAN_COMMENT:
                go("comments Released", DONE if self.relabel_first else HUMAN_RELABEL, log=log + ((RELEASE, me),))
            elif step == QUESTION_EDIT:
                go("adds an open question to Blocked by", QUESTION_COMMENT, blocked=True)
            elif step == QUESTION_COMMENT:
                go("comments Waiting on the question", RELEASE_UNASSIGN if self.unassign_first else RELEASE_COMMENT)
            elif step == NOT_PLANNED_CLOSE:
                go("closes as not planned and unassigns", NOT_PLANNED_COMMENT,
                   is_open=False, label=WONTFIX, assigned=False)
            elif step == NOT_PLANNED_COMMENT:
                go("comments Released", DONE, log=log + ((RELEASE, me),))
        if self.owner:
            held = self.holder(log)
            if blocked:
                out.append(((log, assigned, label, False, is_open, sessions, pending), "owner closes the question"))
            if pending is not None:
                out.append(((log + ((RELEASE, pending),), assigned, label, blocked, is_open, sessions, None),
                            f"owner comments Released {pending}"))
            elif held is not None and self.gone(held, sessions):
                out.append(((log, False, label, blocked, is_open, sessions, held), f"owner unassigns {held}"))
        return out

    @staticmethod
    def gone(branch, sessions):
        """A branch whose session crashed, or one from before the run."""
        if branch.startswith("s") and branch[1:].isdigit():
            return sessions[int(branch[1:])][0] == CRASHED
        return True

    def violations(self, state, stuck):
        log, assigned, label, blocked, is_open, sessions, _ = state
        held = self.holder(log)
        believers = [i for i, (step, _) in enumerate(sessions) if self.holding(step)]
        working = [i for i, (step, _) in enumerate(sessions) if step == WORK]
        found = []
        if len(believers) > 1 or any(held != f"s{i}" for i in believers):
            found.append("a")
        for i in working:
            named = self.named and i == 0
            if not (is_open and not blocked and (named or label == AGENT)):
                found.append("d")
        if working and not assigned:
            found.append("e")
        if stuck:
            gone = any(step == CRASHED for step, _ in sessions) or (held is not None and self.gone(held, sessions))
            if not gone and is_open and (assigned or held is not None):
                found.append("b")
            ready = is_open and label == AGENT and not blocked
            if ready and held is not None and not (self.gone(held, sessions) and not self.owner):
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
            for new_state, text in nxt:
                if new_state not in parents:
                    parents[new_state] = (state, text)
                    queue.append(new_state)
        return len(parents), None

    @staticmethod
    def trace(parents, state):
        steps = []
        while parents[state] is not None:
            state, text = parents[state]
            steps.append(text)
        return list(reversed(steps))


HELD_BY_GONE = ((CLAIM, GONE),)
# name: (comment log, assigned, blocked, the first session is named in its prompt)
HISTORIES = {
    "no comments": ((), False, False, False),
    "an old released claim": (((CLAIM, "old"), (RELEASE, "old")), False, False, False),
    "an old lost race": (((CLAIM, "old"), (CLAIM, "other"), (WITHDRAW, "other"), (RELEASE, "old")), False, False, False),
    "a gone holder, assigned, named session": (HELD_BY_GONE, True, False, True),
    "a gone holder, unassigned, named session": (HELD_BY_GONE, False, False, True),
    "a gone holder waiting on a question, named session": (HELD_BY_GONE, True, True, True),
}

CONFIGS = [(2, 1), (2, 2), (3, 1)]


def run_all(**variant):
    """Check every configuration. Returns (total states, list of failures)."""
    total, failures = 0, []
    for (sessions, attempts), owner, (name, (log, assigned, blocked, named)) in itertools.product(
            CONFIGS, (False, True), HISTORIES.items()):
        model = Model(sessions, attempts, owner, log, assigned=assigned, blocked=blocked, named=named, **variant)
        states, failure = model.check()
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
