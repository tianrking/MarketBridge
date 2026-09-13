#!/usr/bin/env python3
"""Guard the categorized Python strategy catalog and bilingual documentation."""

from pathlib import Path
import unittest


EXAMPLES = Path(__file__).resolve().parent
CRYPTO = EXAMPLES / "crypto"
CATEGORIES = ("carry", "defi", "microstructure", "onchain", "options", "universe")


class StrategyCatalogTests(unittest.TestCase):
    def test_every_crypto_category_has_bilingual_readme(self):
        for category in CATEGORIES:
            readme = CRYPTO / category / "README.md"
            self.assertTrue(readme.is_file(), f"missing README for {category}")
            text = readme.read_text(encoding="utf-8")
            self.assertIn("## English", text, category)
            self.assertIn("## 中文", text, category)
            self.assertIn("## Commands / 命令", text, category)

    def test_every_categorized_python_entrypoint_is_documented(self):
        root_index = (EXAMPLES / "README.md").read_text(encoding="utf-8")
        for category in CATEGORIES:
            family_readme = (CRYPTO / category / "README.md").read_text(encoding="utf-8")
            for path in sorted((CRYPTO / category).glob("*.py")):
                if path.name == "__init__.py":
                    continue
                self.assertTrue(
                    path.name in root_index or path.name in family_readme,
                    f"undocumented strategy entrypoint: {path.relative_to(EXAMPLES)}",
                )

    def test_examples_contains_no_rust_strategy_files(self):
        self.assertEqual(list(EXAMPLES.rglob("*.rs")), [])


if __name__ == "__main__":
    unittest.main()
