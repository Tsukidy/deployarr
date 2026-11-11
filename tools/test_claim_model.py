"""Tests for claim_model.py. Run with: python3 -m unittest discover -s tools -p 'test_*.py'"""
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from claim_model import CLAIM, RELEASE, WITHDRAW, Model, holder, run_all  # noqa: E402


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
        self.assertGreater(total, 100000)
        self.assertEqual(failures, [])

    def test_check_catches_a_protocol_that_ignores_releases(self):
        states, failure = Model(2, 2, owner=False, honor_release=False).check()
        self.assertIsNotNone(failure)
        self.assertIn("c", failure[0])

    def test_check_catches_a_protocol_without_the_recheck(self):
        states, failure = Model(2, 1, owner=False, recheck=False).check()
        self.assertIsNotNone(failure)
        self.assertIn("a", failure[0])


if __name__ == "__main__":
    unittest.main()
