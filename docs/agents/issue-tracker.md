# Issue tracker

Issues for this repository live in GitHub Issues on `Tsukidy/deployarr`. This page is adapted from the GitHub tracker template in [mattpocock/skills](https://github.com/mattpocock/skills). See [Third-party notices](third-party-notices.md).

File issues only into `Tsukidy/deployarr`.

## Tools

Cloud sessions use the GitHub MCP tools, because policy there allows the `gh` command only for search. A local session can use `gh`. Each operation below lists both.

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

- MCP: `issue_write` with `method: update`, `state: closed`, and `state_reason` set to `completed`, `not_planned`, or `duplicate`.
- `gh`: `gh issue close <number> --reason "not planned"`. On an issue that is already closed, `gh issue close --comment` drops the comment and still succeeds.

A pull request that says `Closes #<number>` closes its issue as completed when it merges. Use it only when the merge completes the issue. Otherwise write `Refs #<number>`. A `wontfix` issue is closed by hand as not planned, never by a pull request.

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

To find it, list the open `ready-for-agent` issues. Check the unassigned ones first, because an assigned issue is almost always held. For each candidate, read its body for blockers and its comments for a holder. Take the first free one in its epic's sub-issue order.

A `ready-for-agent` issue with no `## Blocked by` section did not finish triage. Skip it.

## Claim

Claim comments decide which session holds a ticket. Agents in this repository share one GitHub login, so the assignee cannot tell sessions apart, and no step compares clocks. The assignee only shows in GitHub that the ticket is taken.

To find the holder, read every comment in order, starting with nobody holding the ticket:

- `Claimed by branch <branch>` gives the ticket to that branch, but only when nobody holds it.
- `Claim released by branch <branch>` frees the ticket when that branch holds it.
- Any other comment, including `Claim withdrawn by branch <branch>`, changes nothing.

Read comments with `issue_read`, `method: get_comments`, and `perPage: 100`. If a page is full, read the next one.

To claim a ticket:

1. Read every comment. If a session holds the ticket, pick another one.
2. Comment `Claimed by branch <branch>`.
3. Read every comment again. If you do not hold the ticket now, another session got there first. Comment `Claim withdrawn by branch <branch>`, leave the assignee alone, and pick another ticket.
4. Assign yourself. With MCP, call `get_me` for your login, then `issue_write` with `method: update` and `assignees`. With `gh`, run `gh issue edit <number> --add-assignee @me`.

Claiming is the first write of a session that works a ticket. The one exception is an `Owner input:` comment, which may come first. A session that triages an issue or breaks an epic into tickets does not claim it.

`tools/claim_model.py` checks every interleaving of these steps for two and three sessions, with crashes and the owner freeing stale claims. It finds no state where two sessions hold a ticket, and none where a ticket stays held after every session released it. The Tools workflow runs it on every pull request. Change the model in the same pull request as these steps.

### Release a claim

Release your claim when you stop work on a ticket without finishing it.

1. Comment `Claim released by branch <branch>`. This frees the ticket.
2. Remove yourself as assignee. With MCP, call `issue_write` with `method: update` and `assignees: []`, in the same call as any label change. With `gh`, run `gh issue edit <number> --remove-assignee @me`.

A ticket that merges or closes needs no release.

### Stale claims

A session that ends without releasing its claim leaves the ticket held, and off the frontier. Only the owner can tell that the session is gone. The owner frees the ticket in one of two ways:

- Comment `Claim released by branch <branch>` with the dead session's branch, then unassign the ticket.
- Name the issue and that branch in a new session's prompt. That session posts the same comment and then claims the ticket as usual.

## Pull requests as a triage surface

No. Pull requests in this repository come from the owner's agents, so triage covers issues only.

## When a skill says "publish to the issue tracker"

Create a GitHub issue.

## When a skill says "fetch the relevant ticket"

Read it as in [Read an issue](#read-an-issue).
