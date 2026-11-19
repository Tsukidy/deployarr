# Triage labels

This page maps the canonical triage roles from [mattpocock/skills](https://github.com/mattpocock/skills) to the labels in this repository. The rules are adapted from that repository's `triage` and `to-tickets` skills. See [Third-party notices](third-party-notices.md).

`.github/labels.json` defines every label, and the Labels workflow creates or updates them when that file reaches `main`. Copy label names from that file. To add a label or change its color or description, edit that file. To rename or remove one, open an issue for the owner, because the Labels workflow never renames or deletes.

## State roles

Every issue except an epic carries exactly one state label.

| Label in mattpocock/skills | Label in this repository | Meaning |
| --- | --- | --- |
| `needs-triage` | `needs-triage` | The maintainer needs to evaluate this issue |
| `needs-info` | `needs-info` | Waiting on the reporter for more information |
| `ready-for-agent` | `ready-for-agent` | Fully specified, ready for an AFK agent |
| `ready-for-human` | `ready-for-human` | Requires a human |
| `wontfix` | `wontfix` | Will not be actioned |

`ready-for-human` covers judgment calls, owner decisions, external access, real hardware, and manual testing. The issue says which one applies. Work that needs only the VM lab is `ready-for-agent` with the `lab` label.

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
| `epic` | A roadmap milestone. Each milestone has one, and its sub-issues are the milestone's tickets. An epic carries no category or state label. |
| `area:*` | The part of deployarr an issue touches. An issue can carry several, or none. |
| `bookkeeping` | Upkeep of the docs and agent guides, kept apart from milestone work. The owner uses it to point an agent at this work. It goes with a category and a state, like `area:*`. |
| `lab` | Work that needs the VM lab, which only a session on the lab host can reach. It goes with a category and a state, like `area:*`. The lab host's session has `lab` in its prompt and takes only these tickets. Every other session skips them. See [Frontier](issue-tracker.md#frontier). |

## Who applies each label

- The Triage label workflow adds `needs-triage` to a new issue that has no state label.
- The issue forms add the category. An issue opened blank has none until triage adds it.
- An agent that breaks work into tickets shows the owner the list first. Once the owner approves it, the agent labels each ticket with its category and `ready-for-agent` or `ready-for-human`, and adds `lab` to a ticket that needs the VM lab. The tickets skip triage, and the owner never labels them by hand.
- Work an agent finds outside its ticket gets its category and `needs-triage`, so the owner decides whether it happens.
- Triage applies the category and the state of an issue in `needs-triage`. See [Triage](#triage). The maintainer can override any label at any time. A `ready-for-agent` label applied by hand also needs a `## Blocked by` section at the end of the body, with `None (can start immediately).` when nothing blocks the issue. Without one, the frontier skips it.
- The Close needs-info workflow runs daily. Any comment made after `needs-info` goes on swaps it for `needs-triage`, so add the label after you ask your question. An issue with no activity for 14 days after the label goes on is closed as not planned.
- An agent that needs the owner's answer asks the owner in the session when the owner is there, and records the answer on the issue as an `Owner input:` comment. Otherwise it opens a `ready-for-human` question and lists it as a blocker of its ticket. The ticket keeps its label. When the owner answers and closes the question, the ticket is back on the frontier with no relabel.
- An agent off the lab host adds `lab` to its ticket when the ticket's own work needs the VM lab. An agent moves a ticket to `ready-for-human` only when the ticket's own work needs hardware, external access, or manual testing.
- When the owner names an issue in an agent's prompt for it to work on, that agent works on it and keeps its label. A `ready-for-human` issue stays off the frontier if the session stops early. Breaking an epic into tickets never claims the issue.
- A ticket whose blocker was closed as not planned should not happen. The next session that takes it closes it as not planned with `wontfix`.
- An issue that is reopened keeps its labels and loses its assignee. Whoever reopens a `wontfix` issue swaps the label for `needs-triage` in the same call, and first frees any claim it still carries, as [Release a claim](issue-tracker.md#release-a-claim) says.

## Triage

Triage is for issues the owner or anyone else files by hand. They arrive with `needs-triage`. An agent triages only when the owner asks. The request covers only the issues in `needs-triage` when the agent starts. Issues the agent files during the run stay in `needs-triage`. The agent names every issue that enters `needs-triage` during the run in its final report, including the ones it filed, so the owner can start a new session for them. See [Work list](issue-tracker.md#work-list). The agent applies the category and the state it recommends without waiting for a second go-ahead, and then reports what it applied, so the owner can override any of it. When it cannot choose between two states, it asks the owner with a recommendation before it applies them. Triage never claims or assigns the issue.

- Every triage comment starts with `> *This was generated by AI during triage.*`
- Before you recommend, check whether the request is already implemented, and read `.out-of-scope/` for an earlier rejection of the same idea.
- For a bug, reproduce it before it reaches `ready-for-agent`.
- Apply the labels last, in one call with the category and the state, after the comments and edits below. Applying the state last keeps an issue that triage leaves half done in `needs-triage`.
- For `ready-for-agent` and `ready-for-human`, post the agent brief comment first. Then add a `## Blocked by` section to the end of the body, keeping the reporter's text, and keep the `id` that the `issue_write` call returns. Attach the issue to the epic for its milestone with that `id`. Then apply the labels.
- For `needs-info`, post triage notes first: what is established, and specific questions for the reporter. Then apply the label.
- For `wontfix`, follow the steps in `.out-of-scope/README.md` when an enhancement is rejected. If the work already exists, comment where it lives. Then close the issue as not planned, and swap its state label for `wontfix` in the same `issue_write` call.

## Blocked issues

The upstream skills have no blocked state. A ticket that waits on another ticket keeps its state label and lists its blockers in its `## Blocked by` section. An agent takes it only after every blocker is closed. See [Issue tracker](issue-tracker.md#blocking).
