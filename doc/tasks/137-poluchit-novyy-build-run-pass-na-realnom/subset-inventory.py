"""Record the guard's own subset counters without changing its baseline."""
import json
import pathlib
import subprocess
import sys

root = pathlib.Path.cwd()
sys.path.insert(0, str(root / "scripts/guards"))
import novac_subset_debt as debt

reference = "e909cb960"
names = subprocess.check_output(["git", "ls-tree", "-r", "--name-only", reference, "novac/src"], text=True).splitlines()
before = {p.removeprefix("novac/src/"): subprocess.check_output(["git", "show", reference + ":" + p]).decode("utf-8")
          for p in names if p.endswith(".nv") and not p.endswith("_test.nv")}
after = {p.relative_to(root / "novac/src").as_posix(): p.read_text(encoding="utf-8")
         for p in (root / "novac/src").rglob("*.nv") if not p.name.endswith("_test.nv")}


def inventory(files):
    rows = []
    for path, text in sorted(files.items()):
        for line, value in enumerate(text.splitlines(), 1):
            if value.lstrip().startswith("//"):
                continue
            m = debt.RE_DEBT.search(value)
            if m:
                rows.append(dict(path=path, line=line, text=m.group(0), dated=bool(debt.RE_STAGE.search(m.group(0)))))
    subset, language = debt.by_door(files)
    return dict(refusals=len(rows), undated=sum(not r["dated"] for r in rows),
                undated_by_door=sum(not r[3] for r in subset), rows=rows,
                subset_by_door=subset, language_by_door=language)


result = dict(reference=reference, before=inventory(before), after=inventory(after))
target = pathlib.Path(sys.argv[1])
target.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding="utf-8")
for phase in ("before", "after"):
    print(phase + " " + json.dumps({k: result[phase][k] for k in ("refusals", "undated", "undated_by_door")}))
