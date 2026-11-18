# Domain docs

This page tells the engineering skills from [mattpocock/skills](https://github.com/mattpocock/skills) where this repository keeps its domain language and its design decisions. It is adapted from the `domain.md` template of that repository's `setup-matt-pocock-skills` skill. See [Third-party notices](third-party-notices.md).

## Layout

This repository has one context, so it has no `CONTEXT-MAP.md`.

| Path | Holds |
| --- | --- |
| `CONTEXT.md` | The glossary of domain terms, with no implementation details |
| `docs/adr/` | One record for each hard-to-reverse decision, with its context and its reason. The first ADR creates the folder. |
| `docs/architecture.md` | The current design as one coherent summary. It stays the source of truth for design. |

An ADR records a decision and its reason as they stood when the decision was made. `docs/architecture.md` describes the design as it is now.

## Before you explore

- Read `CONTEXT.md`.
- Read the sections of `docs/architecture.md` for the area you work in. `python3 tools/doc_section.py docs/architecture.md` prints its outline.
- Read the ADRs in `docs/adr/` that touch that area, if the folder exists.

## Use the glossary's terms

When your output names a domain concept, as in an issue title, a test name, or a refactor proposal, use the term that `CONTEXT.md` defines. Do not use a word from an entry's `_Avoid_` line.

If the concept you need is not in the glossary, you are inventing a word the project does not use, or the glossary has a gap. Reconsider the word, or add the term with the `domain-modeling` skill.

## Record a decision

Write an ADR only when the decision passes all three tests in the `domain-modeling` skill. It is hard to reverse, it would surprise a reader who lacks the context, and it came from a real trade-off. Name it with the next free number in `docs/adr/`, as in `0001-slug.md`.

- In the same commit as the ADR, update the sections of `docs/architecture.md` that the decision changes. [Rules for changes](../../AGENTS.md#rules-for-changes) requires that edit for every design change, with or without an ADR.
- Add the ADR in the pull request that changes `docs/architecture.md`, not in a research page's pull request.
- When a later decision replaces an ADR, write a new ADR and set the old one's status to `superseded by ADR-NNNN`. Do not rewrite the old ADR's decision.
- Several sessions work at once, so two pull requests can take the same number. Before you mark the pull request ready, merge the latest `main`. If another ADR took your number, renumber yours and fix the links to it.

## Flag conflicts

If your output contradicts an ADR or `docs/architecture.md`, say so and name it. Do not override it silently.

> _Contradicts [Directory join](../architecture.md#directory-join), but worth reopening because..._
