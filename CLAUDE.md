@AGENTS.md

# Claude Code cloud sessions

These notes cover cloud sessions on the `env` environment. `AGENTS.md` above covers the project itself.

## Environment

- The research needs `github.com`, `go.microsoft.com`, `download.microsoft.com`, `learn.microsoft.com`, and the package registries (apt, npm, PyPI, crates.io, NuGet). If the environment's network is ever set to a limited list, those hosts must be on it.
- The environment's setup script installs REA and its tools, plus `msitools`, the .NET 8 SDK, and ILSpy. `tools/re/CLAUDE.md` lists them. The setup script lives in the environment settings on claude.ai, not in this repository.
- Containers on this environment have had 4 CPUs. Check with `nproc`. A Workflow runs at most `nproc - 2` agents at a time, so 2 here. Prefer a few deep agents over many shallow ones, and remember that Ghidra exports compete for the same CPUs.
- Write downloads, extracted installers, and decompiled output to the session scratchpad, never to the repository.

## GitHub from a cloud session

- Use the `mcp__github__*` tools for issues and pull requests. In cloud sessions the `gh` command is only allowed for search. `docs/agents/issue-tracker.md` maps each issue operation to its MCP call.
- `issue_write` with `labels` replaces every label on the issue. Read the issue with `issue_read` first. Then send the full list: the labels it keeps, plus the one you add or minus the one you remove.
- The MCP tools cannot create GitHub's native blocking links or delete comments. Write blockers in the ticket's `## Blocked by` section, and the Blocked by workflow creates the links.
- `issue_read` does not return an issue's database `id`, which `sub_issue_write` needs. The result of an `issue_write` call on the issue does.
- These tools cannot create labels or milestones or change repository settings. Labels come from `.github/labels.json` through the Labels workflow.
- Open a pull request with `create_pull_request` and `draft: true`. Mark it ready for review with `update_pull_request` and `draft: false`, and convert it back to a draft with `draft: true`.
- A push that deletes a branch is refused with HTTP 403, and auto mode blocks force-pushes. Leave branch deletion to GitHub, which deletes a pull request's branch when it is merged.

## Research tools

- Read `tools/re/README.md` before you use `rea`, `ilspycmd`, or the other research tools. Claude Code then loads `tools/re/CLAUDE.md`, which lists the installed tools and their limits.
- Use the `mcp__rea__*` tools only from the main conversation, because the `rea` MCP server holds one open binary that every subagent shares. Subagents run the `rea` command instead, each on its own binary.
