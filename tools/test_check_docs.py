"""Tests for the agent file budget in check_docs.py. Run with: python3 -m unittest discover -s tools -p 'test_*.py'"""
import contextlib
import io
import os
import shutil
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import check_docs  # noqa: E402
from check_docs import AGENT_FILES_BUDGET  # noqa: E402

ADVICE = (
    "Move task-specific content to its task's doc, behind a one-line link,"
    " or ask the owner to raise AGENT_FILES_BUDGET in tools/check_docs.py."
)


def native(path):
    return os.path.join(*path.split("/"))


class AgentFilesTest(unittest.TestCase):
    def setUp(self):
        self.root = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, self.root)

    def write(self, path, text):
        full = os.path.join(self.root, native(path))
        os.makedirs(os.path.dirname(full), exist_ok=True)
        with open(full, "w", encoding="utf-8", newline="") as handle:
            handle.write(text)
        return len(text.encode("utf-8"))

    def base(self, claude="@AGENTS.md\n"):
        """Write AGENTS.md and a CLAUDE.md that holds `claude`, and return {path: bytes} for both."""
        return {"AGENTS.md": self.write("AGENTS.md", "# Agents\n"), "CLAUDE.md": self.write("CLAUDE.md", claude)}

    def sizes(self):
        sizes, missing = check_docs.agent_files(self.root)
        self.assertEqual(missing, [])
        return sizes

    def assertSizes(self, expected):
        self.assertEqual(self.sizes(), {native(path): size for path, size in expected.items()})

    def run_main(self):
        """Run the whole docs check on the temporary tree and return (exit code, output)."""
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = check_docs.main(self.root)
        return code, out.getvalue()

    def test_a_tree_under_the_budget_passes_and_prints_the_total(self):
        expected = self.base()
        total = sum(expected.values())
        self.assertEqual(check_docs.check_agent_files(self.root), (total, []))
        code, output = self.run_main()
        self.assertEqual(code, 0, output)
        self.assertIn(f"always-loaded agent files: {total:,} of {AGENT_FILES_BUDGET:,} bytes\n", output)

    def test_a_tree_at_the_budget_passes(self):
        claude = self.write("CLAUDE.md", "@AGENTS.md\n")
        self.write("AGENTS.md", "x" * (AGENT_FILES_BUDGET - claude))
        self.assertEqual(check_docs.check_agent_files(self.root), (AGENT_FILES_BUDGET, []))

    def test_a_tree_over_the_budget_fails_with_the_budget_the_total_and_each_size(self):
        claude = self.write("CLAUDE.md", "@AGENTS.md\n")
        agents = self.write("AGENTS.md", "x" * (AGENT_FILES_BUDGET - claude - 50))
        rule = self.write(".claude/rules/style.md", "y" * 51)
        total = agents + claude + rule
        self.assertEqual(total, AGENT_FILES_BUDGET + 1)
        message = "\n".join([
            f"always-loaded agent files: {total:,} bytes, over the budget of {AGENT_FILES_BUDGET:,} bytes",
            f"  {agents:,}  AGENTS.md",
            f"  {claude:>{len(f'{agents:,}')}}  CLAUDE.md",
            f"  {rule:>{len(f'{agents:,}')}}  {native('.claude/rules/style.md')}",
            "  " + ADVICE,
        ])
        self.assertEqual(check_docs.check_agent_files(self.root), (total, [message]))
        code, output = self.run_main()
        self.assertEqual(code, 1)
        self.assertIn(message + "\n", output)
        self.assertNotIn(f"{total:,} of {AGENT_FILES_BUDGET:,} bytes", output)
        self.assertTrue(output.endswith("1 problems\n"), output)

    def test_a_missing_agents_or_claude_file_is_an_error(self):
        for present in ("AGENTS.md", "CLAUDE.md"):
            with self.subTest(present=present):
                for name in ("AGENTS.md", "CLAUDE.md"):
                    if os.path.exists(os.path.join(self.root, name)):
                        os.remove(os.path.join(self.root, name))
                size = self.write(present, "# Agents\n")
                missing = "CLAUDE.md" if present == "AGENTS.md" else "AGENTS.md"
                self.assertEqual(
                    check_docs.check_agent_files(self.root),
                    (size, [f"{missing}: always-loaded agent file is missing"]),
                )
                code, output = self.run_main()
                self.assertEqual(code, 1)
                self.assertIn(f"{missing}: always-loaded agent file is missing\n", output)

    def test_counts_dot_claude_claude_md(self):
        expected = self.base()
        expected[".claude/CLAUDE.md"] = self.write(".claude/CLAUDE.md", "Project notes.\n")
        self.assertSizes(expected)

    def test_counts_an_import_and_its_imports_relative_to_the_importer(self):
        expected = self.base("@AGENTS.md\nSee @docs/guide.md for the workflow.\n")
        expected["docs/guide.md"] = self.write("docs/guide.md", "- details @parts/more.md\n")
        expected["docs/parts/more.md"] = self.write("docs/parts/more.md", "More.\n")
        # A file at the root with the same name is not the one the guide imports.
        self.write("parts/more.md", "Not imported.\n")
        self.assertSizes(expected)

    def test_counts_an_import_whose_path_holds_an_escaped_space(self):
        expected = self.base("@AGENTS.md @Design\\ Docs/api.md\n")
        expected["Design Docs/api.md"] = self.write("Design Docs/api.md", "API.\n")
        self.assertSizes(expected)

    def test_skips_imports_in_code_fences_and_code_spans(self):
        expected = self.base(
            "@AGENTS.md\n"
            "```text\n@docs/fenced.md\n```\n"
            "~~~~\n@docs/tilde.md\n~~~~\n"
            "Write `@docs/span.md` to name a file without importing it.\n"
        )
        for name in ("fenced", "tilde", "span"):
            self.write(f"docs/{name}.md", "Not imported.\n")
        self.assertSizes(expected)

    def test_skips_text_that_is_not_an_import(self):
        expected = self.base(
            "@AGENTS.md\n"
            "Mail noreply@docs/mail.md is not an import.\n"
            'A quoted @"docs/quoted.md" is not imported.\n'
            "@docs/missing.md names no file. @~/home.md and @../outside.md leave the repository.\n"
        )
        for name in ("mail", "quoted"):
            self.write(f"docs/{name}.md", "Not imported.\n")
        self.assertSizes(expected)

    def test_counts_each_file_once(self):
        # CLAUDE.md imports AGENTS.md, which the set holds anyway, and two files import shared.md.
        expected = self.base("@AGENTS.md @a.md @shared.md\n")
        expected["a.md"] = self.write("a.md", "@shared.md @CLAUDE.md @AGENTS.md\n")
        expected["shared.md"] = self.write("shared.md", "@a.md\n")
        self.assertSizes(expected)

    def test_follows_imports_up_to_four_hops(self):
        expected = self.base("@AGENTS.md @hop1.md\n")
        for hop in range(1, 5):
            expected[f"hop{hop}.md"] = self.write(f"hop{hop}.md", f"@hop{hop + 1}.md\n")
        self.write("hop5.md", "Five hops from CLAUDE.md.\n")
        self.assertSizes(expected)

    def test_counts_a_file_at_its_fewest_hops(self):
        # hop5.md is five hops from CLAUDE.md, too far to follow its imports, but two hops from AGENTS.md.
        expected = {"CLAUDE.md": self.write("CLAUDE.md", "@AGENTS.md @hop1.md\n")}
        expected["AGENTS.md"] = self.write("AGENTS.md", "@near.md\n")
        expected["near.md"] = self.write("near.md", "@hop5.md\n")
        for hop in range(1, 6):
            expected[f"hop{hop}.md"] = self.write(f"hop{hop}.md", f"@hop{hop + 1}.md\n")
        expected["hop6.md"] = self.write("hop6.md", "Three hops from AGENTS.md.\n")
        self.assertSizes(expected)

    def test_counts_rules_without_paths_and_skips_rules_with_paths(self):
        expected = self.base()
        expected[".claude/rules/style.md"] = self.write(".claude/rules/style.md", "Use short sentences.\n")
        expected[".claude/rules/deep/ci.md"] = self.write(
            ".claude/rules/deep/ci.md", "---\ndescription: CI rules\n---\nKeep CI green. @../../../docs/ci.md\n"
        )
        expected["docs/ci.md"] = self.write("docs/ci.md", "CI details.\n")
        self.write(".claude/rules/rust.md", '---\npaths:\n  - "crates/**/*.rs"\n---\nRust rules. @../../docs/rust.md\n')
        self.write(".claude/rules/deep/py.md", "---\npaths: tools/**/*.py\n---\nPython rules.\n")
        self.write(".claude/rules/notes.txt", "Not a rule.\n")
        self.write("docs/rust.md", "Imported only by a scoped rule.\n")
        self.assertSizes(expected)

    def test_counts_the_listing_entry_of_each_skill(self):
        expected = self.base()
        self.write(
            ".claude/skills/deploy/SKILL.md",
            "---\n"
            "name: deploy\n"
            "description: Deploy the app.\n"
            "  Use when asked to ship.\n"
            "when_to_use: \"Say ship it.\"\n"
            "allowed-tools: Bash\n"
            "---\n"
            "# Deploy\n\nA long body that loads only when the skill runs. @docs/never.md\n",
        )
        self.write(".claude/skills/review/SKILL.md", "---\ndescription: >\n  Review a change.\n---\n\nBody.\n")
        self.write(".claude/skills/plain/SKILL.md", "\n# Plain skill\n\nBody.\n")
        self.write(
            ".claude/skills/manual/SKILL.md",
            "---\nname: manual\ndescription: Hidden.\ndisable-model-invocation: true\n---\nBody.\n",
        )
        self.write(".claude/skills/empty/notes.md", "No SKILL.md here.\n")
        self.write("docs/never.md", "A skill body import loads only with the skill.\n")
        for path, listing in (
            ("deploy", "deploy\nDeploy the app. Use when asked to ship.\nSay ship it."),
            ("manual", "manual"),
            ("plain", "plain\n# Plain skill"),
            ("review", "review\nReview a change."),
        ):
            expected[f".claude/skills/{path}/SKILL.md (listing)"] = len(listing.encode("utf-8"))
        self.assertSizes(expected)

    def test_frontmatter_must_start_the_file(self):
        self.assertEqual(check_docs.frontmatter("text\n---\npaths: a\n---\n"), ({}, "text\n---\npaths: a\n---\n"))
        self.assertEqual(check_docs.frontmatter("---\npaths: 'a'\n---\nbody\n"), ({"paths": "a"}, "\nbody\n"))


class RepositoryAgentFilesTest(unittest.TestCase):
    def test_the_repository_set_holds_the_agent_files(self):
        root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
        sizes, missing = check_docs.agent_files(root)
        self.assertEqual(missing, [])
        self.assertIn("AGENTS.md", sizes)
        self.assertIn("CLAUDE.md", sizes)


if __name__ == "__main__":
    unittest.main()
