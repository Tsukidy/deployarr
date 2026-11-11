#!/usr/bin/env python3
"""Check the repository's Markdown files.

Fails on a relative link to a missing file, a link to a missing heading
anchor, an em or en dash, or a curly quote. Run from the repository root:

    python3 tools/check_docs.py
"""
import os
import re
import sys
from collections import Counter

BANNED = {
    "—": "em dash",
    "–": "en dash",
    "‘": "curly quote",
    "’": "curly quote",
    "“": "curly quote",
    "”": "curly quote",
}
LINK = re.compile(r"\]\(([^)\s]+)\)")
HEADING = re.compile(r"^#{1,6} (.+?)\s*#*\s*$")
SKIP_DIRS = {".git", "target", "node_modules"}


def markdown_files(root):
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [d for d in dirnames if d not in SKIP_DIRS]
        for name in filenames:
            if name.endswith(".md"):
                yield os.path.relpath(os.path.join(dirpath, name), root)


def strip_code(lines):
    """Yield (line number, text) for lines outside fenced code blocks."""
    fenced = False
    for number, line in enumerate(lines, 1):
        if line.lstrip().startswith("```"):
            fenced = not fenced
            continue
        if not fenced:
            yield number, line


def slug(heading):
    text = re.sub(r"`", "", heading.strip().lower())
    text = re.sub(r"[^\w\- ]", "", text)
    return text.replace(" ", "-")


def anchors(path):
    seen = Counter()
    result = set()
    with open(path, encoding="utf-8") as handle:
        for _, line in strip_code(handle.read().splitlines()):
            match = HEADING.match(line)
            if not match:
                continue
            base = slug(match.group(1))
            name = base if seen[base] == 0 else f"{base}-{seen[base]}"
            seen[base] += 1
            result.add(name)
    return result


def check(root):
    files = sorted(markdown_files(root))
    known = {path: anchors(os.path.join(root, path)) for path in files}
    problems = []
    for path in files:
        with open(os.path.join(root, path), encoding="utf-8") as handle:
            lines = handle.read().splitlines()
        for number, line in enumerate(lines, 1):
            for char, what in BANNED.items():
                if char in line:
                    problems.append(f"{path}:{number}: {what}")
        for number, line in strip_code(lines):
            for target in LINK.findall(line):
                if re.match(r"^[a-z]+:", target):
                    continue
                file_part, _, fragment = target.partition("#")
                if file_part:
                    dest = os.path.normpath(os.path.join(os.path.dirname(path), file_part))
                else:
                    dest = path
                if not os.path.exists(os.path.join(root, dest)):
                    problems.append(f"{path}:{number}: missing file {target}")
                elif fragment and dest in known and fragment not in known[dest]:
                    problems.append(f"{path}:{number}: missing anchor {target}")
    return files, problems


def main():
    files, problems = check(".")
    for problem in problems:
        print(problem)
    print(f"checked {len(files)} Markdown files, {len(problems)} problems")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
