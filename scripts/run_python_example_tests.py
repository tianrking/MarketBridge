#!/usr/bin/env python3
"""Run example tests and expose individual failures to CI annotations."""

import io
import sys
import unittest


def main():
    suite = unittest.defaultTestLoader.discover("examples", pattern="test_*.py")
    output = io.StringIO()
    result = unittest.TextTestRunner(stream=output, verbosity=2).run(suite)
    text = output.getvalue()
    print(text, end="")
    if result.wasSuccessful():
        return 0
    for test, error in result.failures + result.errors:
        detail = " ".join(error.splitlines())
        print(f"::error title=Python strategy example failure::{test}: {detail[:600]}")
    return 1


if __name__ == "__main__":
    sys.exit(main())
