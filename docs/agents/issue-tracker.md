# Issue tracker

Issues for this repository live in GitHub Issues on `Tsukidy/deployarr`. This page is adapted from the GitHub tracker template in [mattpocock/skills](https://github.com/mattpocock/skills). See [Third-party notices](third-party-notices.md).

File issues only into `Tsukidy/deployarr`.

## Tools

Cloud sessions use the GitHub MCP tools, because policy there allows the `gh` command only for search. A local session can use `gh`. Each operation below lists both.

## Operations

### Create an issue

- MCP: `issue_write` with `method: create`, `title`, `body`, and `labels`. Add `parent_issue_number` to create it as a sub-issue.
- `gh`: `gh issue create --title "..." --body "..." --label "..."`, with `--parent <number>` for a sub-issue.

### Read an issue

- MCP: `issue_read` with `method: get`, then `method: get_comments`. Read the comments too, because a comment can already say the work is done.
- `gh`: `gh issue view <number> --json number,title,body,labels,assignees,comments`.

### List issues

- MCP: `list_issues` with `state: OPEN` and a `labels` filter.
- `gh`: `gh issue list --state open --label "..." --json number,title,labels,assignees`.

### Make an issue a sub-issue

- At creation, pass the parent as `parent_issue_number` (MCP) or `--parent` (`gh`).
- For an existing issue, call `sub_issue_write` with `method: add`, the parent's number as `issue_number`, and the child's `id` from `issue_read` as `sub_issue_id`. The `id` is not the issue number.

### Apply or remove labels

- MCP: `issue_write` with `method: update` and `labels`. This replaces every label on the issue. Read the issue first, then send the full list: the labels it keeps, plus the one you add or minus the one you remove.
- `gh`: `gh issue edit <number> --add-label "..."` or `--remove-label "..."`.

When you change the state, remove the old state label in the same call. An issue never carries two state labels.

### Comment on an issue

- MCP: `add_issue_comment`.
- `gh`: `gh issue comment <number> --body "..."`.

### Close an issue

Post the explanation as a comment first. Then close it.

- MCP: `issue_write` with `method: update`, `state: closed`, and `state_reason` set to `completed`, `not_planned`, or `duplicate`.
- `gh`: `gh issue close <number> --reason "not planned"`. On an issue that is already closed, `gh issue close --comment` drops the comment and still succeeds.

A pull request that says `Closes #<number>` closes its issue when it merges.

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
- not assigned to anyone
- unblocked, because every issue in its `## Blocked by` section is closed

To find it, list the open `ready-for-agent` issues, drop the assigned ones, then read each remaining issue and check its blockers. Take the first one in its epic's sub-issue order.

## Claim

Claiming is the first write of a session, before any other work.

1. Assign the issue to yourself. With MCP, call `get_me` for your login, then `issue_write` with `method: update` and `assignees`. With `gh`, run `gh issue edit <number> --add-assignee @me`.
2. Comment `Claimed by branch <branch>`.
3. Read the comments again. Agents in this repository share one GitHub login, so the assignee cannot tell two sessions apart. If an earlier claim comment names another branch, comment `Claim withdrawn` and pick another ticket. Leave the assignee as it is, because it now marks the other session's claim.

An open, unassigned issue is unclaimed.

## Pull requests as a triage surface

No. Pull requests in this repository come from the owner's agents, so triage covers issues only.

## When a skill says "publish to the issue tracker"

Create a GitHub issue.

## When a skill says "fetch the relevant ticket"

Read it as in [Read an issue](#read-an-issue).
