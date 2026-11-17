#!/usr/bin/env python3
"""Check the repository's Markdown files.

Fails on a relative link to a missing file, a link to a missing heading
anchor, a link that leaves the repository, an unclosed code fence, a file
that is not UTF-8, an em or en dash, or a curly quote. It checks the
repository this script lives in, from any working directory:

    python3 tools/check_docs.py

It skips nested checkouts. A folder below the repository root that holds a
.git file or folder is another checkout, such as an agent worktree under
.claude/worktrees/, a submodule, or a clone. Git does not track the files in
it as part of this repository, so the check does not read them. A run from
inside a worktree checks that worktree's own files.

It also sums the bytes of the agent files that Claude Code loads into every
session and subagent, and fails when the total is over AGENT_FILES_BUDGET.
The set is AGENTS.md and CLAUDE.md, which must exist, .claude/CLAUDE.md, each
.claude/rules/**/*.md file with no paths frontmatter field, each file these
import with @path outside code spans and fences up to four hops, and the
listing entry of each .claude/skills/*/SKILL.md. The memory and skills pages
of the Claude Code docs give these loading rules.
"""
import os
import re
import sys
import urllib.parse
from collections import Counter, deque

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
HEADING = re.compile(r"^ {0,3}(#{1,6})[ \t]+(.+?)(?:[ \t]+#+)?[ \t]*$")
FENCE = re.compile(r"^\s*(`{3,}|~{3,})(.*)$")
SCHEME = re.compile(r"^[A-Za-z][A-Za-z0-9+.-]*:")
CODE_SPAN = re.compile(r"(`+).+?\1")
COMMENT = re.compile(r"<!--.*?-->", re.S)
# Every character that str.splitlines() does not split on.
NOT_BREAK = re.compile(r"[^\n\r\v\f\x1c\x1d\x1e\x85\u2028\u2029]")
SKIP_DIRS = {".git", "target", "node_modules"}

# The most bytes the always-loaded agent files may total. It was their size
# after #65, plus the placement rule that #66 adds to AGENTS.md, which came to
# 13,010 bytes, rounded up to the next 500. For #201 the owner raised it to
# 13,700, so that the work list rule fits without cutting other text. For
# #203 the owner raised it to 14,000, so that the rules against cutting text
# and against recording answers the owner did not give fit.
AGENT_FILES_BUDGET = 14000
REQUIRED_AGENT_FILES = ("AGENTS.md", "CLAUDE.md")
IMPORT_HOPS = 4
# An @path import. A backslash before a space keeps the space in the path.
IMPORT = re.compile(r"(?<!\S)@((?:\\ |\S)+)")
FRONTMATTER = re.compile(r"\A---[ \t]*\r?\n(.*?)^---[ \t]*\r?$", re.S | re.M)
FIELD = re.compile(r"([A-Za-z_][\w-]*)[ \t]*:(.*)$")


def markdown_files(root):
    """Yield the path, relative to root, of each Markdown file in the checkout at root.

    It skips the folders in SKIP_DIRS and every folder below root that holds
    a .git entry, which marks a nested checkout.
    """
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [
            d for d in dirnames
            if d not in SKIP_DIRS and not os.path.lexists(os.path.join(dirpath, d, ".git"))
        ]
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


def uncommented_lines(text):
    """Return text.splitlines() with HTML comments removed, so line N is still line N of the text.

    Each comment character that is not a line break becomes NUL before the
    split, and the NULs are dropped after it. Writing a newline for each
    break in a comment would shift the lines after a comment that holds a
    lone carriage return or a form feed.
    """
    masked = COMMENT.sub(lambda m: NOT_BREAK.sub("\0", m.group(0)), text)
    return [line.replace("\0", "") for line in masked.splitlines()]


def slug(heading):
    heading = re.sub(r"!?\[([^\]]*)\]\([^)]*\)", r"\1", heading)
    text = re.sub(r"`", "", heading.strip().lower())
    text = re.sub(r"[^\w\- ]", "", text)
    return text.replace(" ", "-")


def read(path):
    with open(path, encoding="utf-8") as handle:
        return handle.read()


def headings(text):
    """Yield (line number, level, anchor) for each heading outside code fences and comments.

    A repeated anchor gets the suffix -1, -2, and so on, in document order.
    """
    seen = Counter()
    lines, _ = outside_fences(uncommented_lines(text))
    for number, line in lines:
        match = HEADING.match(line)
        if not match:
            continue
        base = slug(match.group(2))
        yield number, len(match.group(1)), base if seen[base] == 0 else f"{base}-{seen[base]}"
        seen[base] += 1


def anchors(text):
    return {anchor for _, _, anchor in headings(text)}


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
        lines, unclosed = outside_fences(uncommented_lines(text))
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


def read_bytes(path):
    with open(path, "rb") as handle:
        return handle.read()


def frontmatter(text):
    """Return (the top-level fields of the YAML frontmatter that starts text, the text after it).

    Each value is its raw text, with continuation lines joined by a space, a
    block scalar indicator dropped, and one pair of surrounding quotes removed.
    This covers the plain fields that rules and skills use. It is not a YAML
    parser.
    """
    match = FRONTMATTER.match(text)
    if not match:
        return {}, text
    fields, key = {}, None
    for line in match.group(1).splitlines():
        field = FIELD.match(line)
        if field:
            key = field.group(1)
            fields[key] = field.group(2).strip()
        elif key is not None and line.strip():
            fields[key] = f"{fields[key]} {line.strip()}".strip()
    for key, value in fields.items():
        value = re.sub(r"^[|>][+-]?\d*(?:\s+|$)", "", value)
        if len(value) >= 2 and value[0] == value[-1] and value[0] in "\"'":
            value = value[1:-1]
        fields[key] = value
    return fields, text[match.end():]


