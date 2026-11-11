# Triage labels

This page maps the canonical triage roles from [mattpocock/skills](https://github.com/mattpocock/skills) to the labels in this repository. The rules are adapted from that repository's `triage` and `to-tickets` skills. See [Third-party notices](third-party-notices.md).

`.github/labels.json` defines every label, and the Labels workflow creates or updates them when that file reaches `main`.

## State roles

Every issue except an epic carries exactly one state label.

| Label in mattpocock/skills | Label in this repository | Meaning |
| --- | --- | --- |
| `needs-triage` | `needs-triage` | The maintainer needs to evaluate this issue |
| `needs-info` | `needs-info` | Waiting on the reporter for more information |
| `ready-for-agent` | `ready-for-agent` | Fully specified, ready for an AFK agent |
| `ready-for-human` | `ready-for-human` | Requires a human |
| `wontfix` | `wontfix` | Will not be actioned |

`ready-for-human` covers judgment calls, owner decisions, external access, the VM lab, real hardware, and manual testing. The issue says which one applies.

## Category roles

Every issue except an epic carries exactly one category label.

| Label | Meaning |
| --- | --- |
| `bug` | Something is broken |
| `enhancement` | A new feature or an improvement |

## Labels outside the triage roles

These labels belong to this repository. The upstream skills do not know them.

| Label | Meaning |
| --- | --- |
| `epic` | A roadmap milestone. Its sub-issues are the tickets. An epic carries no category or state label. |
| `area:*` | The part of deployarr an issue touches. An issue can carry several, or none. |

## Who applies each state

- The Triage label workflow adds `needs-triage` to a new issue that has no state label.
- An agent that writes tickets labels each one `ready-for-agent` or `ready-for-human` itself. Its tickets are specified by construction, so they skip triage. The owner never has to add `ready-for-agent` by hand.
- Triage moves an issue out of `needs-triage` to `needs-info`, `ready-for-agent`, `ready-for-human`, or `wontfix`. The maintainer can override any state at any time.
- The Close needs-info workflow runs daily. Any comment made after `needs-info` goes on swaps it for `needs-triage`, so add the label after you ask your question. An issue with no activity for 14 days after the label goes on is closed as not planned.
- An agent that gets stuck on a `ready-for-agent` issue moves it to `ready-for-human` and comments what it needs.

## Blocked issues

The upstream skills have no blocked state. A ticket that waits on another ticket keeps its state label and lists its blockers in its `## Blocked by` section. An agent takes it only after every blocker is closed. See [Issue tracker](issue-tracker.md#blocking).
