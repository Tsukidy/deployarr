# Research pages that follow later decisions

A research page keeps the design changes it suggested when it was written, and a later design decision does not edit them.

## Why this is out of scope

- A research page records one source at one version. Its last section records the design changes that the source suggested when the page was written.
- `docs/architecture.md` is the source of truth for design. It records each later decision and the reason for it, so a reader who needs the current design reads it there.
- `AGENTS.md` allows only link edits to another source's page. That rule lets research from parallel branches merge without conflicts.
- A page that followed every later decision would need an edit each time the design moves. It would also lose the record of what the source suggested.

## Prior requests

- #131: "The MDT 8450 binaries page still gives the empty success list to the compiler"
