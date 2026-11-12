"""Tests for doc_section.py. Run with: python3 -m unittest discover -s tools -p 'test_*.py'"""
import contextlib
import io
import os
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import check_docs  # noqa: E402
from doc_section import find, main, outline  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SCRIPT = os.path.join(ROOT, "tools", "doc_section.py")

DOC = """Preamble text.

# Title

Intro.

## Alpha

Alpha text.

### Alpha detail

Detail text.

#### Deepest

Deep text.

## Beta ##

```sh
# Not a heading
## Also not a heading
```

~~~~
### Tilde fence, not a heading
~~~~

Beta after the fences.

## Alpha

Second alpha text.

<!--
## Commented out
-->

### Alpha detail

Second detail text.

# Appendix

Last line."""


def run(*argv):
    """Run main() in process and return (exit code, stdout, stderr)."""
    out, err = io.StringIO(), io.StringIO()
    with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
        code = main(list(argv))
    return code, out.getvalue(), err.getvalue()


class OutlineTest(unittest.TestCase):
    def setUp(self):
        self.sections = outline(DOC)

    def section(self, anchor):
        found = find(self.sections, anchor)
        self.assertIsNotNone(found, anchor)
        return found.text

    def test_outline_lists_levels_anchors_and_lines(self):
        self.assertEqual(
            [(s.level, s.anchor, s.line) for s in self.sections],
            [
                (1, "title", 3),
                (2, "alpha", 7),
                (3, "alpha-detail", 11),
                (4, "deepest", 15),
                (2, "beta", 19),
                (2, "alpha-1", 32),
                (3, "alpha-detail-1", 40),
                (1, "appendix", 44),
            ],
        )

    def test_section_includes_its_subsections_and_stops_at_the_same_level(self):
        text = self.section("alpha")
        self.assertTrue(text.startswith("## Alpha\n"))
        self.assertIn("### Alpha detail", text)
        self.assertIn("Deep text.", text)
        self.assertNotIn("## Beta", text)

    def test_section_stops_at_a_higher_level(self):
        text = self.section("deepest")
        self.assertEqual(text, "#### Deepest\n\nDeep text.\n\n")
        self.assertEqual(self.section("alpha-detail-1"), "### Alpha detail\n\nSecond detail text.\n\n")

    def test_top_level_section_runs_to_the_next_top_level_heading(self):
        text = self.section("title")
        self.assertTrue(text.startswith("# Title\n"))
        self.assertIn("Second detail text.", text)
        self.assertNotIn("# Appendix", text)

    def test_last_section_runs_to_the_end(self):
        self.assertEqual(self.section("appendix"), "# Appendix\n\nLast line.")

    def test_duplicate_headings_get_numbered_anchors_in_order(self):
        self.assertIn("Alpha text.", self.section("alpha"))
        self.assertNotIn("Second alpha text.", self.section("alpha"))
        self.assertIn("Second alpha text.", self.section("alpha-1"))
        self.assertIn("Detail text.", self.section("alpha-detail"))
        self.assertIn("Second detail text.", self.section("alpha-detail-1"))

    def test_a_repeated_anchor_resolves_to_the_first_heading(self):
        # "Foo 1" slugs to foo-1, the anchor the second "Foo" already took.
        sections = outline("## Foo\n\none\n\n## Foo\n\ntwo\n\n## Foo 1\n\nthree\n")
        self.assertEqual([s.anchor for s in sections], ["foo", "foo-1", "foo-1"])
        self.assertIn("two", find(sections, "foo-1").text)

    def test_hash_lines_inside_fences_are_not_headings(self):
        anchors = {s.anchor for s in self.sections}
        self.assertNotIn("not-a-heading", anchors)
        self.assertNotIn("also-not-a-heading", anchors)
        self.assertNotIn("tilde-fence-not-a-heading", anchors)
        text = self.section("beta")
        self.assertIn("# Not a heading", text)
        self.assertIn("### Tilde fence, not a heading", text)
        self.assertIn("Beta after the fences.", text)
        self.assertNotIn("Second alpha text.", text)

    def test_headings_inside_comments_are_not_headings(self):
        self.assertIsNone(find(self.sections, "commented-out"))
        self.assertIn("## Commented out", self.section("alpha-1"))

    def test_size_is_the_section_byte_count(self):
        for s in self.sections:
            self.assertEqual(s.size, len(s.text.encode("utf-8")))
        top = [s for s in self.sections if s.level == 1]
        preamble = DOC[: DOC.index("# Title")]
        self.assertEqual(sum(s.size for s in top) + len(preamble), len(DOC.encode("utf-8")))

    def test_size_counts_bytes_not_characters(self):
        [section] = outline("# Café\n\né\n")
        self.assertEqual(section.anchor, "café")
        self.assertEqual(section.size, len("# Café\n\né\n".encode("utf-8")))


