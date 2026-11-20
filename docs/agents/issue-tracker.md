# Issue tracker

Issues for this repository live in GitHub Issues on `Tsukidy/deployarr`. This page is adapted from the GitHub tracker template in [mattpocock/skills](https://github.com/mattpocock/skills). See [Third-party notices](third-party-notices.md).

File issues only into `Tsukidy/deployarr`.

## Tools

Cloud sessions use the GitHub MCP tools, because policy there allows the `gh` command only for search. A local session can use `gh`. Each operation below lists both.

In a cloud session, a `gh` command that uses GraphQL returns HTTP 403. This includes `gh search issues` and `gh issue list`. Search through the REST API with `gh api` instead, as in this command:

```sh
gh api -X GET search/issues -f q='repo:Tsukidy/deployarr is:issue <words>'
```

- The query must include `is:issue` or `is:pull-request`. Without either, the endpoint returns HTTP 422.
- Keep `-X GET`. With `-f` fields and no `-X`, `gh api` sends a POST. The session refuses that POST with HTTP 403.

## Operations

### Create an issue

- MCP: `issue_write` with `method: create`, `title`, `body`, and `labels`. Add `parent_issue_number` to create it as a sub-issue.
- `gh`: `gh issue create --title "..." --body "..." --label "..."`. With `gh` 2.94 or later, add `--parent <number>` for a sub-issue. With an older `gh`, create the issue first and attach it as in [Make an issue a sub-issue](#make-an-issue-a-sub-issue).

Pass the labels in the create call. The Triage label workflow adds `needs-triage` to an issue that has no state label when it opens.

### Read an issue

- MCP: `issue_read` with `method: get`, then `method: get_comments`. Read the comments too, because a comment can already say the work is done.
- `gh`: `gh issue view <number> --json number,title,body,labels,assignees,comments`.

### List issues

- MCP: `list_issues` with `state: OPEN` and a `labels` filter.
- `gh`: `gh issue list --state open --label "..." --json number,title,labels,assignees`.

### Make an issue a sub-issue

- At creation, pass the parent as `parent_issue_number` (MCP) or `--parent` (`gh` 2.94 or later).
- For an existing issue with MCP, call `sub_issue_write` with `method: add`, the parent's number as `issue_number`, and the child's database `id` as `sub_issue_id`, as a number. Take the `id` from the result of an `issue_write` call on the child, which returns `{"id": ..., "url": ...}`. `issue_read` does not return it, and the `id` under `user` belongs to the author. The `id` is not the issue number.
- For an existing issue with `gh` 2.94 or later, run `gh issue edit <parent> --add-sub-issue <child>`. With an older `gh`, get the child's database id with `gh api repos/Tsukidy/deployarr/issues/<child> --jq .id`, then run `gh api --method POST repos/Tsukidy/deployarr/issues/<parent>/sub_issues -F sub_issue_id=<id>`.

### Apply or remove labels

- MCP: `issue_write` with `method: update` and `labels`. This replaces every label on the issue. Read the issue first, then send the full list: the labels it keeps, plus the one you add or minus the one you remove.
- `gh`: `gh issue edit <number> --add-label "..."` or `--remove-label "..."`.

When you change the state, remove the old state label in the same call. An issue never carries two state labels.

### Comment on an issue

- MCP: `add_issue_comment`.
- `gh`: `gh issue comment <number> --body "..."`.

### Edit an issue body

- MCP: `issue_write` with `method: update` and the whole new `body`. Read the issue first and keep the text you do not change.
- `gh`: `gh issue edit <number> --body "..."`.

### Close an issue

Post the explanation as a comment first. Then close it.

- MCP: `issue_write` with `method: update`, `state: closed`, and `state_reason` set to `completed`, `not_planned`, or `duplicate`. With `duplicate`, also set `duplicate_of` to the number of the kept issue.
- `gh`: `gh issue close <number> --reason "not planned"`, or `gh issue close <number> --duplicate-of <kept number>` for a duplicate. On an issue that is already closed, `gh issue close --comment` drops the comment and still succeeds.

A pull request that says `Closes #<number>` closes its issue as completed when it merges. Use it only when the merge completes the issue. Otherwise write `Refs #<number>`. A `wontfix` issue is closed by hand, as not planned, as a duplicate, or as completed, never by a pull request.

## Blocking

A ticket lists the tickets it waits on in a `## Blocked by` section of its body, one `- #<number>` line each, or `None (can start immediately).` The section is the source of truth.

The GitHub MCP tools cannot create GitHub's native blocking links. The Blocked by workflow does it instead. When an issue is opened, edited, or reopened, the workflow reads the section and makes the issue's native "blocked by" links match it. It adds missing links and removes links the section no longer lists. An issue with no `## Blocked by` section keeps its links unchanged.

To change a ticket's blockers, edit its section. A link added in the GitHub UI disappears at the next edit of the body unless the section lists it.

Publish blockers before the tickets they block, so each `## Blocked by` section can name real issue numbers. Never write a placeholder such as `#<n>` into an issue, because GitHub links it to whatever issue has that number.

A ticket is unblocked when every issue in its section is closed.

## Frontier

The frontier is the set of tickets an agent can start now. A ticket is on the frontier when it is:

- open
- labeled `ready-for-agent`
- written with a `## Blocked by` section
- unblocked, because every issue in that section is closed
- held by no session, by the rule in [Claim](#claim)

To find it, list the open `ready-for-agent` issues. Check the unassigned ones first, because an assigned issue is almost always held. For each candidate, read its body for blockers and its comments for a holder. Take the first free one in its epic's sub-issue order. Take tickets that have no epic in issue number order.

When your prompt names a label, such as `bookkeeping`, the frontier holds only the tickets that carry it.

A ticket labeled `lab` needs the VM lab. It is on the frontier only for the session on the lab host, whose prompt names `lab`. Every other session skips it.

A `ready-for-agent` issue with no `## Blocked by` section did not finish triage. Skip it.

## Claim

Claim comments decide which session holds a ticket. Agents in this repository share one GitHub login, so the assignee cannot tell sessions apart, and no step compares clocks. The assignee only shows in GitHub that the ticket is taken.

To find the holder, read every comment in order, starting with nobody holding the ticket:

- `Claimed by branch <branch>` gives the ticket to that branch, but only when nobody holds it.
- `Claim released by branch <branch>` frees the ticket when that branch holds it.
- Any other comment, including `Claim withdrawn by branch <branch>`, changes nothing.

Read comments with `issue_read`, `method: get_comments`, and `perPage: 100`. If a page is full, read the next one.

Never edit or delete a `Claimed by branch`, `Claim released by branch`, or `Claim withdrawn by branch` comment, and never edit another comment into one. The holder rule replays them in order, so an edit or a deletion can hand the ticket to a branch that is not working it.

To claim a ticket:

1. Read every comment. If a session holds the ticket, pick another one.
2. Comment `Claimed by branch <branch>`.
3. Read every comment again. If you do not hold the ticket now, another session got there first. Comment `Claim withdrawn by branch <branch>`, leave the assignee alone, and pick another ticket.
4. Read the issue again with `issue_read`, `method: get`. If it is closed, is no longer labeled `ready-for-agent`, gained or lost the `lab` label, or an issue in its `## Blocked by` section is open, it changed after you found it. Remove the assignee, then comment `Claim released by branch <branch>`, and pick another ticket. You hold the ticket at this point, so any assignee is left over from a session that is gone. Skip the label test for an issue that your prompt names. A session whose prompt names a wayfinder map skips only the `ready-for-agent` part of that test for the ticket it takes from the map's frontier, as [Work through a map](#work-through-a-map) says.
5. Assign yourself. With MCP, call `get_me` for your login, then `issue_write` with `method: update` and `assignees`. With `gh`, run `gh issue edit <number> --add-assignee @me`.

For an issue that your prompt names, each step that says to pick another ticket means stop and tell the owner instead.

`tools/claim_model.py` checks every interleaving of these steps, the release steps below, and the hand-off steps under [Take a ticket](#take-a-ticket): not planned, waiting on a question, ready-for-human, and the hand-off to the lab. It covers two and three sessions, crashes, an owner who answers questions and frees stale claims, a session named in a prompt that frees a gone branch, a session off the lab host that takes a `ready-for-agent` or `ready-for-human` ticket from a wayfinder map's frontier, and a session on the lab host beside sessions that are not. It finds no state where two sessions hold a ticket, where a session works a ticket it should not, or where a ticket stays held or assigned after every session released it. It assumes the claim comments are never edited or deleted, and it does not cover reopening an issue or a pull request that waits for review. The Tools workflow runs it on every pull request. Change the model in the same pull request as any of these steps.

### Release a claim

Release your claim when you stop work on a ticket without finishing it. The release puts the ticket back on the frontier for the next session, unless you first take it off.

1. If the ticket should not return to the frontier, make that change first: a new open issue in its `## Blocked by` section, a new state label, or a close. Otherwise skip this step.
2. Remove yourself as assignee. With MCP, call `issue_write` with `method: update` and `assignees: []`, in the same call as a label change or a close. With `gh`, run `gh issue edit <number> --remove-assignee @me`.
3. Comment `Claim released by branch <branch>`. This frees the ticket.

A ticket closed by a merged pull request needs no release. Whoever reopens an issue first reads its comments and frees any claim it still carries by commenting `Claim released by branch <branch>` for the holder. Then they reopen it with no assignee. With MCP, call `issue_write` with `method: update`, `state: open`, and `assignees: []`. With `gh`, remove the assignee while the issue is still closed, then run `gh issue reopen <number>`. Never unassign after the reopen, because a session may already have claimed it.

### Stale claims

A session that ends without releasing its claim leaves the ticket held, and off the frontier. Only the owner can tell that the session is gone. A session whose `Closes` pull request is still open is not gone. Close that pull request without merging it before you free its claim, or its later merge closes a ticket that another session is working. A held ticket can also be unassigned, when a session stopped between its claim comment and its assignment. To find stale claims, read the comments of the open `ready-for-agent` tickets, not only the assigned ones. Also read the comments of every open sub-issue of each open `wayfinder:map` issue, whatever its state label, because a session whose prompt names a map can hold a ticket in any state.

The owner frees a stale claim in one of two ways:

- Unassign the ticket, then comment `Claim released by branch <branch>` with the gone session's branch.
- Name the issue and that branch in a new session's prompt. That session posts the same comment and then claims the ticket as usual.

Do not delete the gone session's claim comment. A deletion can hand the ticket to an earlier session that withdrew.

## Take a ticket

Claiming is the first write of a session that works a ticket. Two comments may come before it: an `Owner input:` comment, and the `Claim released by branch <branch>` comment that frees a branch the prompt names as gone. A session that charts a wayfinder map creates the map and its tickets without a claim, and claims none of them, as [Chart a map](#chart-a-map) says.

- If your prompt names an issue for you to work on, work on that issue, whatever its labels, and keep its state label. A prompt that asks you to triage an issue or break an epic into tickets is not a request to work on it, and you do not claim it. A prompt that names a wayfinder map is not a request to work on the map, and you do not claim it. Follow [Work through a map](#work-through-a-map) instead.
- Never work on an issue in `needs-triage` without the owner's direct approval of that issue. This holds for issues that you or your subagents filed. Direct approval means the owner names the issue in your prompt, or approves that issue by number in the session. An answer about the same work that the owner gave before you saw the issue does not count. Treat an issue that the owner approves in the session like an issue your prompt names. Never relabel a `needs-triage` issue so that you can take it. Only triage that the owner asked for changes its state.
- If the prompt gives input, such as a decision or lab results, first post it on the issue as a comment that starts with `Owner input:`, so a later session can find it.
- Claim a named issue with the steps in [Claim](#claim). If another branch holds it and the prompt names that branch as gone, release that branch's claim as described under [Stale claims](#stale-claims) first.
- Otherwise, take the first ticket on the [frontier](#frontier). Never take an issue in any other state.
- Claim it with the steps in [Claim](#claim).
- Before you start, read each issue in the `## Blocked by` section with its comments. Follow any answer the owner gave there, and read the ticket's comments for `Owner input:` and for partial work that an earlier session pushed.
- If a blocker was closed as not planned, the ticket should not happen. Comment `Not planned, because #<blocker> was closed as not planned.` Then close the ticket as not planned, swap its state label for `wontfix`, and clear the assignee, all in one `issue_write` call. Then comment `Claim released by branch <branch>`. If a blocker was closed as a duplicate, follow the duplicates to the first issue that was not closed as one, and replace the blocker with it in one body edit. Then check that issue like any other blocker. If it is open, [release your claim](#release-a-claim). If it was closed as not planned, close the ticket as above. A ticket on a wayfinder map with an `Owner input:` comment that says it should not happen is the exception. Rule it out as the ticket you hold, as [Rule a ticket out of scope](#rule-a-ticket-out-of-scope) says, in place of closing it as above.
- If you need an answer from the owner and the owner is in the session, ask them, with your recommended option first. Post the answer on the ticket as a comment that starts with `Owner input:`, then carry on. You keep the ticket.
- If you need an answer from the owner and the owner is not in the session, as in a scheduled run, open a question as described under [Write tickets](agent-brief.md#write-tickets). In one body edit, add it to the ticket's `## Blocked by` section. Comment `Waiting on #<question>. Partial work: branch <branch>, pushed`, or `not pushed`. Then [release your claim](#release-a-claim). The ticket keeps its label and returns to the frontier when the owner closes the question.
- If the ticket's own work turns out to need the VM lab, and your prompt does not name `lab`, first comment on what the session on the lab host must do. Then add `lab` and clear the assignee in one `issue_write` call. Then comment `Claim released by branch <branch>`.
- If the ticket's own work turns out to need hardware, external access, or manual testing, first comment on what a human must do. Then swap `ready-for-agent` for `ready-for-human` and clear the assignee in one `issue_write` call. Then comment `Claim released by branch <branch>`.
- In the pull request, link the issue as [Close an issue](#close-an-issue) says.

## Work list

A session works on a fixed list of issues, set when it starts.

- At the start, list the issues you will work on, and tell the owner. For a named issue or one ticket, the list is that issue. For a run over the frontier or over `needs-triage`, it is the issues there when you start. A run over the frontier takes tickets on a wayfinder map within the limit in [Work through a map](#work-through-a-map). Claim comments mark the tickets on it.
- Finish the issues on your list. Review fixes and merge conflicts on your own pull requests are part of that work. An issue you file does not join your list, so a triage run leaves the issues it files in `needs-triage`.
- Do not move on to an issue that another session opens while you work, or to a ticket that becomes unblocked after you start, without the owner's permission. You may ask.
- When your list is done, report and stop. A run over the frontier or over `needs-triage` first lists it once more and changes nothing there. Its report names the issues that joined it during the run, including the ones the run filed, and tells the owner to start a new session for them.

## Branches in a cloud session

A cloud session's system prompt names one branch to push to, such as `claude/sleepy-hypatia-g8fonu`, and allows other branches only with the owner's explicit permission. The owner gives that permission here, so do not ask for it.

- Work on one ticket, or work with no ticket, goes on the named branch.
- A session that works more than one ticket, itself or through subagents, pushes one branch per ticket. Name it after the named branch and the issue number, such as `claude/sleepy-hypatia-g8fonu-issue-159`. Open one draft pull request from each branch.
- Claim each ticket with its own branch name, as in `Claimed by branch claude/sleepy-hypatia-g8fonu-issue-159`.
- Never push to `main`. Push to a branch that another session made only when the owner asks.

## Pull requests as a triage surface

No. Pull requests in this repository come from the owner's agents, so triage covers issues only.

## When a skill says "publish to the issue tracker"

Create a GitHub issue.

## When a skill says "fetch the relevant ticket"

Read it as in [Read an issue](#read-an-issue).

## Wayfinding operations

The `wayfinder` skill in the `mattpocock-skills` plugin reads this section. A person starts it by typing `/wayfinder`. The skill charts an effort too big for one session as a map of decision tickets, then resolves the tickets one at a time. This section expresses its operations through the rules on this page. Where the skill and this page differ, follow this page.

### Maps and their tickets

- A map is one issue labeled `wayfinder:map`. Like an epic, it carries no category or state label, and the Triage label workflow leaves it without `needs-triage`.
- A map has no parent. Its body has the sections the skill gives it: Destination, Notes, Decisions so far, Not yet specified, and Out of scope. Its Notes name the milestone it serves, if any.
- Each ticket on a map is a sub-issue of the map. It carries exactly one category label, exactly one state label, and exactly one `wayfinder:<type>` label from the table below. The type sets the state a ticket starts with.
- Blocking follows [Blocking](#blocking). Publishing each blocker before the tickets it blocks, as that section says, takes the place of the skill's second wiring pass.
- Other sessions edit the same map and tickets. Read an issue again right before you edit its body, then edit it as [Edit an issue body](#edit-an-issue-body) says.

| Type label | State label |
| --- | --- |
| `wayfinder:research` | `ready-for-agent` |
| `wayfinder:prototype` | `ready-for-human` |
| `wayfinder:grilling` | `ready-for-human` |
| `wayfinder:task` | `ready-for-agent` when an agent can do it alone, otherwise `ready-for-human` |

A ticket's body follows this layout, in place of the skill's ticket body. The link under "Read first" leads a session that takes the ticket from the general frontier to this section.

```markdown
## Parent

- #<map number>

## Question

The decision or investigation this ticket resolves.

Read first:

- [docs/agents/issue-tracker.md#wayfinding-operations](https://github.com/Tsukidy/deployarr/blob/main/docs/agents/issue-tracker.md#wayfinding-operations)

## Blocked by

- #<blocking issue number>
```

### Chart a map

- Before you create the map or any ticket, show the owner the map body and the numbered list of tickets, with each ticket's type, state, and blockers. Create them only after the owner approves the list in the session, as [Write tickets](agent-brief.md#write-tickets) says.
- Create the map with the `wayfinder:map` label. Then create its tickets as sub-issues of the map, blockers first, as in [Create an issue](#create-an-issue).
- The tickets you create do not join your [work list](#work-list), as Work list says for every issue you file. They wait on the [frontier](#frontier) for a later session. Your report names them and tells the owner to start a new session for them.

### Work through a map

- A map's `ready-for-agent` tickets are on the general [frontier](#frontier) like any ticket. They have no epic, so they take issue number order there.
- The map's frontier is its open sub-issues that have a `## Blocked by` section, are unblocked, and are held by no session under [Claim](#claim), whatever their state label, in the map's sub-issue order. The rule in [Frontier](#frontier) for the `lab` label applies to it too. The session on the lab host takes only the tickets labeled `lab` from it, and every other session skips them. List the sub-issues with `issue_read`, `method: get_sub_issues`, and `perPage: 100`, on the map. If a page is full, read the next one. With `gh`, run `gh api repos/Tsukidy/deployarr/issues/<map>/sub_issues --paginate`.
- A session whose prompt names a map takes the first ticket on the map's frontier, unless the prompt names a ticket. It treats that ticket as an issue its prompt names, for [Take a ticket](#take-a-ticket) and for [Claim](#claim). So it keeps the ticket's state label, and it stops and tells the owner if it loses the claim. In Claim step 4 it skips only the `ready-for-agent` part of the label test. If the ticket gained or lost the `lab` label, it releases the ticket as that step says, then stops and tells the owner. A ticket that the prompt names directly skips the whole label test, as Claim says. The claim comments and the assignment in Claim take the place of the skill's "assign it first" step.
- A `needs-triage` ticket still needs the owner's direct approval, as [Take a ticket](#take-a-ticket) says. Naming the map is not direct approval of a ticket on it. A `needs-info` ticket needs the same approval, because the Close needs-info workflow swaps its label for `needs-triage` after your claim comment, as [Who applies each label](triage-labels.md#who-applies-each-label) says. Before you claim a `needs-triage` or `needs-info` ticket from the map's frontier, ask the owner in the session to approve it by number. Without that approval, stop and tell the owner.
- A session resolves at most one map ticket that is not a research ticket, and any number of research tickets, as the skill says. So a run over the frontier puts any number of research tickets on its [work list](#work-list), but at most one other map ticket, the first in issue number order. Its report names the other map tickets it left off and tells the owner to start a new session for them.

### Resolve a map ticket

These steps apply to every session that resolves a ticket on a map, including a session that took it from the general frontier without running `/wayfinder`.

1. Record the owner's answers as `Owner input:` comments on the ticket, as [Take a ticket](#take-a-ticket) says.
2. Post the answer as a resolution comment on the ticket. Link any asset, such as a prototype, a research page, or a pull request.
3. Make any change to the repository through a pull request from the branch that [Branches in a cloud session](#branches-in-a-cloud-session) gives you, in place of the skill's `research/<name>` branch.
   - A research ticket that reads a source writes its findings on that source's page under `docs/research/`, as [Rules for changes](../../AGENTS.md#rules-for-changes) says. Its pull request leaves `docs/architecture.md` alone and closes the ticket. The design changes that the page ends with follow in a separate pull request, for the map ticket that decides them or for a new ticket that [New map tickets](#new-map-tickets) adds.
   - Any other ticket whose decision changes the design or the plan updates `docs/architecture.md` or `docs/roadmap.md` in its pull request, as Rules for changes requires. An ADR, if the decision needs one, goes in the same pull request, as [Record a decision](domain.md#record-a-decision) says.
4. Close the ticket as completed. When the resolution goes through a pull request, the pull request closes it instead, with `Closes #<number>`.
5. Add one line for the ticket to the map's Decisions so far, as `- [<ticket title>](<ticket link>): <one-line gist of the answer>`. Keep the rest of the map body.

When the resolution goes through a pull request, do steps 2 and 5 when you mark the pull request ready for review.

### Rule a ticket out of scope

- To rule out the ticket you hold, comment why it sits past the destination. Then close it as not planned with `wontfix` and release your claim, as [Take a ticket](#take-a-ticket) does for a ticket that is not planned.
- To rule out another ticket on the map, ask the owner in the session. If the owner agrees, post that answer on the ticket as an `Owner input:` comment. Then claim the ticket as in [Claim](#claim), with its own branch name as [Branches in a cloud session](#branches-in-a-cloud-session) says, and treat it as an issue the owner approved in the session. Once you hold it, rule it out as the ticket you hold. If you cannot claim it, because another session holds it or a blocker is open, stop and tell the owner. A session that later takes a ticket with such an `Owner input:` comment rules it out as the ticket it holds. With no owner in the session, name the ticket in your resolution comment instead.
- Before you close a ticket to rule it out, find the open issues that list it under `## Blocked by`. Read the map's other open tickets. An issue outside the map can wait on a map ticket too, so also search as in [Tools](#tools), with `is:open` and the ticket's number in place of `<words>`. Read the `## Blocked by` section of each result.
- For each map ticket that lists it, remove that line in a body edit, and write `None (can start immediately).` if no line is left. Keep the line only when the owner agrees in the session that the waiting ticket should not happen either. Post that answer on the waiting ticket as an `Owner input:` comment, and name that ticket in your comment. Once you close the ticket you rule out, choose one of two paths for the waiting ticket. Claim the waiting ticket, and rule it out as the bullet above on ruling out another ticket says. Or leave the waiting ticket to the next session that takes it. That session rules it out as the ticket it holds, with the search for waiting issues, the blocker line rules, and the Out of scope line, in place of the not-planned close in [Take a ticket](#take-a-ticket). With no owner in the session, choose one of two paths. Remove the line, and name the waiting ticket in your comment. Or leave the ticket you rule out open, and get the owner's answer as [Take a ticket](#take-a-ticket) says for an owner who is not in the session.
- For an issue outside the map that lists it, ask the owner in the session whether to remove the line, keep it, or leave the ticket open. Post the answer on that issue as an `Owner input:` comment, then do what the owner said. With no owner in the session, do not close the ticket. Get the owner's answer as [Take a ticket](#take-a-ticket) says for an owner who is not in the session.
- Add one line for each ticket you rule out to the map's Out of scope section, with the gist, the reason, and a link to the ticket. It gets no line in Decisions so far.
- A ticket that a decision makes moot follows the same steps, without the line on the map. Its comment links the ticket whose decision makes it moot, in place of why it sits past the destination. This takes the place of the skill's delete.

### New map tickets

- New tickets, including fog that a resolution makes specifiable, follow [Chart a map](#chart-a-map). The owner approves the numbered list in the session before you create them.
- With no owner in the session, write the tickets you would add into the map's Not yet specified section instead, for the next session with the owner.
- When the owner is not in the session, a question for the owner can block the ticket you hold. That question is not a new map ticket. The two bullets above do not apply to it. Open it as [Take a ticket](#take-a-ticket) says. Make it a sub-issue of the epic for the milestone that the map's Notes name, as [Write tickets](agent-brief.md#write-tickets) says. When the map serves no milestone, the question has no parent.
