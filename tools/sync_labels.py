#!/usr/bin/env python3
"""Create or update the repository's labels from .github/labels.json.

Runs the GitHub CLI, so it needs GH_TOKEN with permission to write issues.
It never deletes a label. Run from the repository root:

    python3 tools/sync_labels.py .github/labels.json
"""
import json
import subprocess
import sys


def main(path):
    with open(path, encoding="utf-8") as handle:
        labels = json.load(handle)
    failed = 0
    for label in labels:
        command = [
            "gh", "label", "create", label["name"],
            "--color", label["color"],
            "--description", label["description"],
            "--force",
        ]
        result = subprocess.run(command, capture_output=True, text=True)
        if result.returncode == 0:
            print(f"ok      {label['name']}")
        else:
            failed += 1
            print(f"failed  {label['name']}: {result.stderr.strip()}")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1] if len(sys.argv) > 1 else ".github/labels.json"))