class CommandTest(unittest.TestCase):
    def setUp(self):
        handle = tempfile.NamedTemporaryFile("w", suffix=".md", delete=False, encoding="utf-8", newline="")
        with handle:
            handle.write(DOC)
        self.path = handle.name
        self.addCleanup(os.unlink, self.path)

    def test_outline(self):
        code, out, err = run(self.path)
        self.assertEqual((code, err), (0, ""))
        lines = out.splitlines()
        self.assertIn(f"{len(DOC.encode('utf-8'))} bytes, 8 headings", lines[0])
        self.assertRegex(lines[4], r"^\s+7\s+\d+  ## alpha$")
        self.assertRegex(lines[-1], r"^\s+44\s+\d+  # appendix$")

    def test_section(self):
        code, out, err = run(self.path, "alpha-1")
        self.assertEqual((code, err), (0, ""))
        self.assertTrue(out.startswith("## Alpha\n\nSecond alpha text."))
        self.assertTrue(out.endswith("Second detail text.\n\n"))

    def test_link_forms(self):
        expected = run(self.path, "beta")
        self.assertEqual(run(self.path, "#beta"), expected)
        self.assertEqual(run(f"{self.path}#beta"), expected)

    def test_unknown_anchor_fails_and_lists_anchors(self):
        code, out, err = run(self.path, "gamma")
        self.assertEqual(code, 1)
        self.assertEqual(out, "")
        self.assertIn("'gamma'", err)
        listed = [line.strip() for line in err.splitlines()[1:]]
        self.assertEqual(listed, [s.anchor for s in outline(DOC)])

    def test_crlf_file_keeps_its_bytes(self):
        with open(self.path, "w", encoding="utf-8", newline="") as handle:
            handle.write("# A\r\n\r\ntext\r\n# B\r\n")
        code, out, _ = run(self.path)
        self.assertEqual(code, 0)
        self.assertIn(f": {os.path.getsize(self.path)} bytes, 2 headings", out)
        self.assertEqual(run(self.path, "a"), (0, "# A\r\n\r\ntext\r\n", ""))

    def test_missing_file_fails(self):
        code, out, err = run(self.path + ".missing")
        self.assertEqual(code, 2)
        self.assertEqual(out, "")
        self.assertIn("No such file", err)

    def test_runs_as_a_command(self):
        result = subprocess.run(
            [sys.executable, "-I", SCRIPT, os.path.join(ROOT, "docs", "architecture.md"), "agent"],
            capture_output=True, text=True, encoding="utf-8", check=False,
        )
        self.assertEqual((result.returncode, result.stderr), (0, ""))
        self.assertTrue(result.stdout.startswith("## Agent\n"))


class RepositoryDocsTest(unittest.TestCase):
    def test_every_anchor_resolves_and_matches_the_docs_check(self):
        files = sorted(check_docs.markdown_files(ROOT))
        self.assertGreater(len(files), 0)
        for path in files:
            with self.subTest(path=path):
                full = os.path.join(ROOT, path)
                with open(full, encoding="utf-8", newline="") as handle:
                    text = handle.read()
                sections = outline(text)
                expected = check_docs.anchors(check_docs.read(full))
                self.assertEqual({s.anchor for s in sections}, expected)
                lines = text.splitlines(keepends=True)
                starts = {s.line: s.level for s in sections}
                for anchor in expected:
                    section = find(sections, anchor)
                    self.assertIsNotNone(section, anchor)
                    # The section starts at its own heading line.
                    count = len(section.text.splitlines())
                    self.assertEqual(section.text, "".join(lines[section.line - 1:section.line - 1 + count]))
                    match = check_docs.HEADING.match(lines[section.line - 1])
                    self.assertIsNotNone(match, anchor)
                    base = check_docs.slug(match.group(2))
                    self.assertTrue(anchor == base or anchor.startswith(base + "-"), anchor)
                    # It ends at the end of the file or at a heading of the same or a higher level.
                    after = section.line + count
                    self.assertTrue(after == len(lines) + 1 or starts.get(after, 7) <= section.level, anchor)


if __name__ == "__main__":
    unittest.main()
