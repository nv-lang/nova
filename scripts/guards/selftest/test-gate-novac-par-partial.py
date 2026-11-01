"""Exercise the real pool/trap functions, without running a compiler or gate."""
import pathlib
import re
import subprocess
import sys
import tempfile
import time


def main():
    root = pathlib.Path(__file__).resolve().parents[3]
    # The caller supplies its session scratch (CI supplies RUNNER_TEMP).
    with tempfile.TemporaryDirectory(dir=sys.argv[1]) as tmp:
        work = pathlib.Path(tmp)
        source = (root / "scripts/gate-novac.sh").read_text(encoding="utf-8")
        names = ["par_snapshot", "par_add", "par_run", "_novac_write_verdict"]
        functions = "\n".join(re.search(r"^" + n + r"\(\) \{.*?^\}", source, re.M | re.S)[0] for n in names)
        functions = functions.replace('$ROOT/target/novac-partial', '$W/snapshots')
        for name, body in [("fast", "echo 'ok: fast-output'"), ("slow", "echo slow-partial; sleep 3; echo 'ok: slow-end'"), ("red", "echo red-output; exit 7")]:
            (work / (name + ".sh")).write_text(body + "\n", encoding="utf-8")
        harness = work / "harness.sh"
        harness.write_text("""set -u
ROOT=$1; W=$2; MODE=$3
PAR_DIR="$W/pool-$MODE"; PAR_N=0; NOVAC_JOBS=8
mkdir -p "$PAR_DIR"
PAR_DEADLINE_DEFAULT=10; CAL=1; GATE_T0=$(date +%s)
NOVAC_DONE="$W/verdict-$MODE"; NOVAC_DONE_TREE=""
NOVAC_REACHED_VERDICT=0; NOVAC_VERDICT_HASH=test; NOVAC_VERDICT_BRANCH=test; SEAMS=""
FAILS=0
floor_for() { echo 10; }
fail() { FAILS=$((FAILS+1)); }
desync() { FAILS=$((FAILS+1)); }
is_desync() { return 1; }
gate_lock_release() { echo released > "$W/released-$MODE"; }
""" + functions + """
trap _novac_write_verdict EXIT
trap 'exit 143' TERM
trap 'exit 130' INT
par_add "$W/fast.sh" fast
par_add "$W/slow.sh" slow
if [ "$MODE" != success ]; then par_add "$W/red.sh" red; fi
if [ "$MODE" = cancel ]; then
  parent=$BASHPID
  (sleep 1; kill -TERM "$parent") &
fi
par_run
NOVAC_REACHED_VERDICT=1
[ "$FAILS" = 0 ]
""", encoding="utf-8")
        for mode, expected in [("success", 0), ("failure", 1), ("cancel", 143)]:
            bash = sys.argv[2] if len(sys.argv) > 2 else "bash"
            result = subprocess.run([bash, harness.as_posix(), root.as_posix(), work.as_posix(), mode], capture_output=True, timeout=30)
            assert result.returncode == expected, (mode, result.returncode, result.stderr)
            verdict = (work / ("verdict-" + mode)).read_text()
            assert "RC=" + str(expected) + " " in verdict, verdict
            assert (work / ("released-" + mode)).exists()
            if mode == "cancel":
                snapshots = list((work / "snapshots").glob("pool-cancel"))
                dest = snapshots[0]
                try:
                    for idx in [1, 2, 3]:
                        assert (dest / f"{idx}.start").exists()
                        assert (dest / f"{idx}.out").exists()
                    assert "status=completed" in (dest / "1.status").read_text()
                    assert "status=started" in (dest / "2.status").read_text()
                    assert "rc=7" in (dest / "3.status").read_text()
                    assert (dest / "3.rc").read_text().strip() == "7"
                    assert (dest / "3.sec").exists()
                    assert "slow-partial" in (dest / "2.out").read_text()
                    assert "slow-end" not in (dest / "2.out").read_text()
                finally:
                    import shutil
                    shutil.rmtree(dest)
            print("PASS", mode, "rc=" + str(expected))
        py_pool = work / "py-pool"
        py_pool.mkdir()
        guard = work / "slow.py"
        guard.write_text('import time\ndef main():\n print("python-partial")\n time.sleep(20)\n return 0\n', encoding="utf-8")
        (py_pool / '1.cmd').write_text(guard.as_posix(), encoding='utf-8')
        proc = subprocess.Popen([sys.executable, str(root / 'scripts/guards/run-guards.py'), str(root), str(py_pool), '1'])
        try:
            until = time.monotonic() + 10
            while time.monotonic() < until:
                out = py_pool / '1.out'
                if out.exists() and 'python-partial' in out.read_text():
                    break
                time.sleep(0.05)
            else:
                raise AssertionError('Python partial output was buffered/lost')
            assert (py_pool / '1.start').exists()
            assert not (py_pool / '1.rc').exists()
        finally:
            proc.terminate()
            proc.wait(timeout=10)
        assert 'python-partial' in (py_pool / '1.out').read_text()
        assert not (py_pool / '1.rc').exists()
        guard.write_text('def main():\n print("python-red")\n return 7\n', encoding='utf-8')
        result = subprocess.run([sys.executable, str(root / 'scripts/guards/run-guards.py'), str(root), str(py_pool), '1'], timeout=10)
        assert result.returncode == 1
        assert (py_pool / '1.rc').read_text().strip() == '7'
        assert (py_pool / '1.sec').exists()
        print('PASS Python partial on termination; original guard rc=7, runner rc=1')
    return 0


if __name__ == "__main__":
    sys.exit(main())
