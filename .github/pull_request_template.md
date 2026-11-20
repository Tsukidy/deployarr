<!-- This body describes the final diff against main. Leave out the branch's history, such as its commits, its earlier versions, and its review rounds. -->

## Summary

<!-- The smallest view that makes the change clear: a diff sketch, a file tree, a call tree, pseudocode, or a Mermaid diagram. Keep prose short. -->

## Evidence

<!-- Before and after. A test that failed and now passes, command output, or a screenshot. -->

## Removed text

<!-- Each line on main that the final diff against main deletes, shortens, or rewrites. Leave out rewrites between commits on this branch. A line that one branch commit added and a later one rewrote is not listed, and neither is a line on main that one branch commit changed and a later one restored. For each listed line, say who asked for it, the ticket or the owner, and where its content lives now. Write None when the change only adds. -->

## Merge danger

<!-- The owner says, "I merge pull requests that are ready for review from the lowest number to the highest." -->

**Door:** <!-- one-way or two-way. A two-way door is cheap to roll back. -->

**Blast radius:** <!-- one word, then what could break if this is wrong -->

**Lower-numbered pull requests:** <!-- Each open pull request with a lower number that touches the same files. For each, say whether merging it first leaves this one with a conflict. On a conflict, name the pull request and the files. Write None when no lower-numbered open pull request touches the same files. A named conflict does not hold this pull request back. Mark it ready for review anyway. -->

<!-- Link the issue. Write Closes #<number> when merging completes it, or Refs #<number> when it does not, such as a page for a wontfix issue. -->
