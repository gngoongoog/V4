"""Offline portable import-closure test (no MT5, network or broker orders)."""
from __future__ import annotations
from importlib.machinery import SourceFileLoader
from pathlib import Path
from unittest import TestCase, main
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
packer = SourceFileLoader(
    "braintrade_portable_closure_test",
    str(ROOT / "scripts" / "package-portable.py")
).load_module()


class GatewayPortableClosureTests(TestCase):
    def test_all_local_python_imports_exist_in_the_portable_build(self):
        self.assertGreaterEqual(packer.validate_gateway_import_closure(),15)
        for name in ("managed_terminal.py","hvn_research.py","tp_partial_close.py"):
            self.assertIn(name,packer.GATEWAY_FILES)
            self.assertTrue((ROOT / "gateway" / name).is_file())

    def test_missing_pinned_mt5_module_makes_packaging_fail(self):
        reduced=tuple(x for x in packer.GATEWAY_FILES
                      if x != "managed_terminal.py")
        with patch.object(packer,"GATEWAY_FILES",reduced):
            with self.assertRaisesRegex(ValueError,
                                        "managed_terminal.py"):
                packer.validate_gateway_import_closure()

    def test_missing_tp_or_hvn_module_makes_packaging_fail(self):
        for target in ("tp_partial_close.py","hvn_research.py"):
            with self.subTest(target=target):
                reduced=tuple(x for x in packer.GATEWAY_FILES if x!=target)
                with patch.object(packer,"GATEWAY_FILES",reduced):
                    with self.assertRaisesRegex(ValueError,target):
                        packer.validate_gateway_import_closure()

    def test_static_portable_acceptance_requires_gateway_dependency_files(self):
        from importlib.util import spec_from_file_location, module_from_spec
        spec=spec_from_file_location("portable_acceptance_test",
            ROOT/"scripts"/"check_v25_acceptance_package.py")
        mod=module_from_spec(spec)
        spec.loader.exec_module(mod)
        for name in ("managed_terminal.py","hvn_research.py","tp_partial_close.py"):
            self.assertIn("runtime/gateway/"+name,mod.REQUIRED_TOOLS)

if __name__=="__main__":
    main()
