"""Tests for sync_blocked_by.py. Run with: python3 -m unittest discover tools"""
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from sync_blocked_by import blockers, sync  # noqa: E402


class FakeGitHub:
    """Holds issues and native links in memory, keyed by issue number."""

    def __init__(self, issues, links=None, pulls=()):
        self.issues = issues
        self.links = {n: set(b) for n, b in (links or {}).items()}
        self.pulls = set(pulls)

    def _id(self, number):
        return 1000 + number

    def issue(self, number):
        if number not in self.issues and number not in self.pulls:
            raise RuntimeError("HTTP 404: Not Found")
        return {"number": number, "id": self._id(number), "body": self.issues.get(number, ""),
                "is_pull_request": number in self.pulls}

    def blocked_by(self, number):
        return {b: self._id(b) for b in self.links.get(number, set())}

    def add_blocker(self, number, blocker_id):
        self.links.setdefault(number, set()).add(blocker_id - 1000)

    def remove_blocker(self, number, blocker_id):
        self.links[number].discard(blocker_id - 1000)


class BlockersTest(unittest.TestCase):
    def test_no_section(self):
        self.assertIsNone(blockers("## What to build\n\nSee #4 for context."))
        self.assertIsNone(blockers(""))
        self.assertIsNone(blockers(None))

    def test_section_with_bullets(self):
        body = "## What to build\n\nSee #9.\n\n## Blocked by\n\n- #3\n- #5\n\n## Notes\n\n#7"
        self.assertEqual(blockers(body), {3, 5})

    def test_section_says_none(self):
        self.assertEqual(blockers("## Blocked by\n\nNone (can start immediately)."), set())

    def test_inline_line_is_not_a_section(self):
        self.assertIsNone(blockers("Blocked by: #12, #15\n\nBody text #99"))
        self.assertIsNone(blockers("**Blocked by:** #2"))
        self.assertIsNone(blockers("### What happened\n\nBlocked by: the ADK release"))

    def test_ignores_code_comments_and_other_repos(self):
        body = ("## Blocked by\n\n- #3\n- `#4`\n<!-- - #5 -->\n- other/repo#6\n- &#7;\n"
                "```\n- #8\n```\n- #9")
        self.assertEqual(blockers(body), {3, 9})

    def test_heading_variants(self):
        self.assertEqual(blockers("### Blocked by:\n\n- #2"), {2})
        self.assertEqual(blockers("## blocked BY ##\n- #2"), {2})
        self.assertIsNone(blockers("## Blocked by others\n- #2"))


class SyncTest(unittest.TestCase):
    def test_adds_missing_and_removes_extra(self):
        api = FakeGitHub({1: "", 2: "", 3: "", 10: "## Blocked by\n\n- #1\n- #2"}, links={10: {2, 3}})
        self.assertEqual(sync(api, 10), [])
        self.assertEqual(api.links[10], {1, 2})

    def test_prose_line_keeps_links(self):
        api = FakeGitHub({1: "", 10: "### What happened\n\nBlocked by: the ADK release"}, links={10: {1}})
        self.assertEqual(sync(api, 10), [])
        self.assertEqual(api.links[10], {1})

    def test_leaves_links_without_section(self):
        api = FakeGitHub({1: "", 10: "No section here, see #1"}, links={10: {1}})
        self.assertEqual(sync(api, 10), [])
        self.assertEqual(api.links[10], {1})

    def test_none_removes_all(self):
        api = FakeGitHub({1: "", 10: "## Blocked by\n\nNone (can start immediately)."}, links={10: {1}})
        self.assertEqual(sync(api, 10), [])
        self.assertEqual(api.links[10], set())

    def test_ignores_self_reference(self):
        api = FakeGitHub({10: "## Blocked by\n\n- #10"})
        self.assertEqual(sync(api, 10), [])
        self.assertEqual(api.links.get(10, set()), set())

    def test_reports_missing_issue_and_pull_request(self):
        api = FakeGitHub({1: "", 10: "## Blocked by\n\n- #1\n- #404\n- #7"}, pulls={7})
        errors = sync(api, 10)
        self.assertEqual(api.links[10], {1})
        self.assertEqual(len(errors), 2)
        self.assertTrue(any("#404" in e and "Not Found" in e for e in errors))
        self.assertTrue(any("#7" in e and "pull request" in e for e in errors))

    def test_is_idempotent(self):
        api = FakeGitHub({1: "", 10: "## Blocked by\n\n- #1"})
        sync(api, 10)
        sync(api, 10)
        self.assertEqual(api.links[10], {1})


if __name__ == "__main__":
    unittest.main()
