#!/usr/bin/env python3
"""Check the repository's Markdown files.

Fails on a relative link to a missing file, a link to a missing heading
anchor, a link that leaves the repository, an unclosed code fence, a file
that is not UTF-8, an em or en dash, or a curly quote. It checks the
repository this script lives in, from any working directory:

    python3 tools/check_docs.py
"""
import os
import re
import sys
import urllib.parse
from collections import Counter

BANNED = {
    "—": "em dash",
    "–": "en dash",
    "‘": "curly quote",
    "’": "curly quote",
    "“": "curly quote",
    "”": "curly quote",
}
LINK = re.compile(r"\]\(\s*<?([^)\s>]+)>?(?:\s+\"[^\"]*\")?\s*\)")
REFERENCE = re.compile(r"^ {0,3}\[[^\]]+\]:\s*<?(\S+?)>?(?:\s|$)")
HEADING = re.compile(r"^ {0,3}#{1,6}[ \t]+(.+?)(?:[ \t]+#+)?[ \t]*$")
FENCE = re.compile(r"^\s*(`{3,}|~{3,})(.*)$")
SCHEME = re.compile(r"^[A-Za-z][A-Za-z0-9+.-]*:")
CODE_SPAN = re.compile(r"(`+).+?\1")
COMMENT = re.compile(r"<!--.*?-->", re.S)
SKIP_DIRS = {".git", "target", "node_modules"}


def markdown_files(root):
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [d for d in dirnames if d not in SKIP_DIRS]
        for name in filenames:
            if name.endswith(".md"):
                yield os.path.relpath(os.path.join(dirpath, name), root)


def outside_fences(lines):
    """Return (lines outside code fences as (number, text), whether a fence is left open)."""
    fence = None
    kept = []
    for number, line in enumerate(lines, 1):
        match = FENCE.match(line)
        if fence is None:
            if match:
                fence = match.group(1)
                continue
            kept.append((number, line))
        elif match and match.group(1)[0] == fence[0] and len(match.group(1)) >= len(fence) and not match.group(2).strip():
            fence = None
    return kept, fence is not None


def uncomment(text):
    return COMMENT.sub(lambda m: "\n" * m.group(0).count("\n"), text)


def slug(heading):
    heading = re.sub(r"!?\[([^\]]*)\]\([^)]*\)", r"\1", heading)
    text = re.sub(r"`", "", heading.strip().lower())
    text = re.sub(r"[^\w\- ]", "", text)
    return text.replace(" ", "-")


def read(path):
    with open(path, encoding="utf-8") as handle:
        return handle.read()


def anchors(text):
    seen = Counter()
    result = set()
    lines, _ = outside_fences(uncomment(text).splitlines())
    for _, line in lines:
        match = HEADING.match(line)
        if not match:
            continue
        base = slug(match.group(1))
        name = base if seen[base] == 0 else f"{base}-{seen[base]}"
        seen[base] += 1
        result.add(name)
    return result


def targets(line):
    yield from LINK.findall(CODE_SPAN.sub("", line))
    match = REFERENCE.match(line)
    if match:
        yield match.group(1)


def check(root):
    files = sorted(markdown_files(root))
    texts, problems = {}, []
    for path in files:
        try:
            texts[path] = read(os.path.join(root, path))
        except UnicodeDecodeError as error:
            problems.append(f"{path}: not UTF-8 ({error.reason})")
    known = {path: anchors(text) for path, text in texts.items()}
    for path, text in texts.items():
        for number, line in enumerate(text.splitlines(), 1):
            for char, what in BANNED.items():
                if char in line:
                    problems.append(f"{path}:{number}: {what}")
        lines, unclosed = outside_fences(uncomment(text).splitlines())
        if unclosed:
            problems.append(f"{path}: code fence is never closed")
        for number, line in lines:
            for target in targets(line):
                if SCHEME.match(target):
                    continue
                file_part, _, fragment = target.partition("#")
                file_part = urllib.parse.unquote(file_part.split("?", 1)[0])
                if file_part.startswith("/"):
                    dest = os.path.normpath(file_part.lstrip("/"))
                elif file_part:
                    dest = os.path.normpath(os.path.join(os.path.dirname(path), file_part))
                else:
                    dest = path
                if dest == ".." or dest.startswith(".." + os.sep):
                    problems.append(f"{path}:{number}: link leaves the repository {target}")
                elif not os.path.exists(os.path.join(root, dest)):
                    problems.append(f"{path}:{number}: missing file {target}")
                elif fragment and dest in known and fragment not in known[dest]:
                    problems.append(f"{path}:{number}: missing anchor {target}")
    return files, problems


def main():
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    files, problems = check(root)
    for problem in problems:
        print(problem)
    print(f"checked {len(files)} Markdown files, {len(problems)} problems")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
