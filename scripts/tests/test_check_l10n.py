import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from check_l10n import LOCALES, problems


class CheckL10nTest(unittest.TestCase):
    def write(self, folder, locale, content):
        (folder / f"app_{locale}.arb").write_text(json.dumps(content))

    def test_reports_missing_and_unknown_keys(self):
        with tempfile.TemporaryDirectory() as name:
            folder = Path(name)
            self.write(folder, "en", {"a": "A", "b": "B", "@b": {}})
            for locale in LOCALES:
                self.write(folder, locale, {"a": "x", "b": "y"})
            self.assertEqual(problems(folder), [])
            self.write(folder, "fr", {"a": "x", "c": "z"})
            self.assertEqual(problems(folder), ["fr: missing b", "fr: unknown c"])


if __name__ == "__main__":
    unittest.main()
