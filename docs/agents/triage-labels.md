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

## Who applies each label

- The Triage label workflow adds `needs-triage` to a new issue that has no state label.
- The issue forms add the category. An issue opened blank has none until triage adds it.
- An agent that breaks work into tickets shows the owner the list first. Once the owner approves it, the agent labels each ticket with its category and `ready-for-agent` or `ready-for-human`. The tickets skip triage, and the owner never labels them by hand.
- Work an agent finds outside its ticket gets its category and `needs-triage`, so the owner decides whether it happens.
- Triage recommends a category and a state for an issue in `needs-triage`, waits for the owner, then applies both in one call. The maintainer can override any label at any time.
- The Close needs-info workflow runs daily. Any comment made after `needs-info` goes on swaps it for `needs-triage`, so add the label after you ask your question. An issue with no activity for 14 days after the label goes on is closed as not planned.
- An agent that needs the owner's answer opens a `ready-for-human` question and lists it as a blocker of its ticket. The ticket keeps `ready-for-agent`. When the owner closes the question, the ticket is back on the frontier with no relabel.
- An agent moves a ticket to `ready-for-human` only when the ticket's own work needs the lab, hardware, external access, or manual testing.
- When the owner names a `ready-for-human` issue in an agent's prompt, that agent moves it to `ready-for-agent` as it claims it.

## Blocked issues

The upstream skills have no blocked state. A ticket that waits on another ticket keeps its state label and lists its blockers in its `## Blocked by` section. An agent takes it only after every blocker is closed. See [Issue tracker](issue-tracker.md#blocking).
