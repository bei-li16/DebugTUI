"""Check actual pinned conversion and reject unsupported/ambiguous input expressions."""
import json
import subprocess
import sys
import unittest
from pathlib import Path
from cmsis_registers import Header, INPUT, deltas, common


class Conversion(unittest.TestCase):
    def setUp(self):
        self.header = Header("core_cm3.h", json.loads((INPUT/"source-lock.json").read_text()))

    def test_pinned_headers_and_shipped_output_match_offline(self):
        root = Path(__file__).resolve().parents[1]
        result = subprocess.run([sys.executable,str(root/"scripts/generate-register-catalogues.py"),"--check"],
                                cwd=root,capture_output=True,text=True,check=False)
        self.assertEqual(result.returncode,0,result.stderr)
        self.assertIn("6 catalogues match offline regeneration",result.stdout)

    def test_only_finite_integer_operations_are_accepted(self):
        self.assertEqual(self.header.number("SCB_CPUID_PARTNO_Msk"),0xfff0)
        for expression in ["__import__('os')", "1 / 2", "1 << 999", "unknown_name", "[1][0]"]:
            with self.assertRaises(ValueError):
                self.header.number(expression)
        self.header.macros["CYCLE"]=("CYCLE",1)
        with self.assertRaises(ValueError):
            self.header.number("CYCLE")

    def test_cpacr_fields_have_m_profile_manual_evidence_and_no_writer(self):
        lock=json.loads((INPUT/"source-lock.json").read_text())
        for model in (4,7):
            header=Header(f"core_cm{model}.h",lock)
            definition=next(r for r in deltas(header,common(self.header)) if r["id"]=="scb.cpacr")
            self.assertEqual(definition["reader"],dict(kind="core_private",address=0xE000ED88))
            self.assertEqual([(f["name"],f["segments"]) for f in definition["fields"]],
                             [("CP10",[dict(offset=20,width=2)]),("CP11",[dict(offset=22,width=2)])])
            self.assertEqual([e["value"] for e in definition["fields"][0]["enums"]],["0","1","2","3"])
            self.assertEqual(definition["source"]["number"],"DUI 0553A" if model==4 else "DUI 0646B")
            self.assertGreater(definition["source"]["page"],0)
            self.assertNotIn("writer",definition)
            self.assertNotIn("reset",definition)

    def test_conditional_conflicts_and_unknown_field_overlap_fail_closed(self):
        with self.assertRaisesRegex(ValueError,"Conditional CMSIS macro"):
            self.header.number("SCB_VTOR_TBLOFF_Msk")
        self.header.macros.update({"TEST_R_A_Pos":("0",1),"TEST_R_A_Msk":("3",1),
                                  "TEST_R_B_Pos":("1",1),"TEST_R_B_Msk":("2",1)})
        with self.assertRaisesRegex(ValueError,"Overlapping CMSIS fields"):
            self.header.fields("TEST_R")
        lock=json.loads((INPUT/"source-lock.json").read_text())
        lock["files"]["core_cm3.h"]="0"*64
        with self.assertRaisesRegex(ValueError,"Pinned CMSIS input changed"):
            Header("core_cm3.h",lock)


if __name__ == "__main__":
    unittest.main()