def imports(text):
    """Yield the path of each @path import in text, skipping code spans and fenced code blocks."""
    lines, _ = outside_fences(text.splitlines())
    for _, line in lines:
        yield from IMPORT.findall(CODE_SPAN.sub(" ", line))


def resolve_import(root, importer, token):
    """Return the path, relative to root, of the file that token imports into importer.

    A relative path starts at the importer's folder. A #fragment is dropped,
    so the check counts the file whether or not Claude Code keeps it.
    Return None when the check cannot measure what Claude Code would load,
    because the path is quoted, leaves the repository, or names no file.
    """
    path = token.replace("\\ ", " ").split("#", 1)[0]
    if not path or path[0] in "\"'~":
        return None
    if not os.path.isabs(path):
        path = os.path.join(root, os.path.dirname(importer), path)
    try:
        rel = os.path.relpath(os.path.normpath(path), root)
    except ValueError:  # Another drive on Windows.
        return None
    if rel == ".." or rel.startswith(".." + os.sep) or not os.path.isfile(os.path.join(root, rel)):
        return None
    return rel


def unscoped_rules(root):
    """Yield the path of each .claude/rules/**/*.md file with no paths field, which loads at launch."""
    for dirpath, dirnames, filenames in os.walk(os.path.join(root, ".claude", "rules")):
        dirnames.sort()
        for name in sorted(filenames):
            full = os.path.join(dirpath, name)
            if not name.endswith(".md"):
                continue
            fields, _ = frontmatter(read_bytes(full).decode("utf-8", "replace"))
            if not fields.get("paths"):
                yield os.path.relpath(full, root)


def skill_listing(text, name):
    """Return the text that a skill adds to the skill listing, which every session loads.

    The listing holds the skill's name, which defaults to its folder's name,
    and its description and when_to_use fields. A missing description defaults
    to the first non-empty line of the body. A skill with
    disable-model-invocation set to true lists no description.
    """
    fields, body = frontmatter(text)
    parts = [fields.get("name") or name]
    if fields.get("disable-model-invocation", "").lower() != "true":
        first_line = next((line.strip() for line in body.splitlines() if line.strip()), "")
        parts += [fields.get("description") or first_line, fields.get("when_to_use", "")]
    return "\n".join(part for part in parts if part)


def agent_files(root):
    """Return ({path: bytes} for the always-loaded agent files under root, [missing required files]).

    Imports are followed breadth first, so each file counts once, at its
    fewest hops. A skill counts only the bytes of its listing entry.
    """
    missing = [path for path in REQUIRED_AGENT_FILES if not os.path.isfile(os.path.join(root, path))]
    starts = [path for path in REQUIRED_AGENT_FILES if path not in missing]
    if os.path.isfile(os.path.join(root, ".claude", "CLAUDE.md")):
        starts.append(os.path.join(".claude", "CLAUDE.md"))
    starts.extend(unscoped_rules(root))
    sizes, seen = {}, set()
    queue = deque((path, 0) for path in starts)
    while queue:
        path, hops = queue.popleft()
        full = os.path.join(root, path)
        real = os.path.realpath(full)
        if real in seen:
            continue
        seen.add(real)
        data = read_bytes(full)
        sizes[path] = len(data)
        if hops < IMPORT_HOPS:
            for token in imports(data.decode("utf-8", "replace")):
                target = resolve_import(root, path, token)
                if target is not None:
                    queue.append((target, hops + 1))
    skills = os.path.join(root, ".claude", "skills")
    for name in sorted(os.listdir(skills)) if os.path.isdir(skills) else ():
        path = os.path.join(".claude", "skills", name, "SKILL.md")
        if os.path.isfile(os.path.join(root, path)):
            listing = skill_listing(read_bytes(os.path.join(root, path)).decode("utf-8", "replace"), name)
            sizes[f"{path} (listing)"] = len(listing.encode("utf-8"))
    return sizes, missing


def check_agent_files(root):
    """Return (the total bytes of the always-loaded agent files under root, problems)."""
    sizes, missing = agent_files(root)
    problems = [f"{path}: always-loaded agent file is missing" for path in missing]
    total = sum(sizes.values())
    if total > AGENT_FILES_BUDGET:
        width = len(f"{max(sizes.values()):,}")
        lines = [f"always-loaded agent files: {total:,} bytes, over the budget of {AGENT_FILES_BUDGET:,} bytes"]
        lines += [f"  {size:>{width},}  {path}" for path, size in sizes.items()]
        lines.append(
            "  Do not delete or shorten existing text to fit. Ask the owner whether to move"
            " named text to its task's doc, behind a one-line link, or to raise"
            " AGENT_FILES_BUDGET in tools/check_docs.py."
        )
        problems.append("\n".join(lines))
    return total, problems


def main(root=None):
    root = root or os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    files, problems = check(root)
    total, budget_problems = check_agent_files(root)
    problems += budget_problems
    for problem in problems:
        print(problem)
    if total <= AGENT_FILES_BUDGET:
        print(f"always-loaded agent files: {total:,} of {AGENT_FILES_BUDGET:,} bytes")
    print(f"checked {len(files)} Markdown files, {len(problems)} problems")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
