"""MT5 managed-instance pinning; fake terminals, no broker/network calls."""
from pathlib import Path
from tempfile import TemporaryDirectory
from types import SimpleNamespace as N
from unittest import TestCase, main
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "gateway"))
from managed_terminal import ManagedTerminalBinding
from mt5_gateway import broker_snapshot_identity_is_current, mt5_session_alive


class FakeTerminal:
    def __init__(self, directory, data_directory=None):
        self.directory=Path(directory)
        self.info=N(path=str(self.directory),
                    data_path=str(data_directory or self.directory),
                    connected=True)
        self.account=N(login=123,server="Broker-Real",currency="USD",
                       trade_mode=2)
        self.success=True
        self.calls=[]
        self.stops=0
    def initialize(self, *args, **kwargs):
        self.calls.append((args,kwargs))
        return self.success
    def shutdown(self):
        self.stops+=1
    def terminal_info(self):
        return self.info
    def account_info(self):
        return self.account


class ManagedTerminalTests(TestCase):
    def test_old_unmanaged_configuration_still_connects_without_portable(self):
        with TemporaryDirectory() as d:
            m=FakeTerminal(d)
            pinned=ManagedTerminalBinding()
            self.assertTrue(pinned.connect(m))
            self.assertEqual(m.calls,[ ((),{"timeout":10000}) ])
            self.assertEqual(m.stops,0)
            self.assertTrue(pinned.check_current(m))

    def test_exact_executable_and_nonportable_terminal_data_path(self):
        with TemporaryDirectory() as root:
            d=Path(root)/"Broker Terminal";d.mkdir()
            exe=d/"terminal64.exe";exe.write_bytes(b"stub")
            data=Path(root)/"AppData";data.mkdir()
            m=FakeTerminal(d,data)
            binding=ManagedTerminalBinding(str(exe),portable=False)
            self.assertTrue(binding.connect(m))
            self.assertEqual(m.calls[0],((str(exe),),
                             {"timeout":10000,"portable":False}))
            self.assertTrue(binding.check_current(m))
            self.assertEqual(m.stops,0)

    def test_portable_mode_is_explicit_and_binds_to_terminal_folder(self):
        with TemporaryDirectory() as root:
            d=Path(root)/"MT5";d.mkdir()
            exe=d/"terminal64.exe";exe.write_bytes(b"stub")
            m=FakeTerminal(d)
            binding=ManagedTerminalBinding(str(exe),portable=True)
            self.assertTrue(binding.connect(m))
            self.assertEqual(m.calls[0],((str(exe),),
                             {"timeout":10000,"portable":True}))
            m.info.data_path=str(Path(root)/"Another")
            self.assertFalse(binding.check_current(m))
            self.assertEqual(binding.last_error,
                             "portable_data_directory_does_not_match_installation")

    def test_foreign_terminal_is_rejected_after_initialize_without_killing_terminal(self):
        with TemporaryDirectory() as root:
            d=Path(root)/"MT5";d.mkdir()
            exe=d/"terminal64.exe";exe.write_bytes(b"stub")
            foreign=Path(root)/"Unrelated";foreign.mkdir()
            m=FakeTerminal(foreign)
            binding=ManagedTerminalBinding(str(exe))
            self.assertFalse(binding.connect(m))
            self.assertEqual(binding.last_error,
                             "connected_to_different_terminal_installation")
            self.assertEqual(m.stops,1)
            self.assertFalse(binding.check_current(m))
            self.assertEqual(m.stops,1)

    def test_same_terminal_unexpected_data_folder_change_is_not_reused(self):
        with TemporaryDirectory() as root:
            d=Path(root)/"MT5";d.mkdir()
            exe=d/"terminal64.exe";exe.write_bytes(b"stub")
            a=Path(root)/"DataA";a.mkdir()
            other=Path(root)/"DataB";other.mkdir()
            m=FakeTerminal(d,a)
            binding=ManagedTerminalBinding(str(exe))
            self.assertTrue(binding.connect(m))
            m.info.data_path=str(other)
            self.assertFalse(binding.check_current(m))
            self.assertEqual(binding.last_error,"terminal_data_directory_changed")

    def test_second_gateway_to_same_mt5_data_folder_cannot_claim_execution(self):
        with TemporaryDirectory() as root:
            d=Path(root)/"MT5";d.mkdir()
            exe=d/"terminal64.exe";exe.write_bytes(b"stub")
            mt5_a,mt5_b=FakeTerminal(d),FakeTerminal(d)
            first=ManagedTerminalBinding(str(exe))
            second=ManagedTerminalBinding(str(exe))
            try:
                self.assertTrue(first.connect(mt5_a))
                self.assertFalse(second.connect(mt5_b))
                self.assertEqual(second.last_error,
                    "terminal_already_owned_or_lock_unavailable")
                self.assertEqual(mt5_b.stops,1,
                    "Second Gateway must detach Python client, not kill broker terminal")
                self.assertTrue(first.check_current(mt5_a))
                first.release()
                self.assertTrue(second.connect(mt5_b),
                    "After first Gateway completes cleanup, successor may acquire lock")
            finally:
                first.release();second.release()

    def test_separate_terminal_data_folders_can_coexist(self):
        with TemporaryDirectory() as root:
            d=Path(root)/"MT5";d.mkdir()
            exe=d/"terminal64.exe";exe.write_bytes(b"stub")
            d1=Path(root)/"Data-1";d1.mkdir()
            d2=Path(root)/"Data-2";d2.mkdir()
            x,y=ManagedTerminalBinding(str(exe)),ManagedTerminalBinding(str(exe))
            try:
                self.assertTrue(x.connect(FakeTerminal(d,d1)))
                self.assertTrue(y.connect(FakeTerminal(d,d2)))
            finally:
                x.release();y.release()

    def test_missing_or_untrusted_portable_exe_fails_closed(self):
        with TemporaryDirectory() as root:
            self.assertEqual(ManagedTerminalBinding("",True).validate()[0],False)
            invalid=ManagedTerminalBinding(str(Path(root)/"missing.exe"),True)
            self.assertFalse(invalid.validate()[0])
            bad=Path(root)/"not_terminal.exe";bad.write_bytes(b"stub")
            self.assertFalse(ManagedTerminalBinding(str(bad)).validate()[0])

    def test_quote_pulse_requires_matching_installation_account_and_data_folder(self):
        with TemporaryDirectory() as root:
            d=Path(root)/"MT5";d.mkdir()
            exe=d/"terminal64.exe";exe.write_bytes(b"stub")
            m=FakeTerminal(d)
            binding=ManagedTerminalBinding(str(exe),portable=True)
            self.assertTrue(binding.connect(m))
            snapshot={"connected":True,"login":123,"server":"Broker-Real",
                      "currency":"USD","account_trade_mode":2}
            self.assertTrue(broker_snapshot_identity_is_current(m,snapshot,binding))
            self.assertTrue(mt5_session_alive(m,binding))
            m.account.trade_mode=0
            self.assertFalse(broker_snapshot_identity_is_current(m,snapshot,binding))
            m.account.trade_mode=2
            m.info.path=str(Path(root)/"Wrong")
            self.assertFalse(broker_snapshot_identity_is_current(m,snapshot,binding))
            self.assertFalse(mt5_session_alive(m,binding))

if __name__=="__main__":
    main()
