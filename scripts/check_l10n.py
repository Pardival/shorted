#!/usr/bin/env python3
"""Fails when a translation lacks a key of the English template, or has one it should not."""

import json
import sys
from pathlib import Path

LOCALES = ("fr", "es", "pt")


def keys(path):
    return {key for key in json.loads(path.read_text()) if not key.startswith("@")}


def problems(folder):
    template = keys(folder / "app_en.arb")
    found = []
    for locale in LOCALES:
        translated = keys(folder / f"app_{locale}.arb")
        found += [f"{locale}: missing {key}" for key in sorted(template - translated)]
        found += [f"{locale}: unknown {key}" for key in sorted(translated - template)]
    return found


def main():
    folder = Path(__file__).resolve().parent.parent / "app" / "lib" / "l10n"
    found = problems(folder)
    print("\n".join(found) or "translations complete")
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main())
