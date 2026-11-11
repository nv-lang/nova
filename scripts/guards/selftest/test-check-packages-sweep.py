from pathlib import Path
import os
import shutil
import subprocess
import tempfile
import unittest

GUARD = Path(__file__).resolve().parents[1] / "check-packages-sweep.sh"
BASH = str(Path(shutil.which("git")).parent.parent / "bin/bash.exe") if os.name == "nt" else "bash"

class SweepVerdictTest(unittest.TestCase):
    def run_guard(self, text, code, with_package=True):
        with tempfile.TemporaryDirectory(prefix="nova-sweep-verdict-") as tmp:
            root = Path(tmp) / "root"
            packages = Path(tmp) / "packages"
            binary = root / "nova-cli/target/release/nova.exe"
            binary.parent.mkdir(parents=True)
            packages.mkdir()
            binary.write_text("#!/bin/sh\nprintf '%s\\n' '" + text + "'\nexit " + str(code) + "\n", encoding="utf-8")
            binary.chmod(0o755)
            if with_package:
                package = packages / "nova-probe"
                (package / "src").mkdir(parents=True)
                (package / "nova.toml").write_text('[package]\nname="probe"\nversion="0.0.0"\n', encoding="utf-8")
            return subprocess.run([BASH, GUARD.as_posix(), root.as_posix(), packages.as_posix()], capture_output=True, text=True)

    def test_real_success(self):
        self.assertEqual(self.run_guard("PASS: 12  FAIL: 0  SKIP: 1", 0).returncode, 0)

    def test_failure_is_not_no_packages(self):
        self.assertNotEqual(self.run_guard("CODEGEN-FAIL", 1).returncode, 0)

    def test_success_text_does_not_override_exit(self):
        self.assertNotEqual(self.run_guard("PASS: 12  FAIL: 0", 1).returncode, 0)

    def test_empty_suite_is_not_success(self):
        self.assertNotEqual(self.run_guard("PASS: 0  FAIL: 0", 0).returncode, 0)

    def test_nonzero_failure_count(self):
        self.assertNotEqual(self.run_guard("PASS: 12  FAIL: 01", 0).returncode, 0)

    def test_absent_packages_remain_observable(self):
        result = self.run_guard("", 0, with_package=False)
        self.assertEqual(result.returncode, 0)
        self.assertIn("check-packages-sweep", result.stdout)

if __name__ == "__main__":
    unittest.main()
