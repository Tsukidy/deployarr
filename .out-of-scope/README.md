# Out of scope

This folder records enhancements that triage rejected, so the same request is not argued again. The format is adapted from the `triage` skill in [mattpocock/skills](https://github.com/mattpocock/skills). See [Third-party notices](../docs/agents/third-party-notices.md).

Work that is planned for later is not here. The roadmap lists it under "After the first release", and the architecture lists it under "Not in the first release".

## One page per concept

Each page covers one concept, not one issue. Name it in short kebab-case, such as `wireless-deployment.md`, so the name says what was rejected. Several issues that ask for the same thing share one page.

A page has these parts:

- a `# Concept name` heading and one sentence that states the decision
- `## Why this is out of scope`, with a durable reason: the project's scope, a technical constraint, or a design decision. "Not now" is a deferral, not a rejection, and does not belong here.
- `## Prior requests`, a list of the issues that asked for it, as `- #<number>: "<title>"`

## When triage reads it

Triage reads every page here before it recommends a state for an enhancement. It matches by concept, not by keyword. On a match, it quotes the earlier reason and asks the owner whether it still holds.

- If it holds, triage adds the issue to the page's prior requests and closes the issue as `wontfix`.
- If the owner changes their mind, triage deletes or updates the page, and the issue goes through normal triage.

## When triage writes it

Only when an enhancement is rejected as `wontfix`. A bug that is rejected, or a request that is already implemented, gets no page. The closing comment for an already implemented request points to where the feature lives.

Agents do not commit to `main`, so a page arrives through a pull request:

1. Add or update the page on a branch, and open a pull request that says `Refs #<number>`. `Closes` would mark the issue completed.
2. Post the closing comment on the issue. Give the page's path and link the pull request. A link to the branch breaks when GitHub deletes the branch after the merge.
3. Close the issue as not planned, and swap its state label for `wontfix` in the same `issue_write` call. Do not wait for the pull request to merge.
4. If the owner closes the pull request without merging it, reopen the issue and swap `wontfix` for `needs-triage` in the same call.
