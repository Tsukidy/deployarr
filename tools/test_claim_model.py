"""Tests for claim_model.py. Run with: python3 -m unittest discover -s tools -p 'test_*.py'"""
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from claim_model import CLAIM, HELD_BY_GONE, RELEASE, WITHDRAW, Model, holder, run_all  # noqa: E402


class HolderTest(unittest.TestCase):
    def test_first_claim_holds(self):
        self.assertEqual(holder([(CLAIM, "a"), (CLAIM, "b")]), "a")

    def test_release_frees_and_later_claim_holds(self):
        self.assertEqual(holder([(CLAIM, "a"), (CLAIM, "b"), (RELEASE, "a")]), None)
        self.assertEqual(holder([(CLAIM, "a"), (RELEASE, "a"), (CLAIM, "b")]), "b")

    def test_withdraw_and_foreign_release_change_nothing(self):
        self.assertEqual(holder([(CLAIM, "a"), (WITHDRAW, "a"), (RELEASE, "b")]), "a")


class ProtocolTest(unittest.TestCase):
    def test_documented_protocol_holds_every_property(self):
        total, failures = run_all()
        self.assertGreater(total, 1000000)
        self.assertEqual(failures, [])

    def assertCaught(self, props, **variant):
        """The checker must find a violation of one of `props` once a safeguard is removed."""
        states, failure = Model(2, 2, owner=False, **variant).check()
        self.assertIsNotNone(failure, f"no violation found with {variant}")
        self.assertTrue(set(props) & set(failure[0]), f"found {failure[0]}, expected one of {props}")

    def test_catches_ignoring_releases(self):
        self.assertCaught("bc", honor_release=False)

    def test_catches_a_missing_comment_recheck(self):
        self.assertCaught("a", recheck=False)

    def test_catches_a_missing_issue_recheck(self):
        self.assertCaught("d", verify=False)

    def test_catches_releasing_before_the_relabel(self):
        self.assertCaught("ad", relabel_first=False)

    def test_catches_commenting_before_the_unassign(self):
        self.assertCaught("e", unassign_first=False)

    def test_catches_a_lost_claim_that_keeps_a_stale_assignee(self):
        model = Model(2, 1, owner=False, log=HELD_BY_GONE, assigned=True, blocked=True, named=True,
                      verify_unassign=False)
        states, failure = model.check()
        self.assertIsNotNone(failure)
        self.assertIn("b", failure[0])


if __name__ == "__main__":
    unittest.main()
