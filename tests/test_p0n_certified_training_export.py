"""Offline: real training may not consume legacy account identities or partial exports."""
from __future__ import annotations
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

ROOT=Path(__file__).resolve().parents[1]
SPEC=importlib.util.spec_from_file_location("trainer_p0n",ROOT/"scripts/train_linear_baseline.py")
trainer=importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(trainer)
FIXTURE=json.loads((ROOT/"tests/learning_feature_parity.json").read_text(encoding="utf-8"))["features"]

class CertifiedDatasetTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory(prefix="braintrade_p0n_")
        self.path=Path(self.temp.name)
        self.row={"context":json.dumps(["Broker-Test",17,"XAUUSD","USD",0,True]),
                  "bar_time":600,"snapshot_id":1,
                  "feature_schema":trainer.FEATURE_SCHEMA,
                  "label_schema":trainer.LABEL_SCHEMA,
                  "features":copy.deepcopy(FIXTURE),
                  "label":{"schema":trainer.LABEL_SCHEMA,
                           "horizons":[{"minutes":15,"direction_0_2atr":1}]}}
        self.manifest={"version":"learning-foundation-v1",
                       "feature_schema":trainer.FEATURE_SCHEMA,
                       "label_schema":trainer.LABEL_SCHEMA,
                       "rows":1,"trained_model_included":False}
        self.write()
    def tearDown(self):self.temp.cleanup()
    def write(self):
        (self.path/"dataset.jsonl").write_text(json.dumps(self.row)+"\n",encoding="utf-8")
        (self.path/"manifest.json").write_text(json.dumps(self.manifest),encoding="utf-8")
    def test_valid_demo_and_real_mode_separately_certified(self):
        _,sha,rows=trainer.load_dataset(self.path,15,certified=True)
        self.assertEqual(len(sha),64);self.assertEqual(rows[0]["y"],1)
        self.row["context"]=json.dumps(["Broker-Test",17,"XAUUSD","USD",2,False]);self.write()
        _,_,rows=trainer.load_dataset(self.path,15,certified=True)
        self.assertEqual(len(rows),1)
    def test_legacy_mode_ambiguous_context_rejected(self):
        self.row["context"]=json.dumps(["Broker-Test",17,"XAUUSD","USD"]);self.write()
        with self.assertRaisesRegex(ValueError,"Demo/REAL context"):
            trainer.load_dataset(self.path,15,certified=True)
    def test_cross_mode_or_untyped_identity_rejected(self):
        for mode,demo in ((0,False),(2,True),(False,True),(0,1)):
            with self.subTest(mode=mode,demo=demo):
                self.row["context"]=json.dumps(["Broker-Test",17,"XAUUSD","USD",mode,demo]);self.write()
                with self.assertRaisesRegex(ValueError,"Demo/REAL context"):
                    trainer.load_dataset(self.path,15,certified=True)
    def test_missing_manifest_rejected_before_training(self):
        (self.path/"manifest.json").unlink()
        with self.assertRaisesRegex(ValueError,"adjacent learning export manifest"):
            trainer.load_dataset(self.path,15,certified=True)
    def test_tampered_manifest_count_or_schema_rejected(self):
        self.manifest["rows"]=2;self.write()
        with self.assertRaisesRegex(ValueError,"manifest row count"):
            trainer.load_dataset(self.path,15,certified=True)
        self.manifest["rows"]=1
        self.manifest["label_schema"]="old-m1-label";self.write()
        with self.assertRaisesRegex(ValueError,"manifest schema"):
            trainer.load_dataset(self.path,15,certified=True)
    def test_synthetic_ablation_research_remains_separate(self):
        self.row["context"]="synthetic-ablation-context";self.write()
        # Only offline helper tests may opt out; the trainer CLI never does.
        _,_,rows=trainer.load_dataset(self.path,15)
        self.assertEqual(len(rows),1)
        with self.assertRaisesRegex(ValueError,"Demo/REAL context"):
            trainer.load_dataset(self.path,15,certified=True)

if __name__=="__main__":unittest.main()
