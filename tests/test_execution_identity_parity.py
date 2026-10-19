"""Cross-language execution identity contract.

A newly added Rust brain must be accepted by the MT5 Python adapter. This test
intentionally fails if the append-only magic range changes on only one side.
"""
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[1]
RUST = ROOT / "crates" / "domain" / "src" / "execution_legacy.rs"
GATEWAY = ROOT / "gateway" / "execution.py"


class ExecutionIdentityParity(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.rust = RUST.read_text(encoding="utf-8")
        cls.python = GATEWAY.read_text(encoding="utf-8")

    def registry(self):
        declaration = re.search(
            r"pub\s+const\s+BRAINS\s*:\s*\[\(&str\s*,\s*&str\)\s*;\s*(\d+)\s*\]\s*=\s*\[",
            self.rust,
        )
        self.assertIsNotNone(declaration, "Rust BRAINS registry not found; revalidate identity contract")
        rows = self.rust[declaration.end():].split("];", 1)[0]
        names = re.findall(r'^\s*\("([a-z][a-z0-9_]*)"\s*,', rows, re.MULTILINE)
        self.assertEqual(len(names), int(declaration.group(1)), "Rust registry length and declared size differ")
        self.assertEqual(len(names), len(set(names)), "Duplicate broker ownership brain ID")
        return names

    def test_gateway_accepts_every_registered_rust_brain_without_extra_reserved_magics(self):
        names = self.registry()
        bounds = re.search(r"^MAGIC_MIN\s*,\s*MAGIC_MAX\s*=\s*(\d+)\s*,\s*(\d+)", self.python, re.MULTILINE)
        self.assertIsNotNone(bounds, "Gateway magic owner bounds missing")
        low, high = map(int, bounds.groups())
        source = re.search(r"\.map\(\|i\|\s*([\d_]+)\s*\+\s*i\s+as\s+u64\)", self.rust)
        self.assertIsNotNone(source, "Rust magic base mapping changed; audit broker ownership before continuing")
        rust_base = int(source.group(1).replace("_", ""))
        self.assertEqual(rust_base, low)
        self.assertEqual(high, rust_base + len(names) - 1,
                         "Added/removed execution brain without synchronizing Python MT5 magic bounds")
        self.assertEqual(names[0], "system", "Existing append-only ownership must not be reordered")
        self.assertEqual(names[-1], "trend_pullback", "Current 16-identity checkpoint changed; manually audit legacy magic migration")

    def test_open_adaptive_pr_must_not_be_assumed_merged(self):
        self.assertNotIn('"adaptive",', self.rust,
                         "If Adaptive becomes a registered broker brain, update identity parity and frozen ownership tests")


if __name__ == "__main__":
    unittest.main()
