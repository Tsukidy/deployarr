#!/usr/bin/env python3
"""Create or update the repository's labels from .github/labels.json.

Runs the GitHub CLI, so it needs GH_TOKEN with permission to write issues.
It never renames or deletes a label. Usage:

    python3 tools/sync_labels.py .github/labels.json
"""
import json
import re
import subprocess
import sys

COLOR = re.compile(r"^[0-9a-fA-F]{6}$")


def validate(labels):
    errors, seen = [], set()
    for index, label in enumerate(labels):
        name = label.get("name")
        if not isinstance(name, str) or not name.strip():
            errors.append(f"entry {index}: name must be a non-empty string")
            continue
        if name.lower() in seen:
            errors.append(f"{name}: duplicate name (GitHub ignores case)")
        seen.add(name.lower())
        if not isinstance(label.get("color"), str) or not COLOR.match(label["color"]):
            errors.append(f"{name}: color must be 6 hex digits")
        description = label.get("description")
        if not isinstance(description, str) or len(description) > 100:
            errors.append(f"{name}: description must be a string of at most 100 characters")
    return errors


def main(path):
    with open(path, encoding="utf-8") as handle:
        labels = json.load(handle)
    errors = validate(labels)
    if errors:
        for error in errors:
            print(f"invalid  {error}")
        return 1
    failed = 0
    for label in labels:
        command = [
            "gh", "label", "create", label["name"],
            "--color", label["color"],
            "--description", label["description"],
            "--force",
        ]
        try:
            result = subprocess.run(command, capture_output=True, text=True)
        except FileNotFoundError:
            print("gh not found")
            return 1
        if result.returncode == 0:
            print(f"ok       {label['name']}")
        else:
            failed += 1
            print(f"failed   {label['name']}: {result.stderr.strip()}")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1] if len(sys.argv) > 1 else ".github/labels.json"))
