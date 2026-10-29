"""Cross-language execution identity contract.

A newly added Rust brain must be accepted by the MT5 Python adapter. This test
intentionally fails if the append-only magic range changes on only one side.
"""
from pathlib import Path
import re
import sys
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
        # The Adaptive learner's 410616 magic is reserved but intentionally
        # unowned. The shipped Sniper specialist owns 410617. Check the actual
        # Python allowlist, not merely a contiguous upper bound.
        self.assertEqual(high, 410617)
        self.assertEqual(names[16:], ["sniper_learner"])
        self.assertRegex(self.rust, r'if\s+id\s*==\s*"sniper_learner"\s*\{\s*return\s+Some\(410_617\)')
        allowed = re.search(r"OWNED_MAGICS\s*=\s*frozenset\(range\((\d+),\s*(\d+)\)\)\s*\|\s*\{(\d+)\}", self.python)
        self.assertIsNotNone(allowed, "Gateway must enforce explicit non-contiguous magic whitelist")
        start, stop, sniper = map(int, allowed.groups())
        self.assertEqual(set(range(start,stop)) | {sniper}, set(range(rust_base,rust_base+16)) | {410617})
        self.assertNotIn(410616,set(range(start,stop))|{sniper},
                         "Reserved Adaptive magic must never be accepted without registered owner")
        previous = (
            "system", "gann", "wyckoff", "trend", "momentum", "structure",
            "zones", "dom", "profile", "gann_dom_profile", "ces", "kte",
            "regime", "generated", "liquidity", "trend_pullback",
        )
        self.assertEqual(tuple(names[:len(previous)]), previous,
                         "Existing magic identities must not be reordered or remapped; append new brains only")
        sys.path.insert(0,str(ROOT / "gateway"))
        from execution import BRAIN_MAGIC
        expected={name:rust_base+i for i,name in enumerate(names[:16])}
        expected["sniper_learner"]=410617
        self.assertEqual(BRAIN_MAGIC,expected,"Gateway brain-to-magic mapping must match Rust exactly")


if __name__ == "__main__":
    unittest.main()
