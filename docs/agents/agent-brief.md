# Agent brief

An agent brief is the specification an AFK agent works from. Triage posts one as a comment before it moves an issue to `ready-for-agent`. After that, the brief is the contract, and the issue body and discussion are context. A ticket that an agent writes is itself a brief, so its body follows the ticket layout at the end of this page.

This page is adapted from `AGENT-BRIEF.md` and `to-tickets` in [mattpocock/skills](https://github.com/mattpocock/skills), which also has worked examples. See [Third-party notices](third-party-notices.md).

## Principles

### Durability over precision

An issue can wait in `ready-for-agent` for weeks while the code changes. Write the brief so it stays useful after files move.

- Describe interfaces, types, and behavioral contracts.
- Name the types, function signatures, or config shapes the agent should look for or change.
- Point to docs by section, such as a heading in `docs/architecture.md`.
- Do not cite paths or line numbers in this repository's code. They go stale.

### Behavioral, not procedural

Describe what the system should do, not how to build it. The agent explores the code fresh and makes its own implementation choices.

### Complete acceptance criteria

Every brief has concrete criteria that can each be checked on its own. "Triage should work" is not a criterion.

### Explicit scope boundaries

Say what is out of scope, so the agent does not gold-plate or guess at nearby features.

## Brief template

Triage posts this as a comment. For `ready-for-human`, use the same layout and add why an agent cannot do it: a judgment call, external access, a design decision, hardware, or manual testing. For a ticket with the `lab` label, say that it needs the VM lab.

The brief holds no blockers. Triage adds a `## Blocked by` section to the end of the issue body instead, because the frontier and the Blocked by workflow read only the body.

```markdown
## Agent brief

**Category:** bug or enhancement
**Summary:** one line on what needs to happen

**Current behavior:**
What happens now. For a bug, the broken behavior.

**Desired behavior:**
What should happen when the work is done, including edge cases and errors.

**Key interfaces:**
- `TypeName`: what changes and why

**Acceptance criteria:**
- [ ] A specific, testable criterion

**Out of scope:**
- Something that must not change in this issue
```

## Write tickets

- Before you publish tickets, show the owner the breakdown as a numbered list. Give each ticket's title, its blockers, and what it delivers, and ask whether the granularity and the blockers are right. Publish only after the owner approves the list in the session. If the owner is not in the session, as in a scheduled run, publish nothing as `ready-for-agent`.
- Label each approved ticket yourself, with its category and its state. Use `ready-for-agent` when an agent can finish it. Add `lab` when the work needs the VM lab, and say so in the body. Use `ready-for-human` when the work itself needs real hardware, external access, or manual testing, and say which. The owner approved the breakdown, so the tickets skip triage and the owner never labels them by hand.
- Write each body in the [ticket layout](#ticket-layout). Make each ticket a sub-issue of the epic for its milestone. Work that belongs to no milestone, such as a repository chore, has no parent and leaves out `## Parent`.
- Publish blockers first, then list them in each ticket's `## Blocked by` section by number. The Blocked by workflow turns the section into native GitHub links.
- When you find work outside your current ticket, do not do it. Open an issue for it with `bug` or `enhancement` and `needs-triage`, never `ready-for-agent`, and mention it in your pull request.
- A question only the owner can answer is an `enhancement` labeled `ready-for-human`, and a sub-issue of the epic for the milestone it blocks. State the question and the options, and end the body with `Answer in a comment, then close this issue as completed. Close it as not planned if the waiting tickets should not happen.` Publish it before the tickets that wait on it, and list it under their `## Blocked by`. Before you open one, search the open issues for the same question.
- Settle any hold the owner wants while they approve the breakdown. Leave a held ticket out, or publish a question such as "Start X?" and list it as the ticket's blocker.

## Ticket layout

An agent that breaks work into tickets writes each body in this layout. Each ticket is a narrow vertical slice that can be checked on its own and fits one fresh context window. Do prefactoring first, in its own ticket.

```markdown
## Parent

- #<epic number>

## What to build

The behavior this ticket makes work.

Read first:

- [<doc>#<anchor>](https://github.com/Tsukidy/deployarr/blob/main/<doc>#<anchor>)

## Acceptance criteria

- [ ] A specific, testable criterion

## Out of scope

- Something that belongs to another ticket

## Blocked by

- #<blocking issue number>
```

Under "Read first", link each doc to the heading of the section the agent needs, such as `docs/architecture.md#agent`, not to the whole file. Link a doc's title heading only when the agent needs all of it. Write each link as a full URL on `main`, as the layout shows. The agent passes the link to `python3 tools/doc_section.py` as written to read that section.

Write `None (can start immediately).` under `## Blocked by` when nothing blocks the ticket. Leave out `## Out of scope` when there is nothing to say. The [Issue tracker](issue-tracker.md#blocking) page explains how the blockers become native links.
