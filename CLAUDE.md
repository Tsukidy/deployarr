@AGENTS.md

# Claude Code cloud sessions

These notes cover cloud sessions on the `env` environment. `AGENTS.md` above covers the project itself.

## Questions for the owner

- Ask the owner every question with the `AskUserQuestion` tool, including design choices, scope checks, and approval of a ticket breakdown. A question left in reply text is easy to miss.
- A subagent returns its open question to the main conversation, which asks it with the tool.
- Put your recommended option first, marked `(Recommended)`.
- Record an answer about an issue on it as an `Owner input:` comment before you act.

## Environment

- The research needs `github.com`, `go.microsoft.com`, `download.microsoft.com`, `learn.microsoft.com`, and the package registries (apt, npm, PyPI, crates.io, NuGet). If the environment's network is ever set to a limited list, those hosts must be on it.
- The environment's setup script installs REA and its tools, plus `msitools`, the .NET 8 SDK, and ILSpy. `tools/re/CLAUDE.md` lists them. The setup script lives in the environment settings on claude.ai, not in this repository.
- Containers on this environment have had 4 CPUs. Check with `nproc`. A Workflow runs at most `nproc - 2` agents at a time, so 2 here. Prefer a few deep agents over many shallow ones, and remember that Ghidra exports compete for the same CPUs.
- Write downloads, extracted installers, and decompiled output to the session scratchpad, never to the repository.

## GitHub from a cloud session

- Use the `mcp__github__*` tools for issues and pull requests. In cloud sessions the `gh` command is only allowed for search. `docs/agents/issue-tracker.md` maps each issue operation to its MCP call.
- The MCP tools cannot delete comments, create labels or milestones, or change repository settings.
- Open a pull request with `create_pull_request` and `draft: true`. Mark it ready for review with `update_pull_request` and `draft: false`, and convert it back to a draft with `draft: true`.
- A push that deletes a branch is refused with HTTP 403, and auto mode blocks force-pushes.
- Watch a pull request only when the owner asks. Do not offer to.
- The owner allows [one branch per ticket](docs/agents/issue-tracker.md#branches-in-a-cloud-session). Do not ask.

## Research tools

- Before you use `rea`, `ilspycmd`, or the other research tools, read `tools/re/README.md` and `tools/re/CLAUDE.md`. The first gives the sources, steps, and pitfalls. The second lists the installed tools and their limits.
- Use the `mcp__rea__*` tools only from the main conversation, because the `rea` MCP server holds one open binary that every subagent shares. Subagents run the `rea` command instead, each on its own binary.
