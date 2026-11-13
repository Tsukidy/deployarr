#!/usr/bin/env python3
"""Print a Markdown doc's outline, or one of its sections.

Read a large doc by outline first, then read only the sections you need:

    python3 tools/doc_section.py docs/architecture.md
    python3 tools/doc_section.py docs/architecture.md agent

The outline gives each heading's line number, the byte size of its section,
its level as `#` marks, and its anchor. A section runs from its heading to the
next heading of the same or a higher level, so it includes its subsections.

The anchor is the one a link uses, such as `agent` in
`docs/architecture.md#agent`. That link also works as a single argument, and
so does a GitHub link to a file in this repository, such as
`https://github.com/Tsukidy/deployarr/blob/main/docs/architecture.md#agent`.
The second argument is always an anchor, even when it starts with `-`.
Headings, anchors, and the numbering of repeated anchors come from
`check_docs.py`, so every anchor the docs check accepts works here. A `#` line
inside a code fence or an HTML comment is not a heading. When two headings
share an anchor, the first one wins, as it does in a browser.

An unknown anchor exits with status 1 and lists the file's anchors.
"""
import argparse
import os
import re
import sys
import urllib.parse
from typing import NamedTuple

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from check_docs import headings  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
# A link to a file of this repository, or of a fork, on one branch or commit.
GITHUB_LINK = re.compile(
    r"^https://github\.com/[^/]+/deployarr/blob/[^/]+/(?P<path>[^?#]+)(?:\?[^#]*)?(?:#(?P<anchor>.*))?$"
)


class Section(NamedTuple):
    anchor: str
    level: int
    line: int
    text: str

    @property
    def size(self):
        return len(self.text.encode("utf-8"))


def outline(text):
    """Return a Section for each heading in `text`, in document order."""
    lines = text.splitlines(keepends=True)
    found = list(headings(text))
    sections = []
    for index, (line, level, anchor) in enumerate(found):
        end = next((later for later, depth, _ in found[index + 1:] if depth <= level), len(lines) + 1)
        sections.append(Section(anchor, level, line, "".join(lines[line - 1:end - 1])))
    return sections


def find(sections, anchor):
    """Return the first section with `anchor`, or None."""
    return next((section for section in sections if section.anchor == anchor), None)


def render_outline(path, text, sections):
    total = len(text.encode("utf-8"))
    width = max(len(str(total)), 5)
    out = [
        f"{path}: {total} bytes, {len(sections)} headings",
        f"Print a section with: python3 tools/doc_section.py {path} <anchor>",
        f"{'line':>5}  {'bytes':>{width}}  anchor",
    ]
    for s in sections:
        out.append(f"{s.line:>5}  {s.size:>{width}}  {'#' * s.level} {s.anchor}")
    return "\n".join(out) + "\n"


def main(argv=None):
    parser = argparse.ArgumentParser(
        prog="doc_section.py",
        description="Print a Markdown doc's outline, or the section under one heading anchor.",
    )
    parser.add_argument("file", help="the Markdown file, FILE#ANCHOR, or a GitHub link to a file in this repository")
    parser.add_argument("anchor", nargs="?", help="the heading anchor whose section to print")
    argv = list(sys.argv[1:] if argv is None else argv)
    # The heading `--dry-run` has the anchor --dry-run, so read the second argument as an anchor, not an option.
    if len(argv) == 2 and not argv[0].startswith("-") and argv[1] != "--":
        argv.insert(1, "--")
    args = parser.parse_args(argv)
    path, anchor = args.file, args.anchor
    link = GITHUB_LINK.match(path)
    if link:
        path = os.path.relpath(os.path.join(ROOT, urllib.parse.unquote(link["path"])))
        if anchor is None:
            anchor = link["anchor"]
    elif anchor is None and "#" in path and not os.path.exists(path):
        path, anchor = path.split("#", 1)
    try:
        with open(path, encoding="utf-8", newline="") as handle:
            text = handle.read()
    except OSError as error:
        print(f"doc_section.py: {error}", file=sys.stderr)
        return 2
    except UnicodeDecodeError as error:
        print(f"doc_section.py: {path}: not UTF-8 ({error.reason})", file=sys.stderr)
        return 2
    sections = outline(text)
    if anchor is None:
        sys.stdout.write(render_outline(path, text, sections))
        return 0
    anchor = anchor.removeprefix("#")
    section = find(sections, anchor)
    if section is None:
        print(f"doc_section.py: {path} has no anchor {anchor!r}. Its anchors:", file=sys.stderr)
        for s in sections:
            print(f"  {s.anchor}", file=sys.stderr)
        return 1
    sys.stdout.write(section.text if section.text.endswith("\n") else section.text + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
