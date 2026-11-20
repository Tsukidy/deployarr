# Reviews and agents

This page holds the rules for how a session runs pull request reviews, subagents, and workflow agents. [Branches and pull requests](../../AGENTS.md#branches-and-pull-requests) links it.

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
