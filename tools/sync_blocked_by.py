#!/usr/bin/env python3
"""Make an issue's native "blocked by" links match its `## Blocked by` section.

The GitHub MCP tools that cloud agents use cannot create GitHub's native issue
dependencies, so agents list blockers in the issue body instead. The Blocked by
workflow runs this script on every opened, edited, or reopened issue.

An issue with no `## Blocked by` section is left unchanged.

Usage: GH_REPO=owner/repo sync_blocked_by.py <issue number>
"""
import json
import os
import re
import subprocess
import sys

HEADING = re.compile(r"^ {0,3}#{1,6}[ \t]+(.*?)[ \t#]*$")
BLOCKED_LINE = re.compile(r"^\s*\**blocked by\**\s*:(.*)$", re.IGNORECASE)
REFERENCE = re.compile(r"(?<![\w/#&])#(\d+)\b")
COMMENT = re.compile(r"<!--.*?-->", re.S)
CODE_SPAN = re.compile(r"(`+).+?\1")


def blockers(body):
    """Return the issue numbers listed as blockers, or None if no section exists."""
    lines = COMMENT.sub("", body or "").splitlines()
    found = None
    in_section = False
    in_fence = False
    for line in lines:
        if re.match(r"^\s*(`{3,}|~{3,})", line):
            in_fence = not in_fence
            continue
        if in_fence:
            continue
        heading = HEADING.match(line)
        if heading:
            in_section = heading.group(1).strip().lower().rstrip(":") == "blocked by"
            if in_section and found is None:
                found = set()
            continue
        text = None
        if in_section:
            text = line
        else:
            match = BLOCKED_LINE.match(line)
            if match:
                found = found if found is not None else set()
                text = match.group(1)
        if text is not None:
            found.update(int(n) for n in REFERENCE.findall(CODE_SPAN.sub("", text)))
    return found


class GitHub:
    """The few REST calls this script needs, made through the `gh` command."""

    def __init__(self, repo):
        self.repo = repo

    def _api(self, *args):
        result = subprocess.run(["gh", "api", *args], capture_output=True, text=True)
        if result.returncode != 0:
            raise RuntimeError(result.stderr.strip() or f"gh api {' '.join(args)} failed")
        return result.stdout

    def issue(self, number):
        data = json.loads(self._api(f"repos/{self.repo}/issues/{number}"))
        return {"number": data["number"], "id": data["id"], "body": data.get("body") or "",
                "is_pull_request": "pull_request" in data}

    def blocked_by(self, number):
        out = self._api("--paginate", f"repos/{self.repo}/issues/{number}/dependencies/blocked_by",
                        "--jq", ".[] | [.number, .id] | @tsv")
        return {int(n): int(i) for n, i in (line.split("\t") for line in out.splitlines() if line)}

    def add_blocker(self, number, blocker_id):
        self._api("--method", "POST", f"repos/{self.repo}/issues/{number}/dependencies/blocked_by",
                  "-F", f"issue_id={blocker_id}")

    def remove_blocker(self, number, blocker_id):
        self._api("--method", "DELETE", f"repos/{self.repo}/issues/{number}/dependencies/blocked_by/{blocker_id}")


def sync(api, number):
    """Sync one issue. Returns a list of error messages."""
    issue = api.issue(number)
    wanted = blockers(issue["body"])
    if wanted is None:
        print(f"#{number} has no Blocked by section. Its links are left unchanged.")
        return []
    wanted.discard(number)
    current = api.blocked_by(number)
    errors = []
    for blocker in sorted(wanted - set(current)):
        try:
            target = api.issue(blocker)
            if target["is_pull_request"]:
                raise RuntimeError("it is a pull request, not an issue")
            api.add_blocker(number, target["id"])
            print(f"#{number}: added blocked by #{blocker}")
        except RuntimeError as error:
            errors.append(f"#{number}: cannot add blocked by #{blocker}: {error}")
    for blocker in sorted(set(current) - wanted):
        try:
            api.remove_blocker(number, current[blocker])
            print(f"#{number}: removed blocked by #{blocker}")
        except RuntimeError as error:
            errors.append(f"#{number}: cannot remove blocked by #{blocker}: {error}")
    if not errors and wanted == set(current):
        print(f"#{number}: links already match the section")
    return errors


def main():
    if len(sys.argv) != 2 or not sys.argv[1].isdigit():
        sys.exit(__doc__.strip().splitlines()[-1])
    repo = os.environ.get("GH_REPO")
    if not repo:
        sys.exit("GH_REPO is not set")
    try:
        errors = sync(GitHub(repo), int(sys.argv[1]))
    except FileNotFoundError:
        sys.exit("gh not found")
    except RuntimeError as error:
        sys.exit(str(error))
    for error in errors:
        print(error, file=sys.stderr)
    sys.exit(1 if errors else 0)


if __name__ == "__main__":
    main()
