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

Triage posts this as a comment. For `ready-for-human`, use the same layout and add why an agent cannot do it: a judgment call, external access, a design decision, the lab, hardware, or manual testing.

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

Under "Read first", link each doc to the heading of the section the agent needs, such as `docs/architecture.md#agent`, not to the whole file. Link a doc's title heading only when the agent needs all of it. Write each link as a full URL on `main`, as the layout shows. The agent reads the linked section with `python3 tools/doc_section.py docs/architecture.md agent`.

Write `None (can start immediately).` under `## Blocked by` when nothing blocks the ticket. Leave out `## Out of scope` when there is nothing to say. The [Issue tracker](issue-tracker.md#blocking) page explains how the blockers become native links.
