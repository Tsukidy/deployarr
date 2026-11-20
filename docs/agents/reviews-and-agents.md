# Reviews and agents

This page holds the rules for how a session runs pull request reviews, subagents, and workflow agents. [Branches and pull requests](../../AGENTS.md#branches-and-pull-requests) links it.

## Cargo builds by agents that run at once

A session never lets two agents that run at the same time share one cargo target folder, through one checkout or through one `CARGO_TARGET_DIR`. Neither `.cargo/config.toml` nor the `env` environment sets a target folder, so cargo builds into the `target` folder of the checkout it runs in. A shared folder lets one agent's test run pick up another agent's build, so local test results stop being reliable.

- Each subagent or workflow agent that runs `cargo` while another agent may run it too sets its own `CARGO_TARGET_DIR` before its first `cargo` command. A folder in the session scratchpad, such as `cargo-target-issue-361` for the agent that works issue #361, is an example.
- The session that starts such an agent names that folder in the agent's prompt, because the agent does not read this page.

## Pull request reviews

These rules hold at every effort level, and with ultracode on.

- Each pull request gets one review round, sized to the diff. A diff of a few lines gets a small review. A larger or riskier diff gets a deeper review.
- The round ends when each of its findings is applied or rejected. Do not start another round to look for new findings.
- A finding gets a second check, such as a skeptic who tries to refute it, only when it changes a decision, a claim, or code.
- Wording fixes go in without another review round.
- When an owner answer changes the diff after the round, only the changed lines get one more review, sized to that change.

This round is the review that [Branches and pull requests](../../AGENTS.md#branches-and-pull-requests) asks you to run before you mark the pull request ready. A deeper review uses a few deep agents, not many shallow ones, as `CLAUDE.md` prefers.

### Examples

| Diff | Review |
| --- | --- |
| A few lines that add one rule to a page in `docs/agents/` | One reviewer reads the diff against the ticket's acceptance criteria and checks that no other rule contradicts it. |
| An owner answer that rewrites one sentence after the round | One reviewer reads that sentence against the answer. |
| A research page with many new claims and citations | A few reviewers split the claims, and each opens the citations it checks. A skeptic checks each finding that changes a claim. |
