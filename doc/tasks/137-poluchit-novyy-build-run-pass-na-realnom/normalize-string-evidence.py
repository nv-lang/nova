"""Normalize machine prefixes in published evidence; retain untouched raw bytes.

Arguments: session scratch, evidence label. Artifact/source/exe hashes continue
to describe the ORIGINAL bytes; evidence hashes describe the normalized files.
"""
import hashlib
import json
import pathlib
import shutil
import sys

s = pathlib.Path(sys.argv[1]).resolve()
label = sys.argv[2]
root = pathlib.Path.cwd()
dest = root / "doc/tasks/137-poluchit-novyy-build-run-pass-na-realnom/evidence" / label
raw = s / ("raw-evidence-" + label)
shutil.copytree(dest, raw)
prefixes = [(str(s), "$SCRATCH"), (str(root), "$WT"), (str(pathlib.Path.home()), "$HOME")]
for p in raw.rglob("*native-command.json"):
    command = json.loads(p.read_text(encoding="utf-8"))
    if command:
        prefixes.append((command[0], "$CLANG"))
prefixes.sort(key=lambda item: len(item[0]), reverse=True)


def normalize_text(text):
    for old, new in prefixes:
        for spelling in (json.dumps(old, ensure_ascii=True)[1:-1], json.dumps(old, ensure_ascii=False)[1:-1], old, old.replace("\\", "/")):
            text = text.replace(spelling, new)
    return text


def normalize_json(value):
    if isinstance(value, str):
        return normalize_text(value)
    if isinstance(value, list):
        return [normalize_json(v) for v in value]
    if isinstance(value, dict):
        return {normalize_text(k): normalize_json(v) for k, v in value.items()}
    return value


raw_hashes = {}
for source in sorted(raw.rglob("*")):
    if not source.is_file():
        continue
    relative = source.relative_to(raw).as_posix()
    data = source.read_bytes()
    raw_hashes[relative] = hashlib.sha256(data).hexdigest()
    text = data.decode("utf-8")
    if source.suffix == ".json":
        text = json.dumps(normalize_json(json.loads(text)), ensure_ascii=False, indent=2) + "\n"
    else:
        text = normalize_text(text)
    (dest / relative).write_bytes(text.encode("utf-8"))
artifact = dest / "artifact-manifest.json"
record = json.loads(artifact.read_text(encoding="utf-8"))
record["evidence_sha256"] = {p.relative_to(dest).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest()
                             for p in sorted(dest.rglob("*")) if p.is_file() and p != artifact}
record["normalization"] = "Machine prefixes only: $SCRATCH, $WT, $HOME, $CLANG. JSON is decoded, normalized and reserialized as UTF-8 with indent=2. Other text retains original line endings. Artifact hashes still name raw binary/source/C bytes. Raw evidence is retained in the session scratch under raw-evidence-" + label
artifact.write_text(json.dumps(record, ensure_ascii=False, indent=2) + "\n", encoding="utf-8", newline="\n")
normalized_hashes = {p.relative_to(dest).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest()
                     for p in sorted(dest.rglob("*")) if p.is_file()}
(dest / "normalization.json").write_text(json.dumps(dict(raw_sha256=raw_hashes, published_sha256=normalized_hashes), indent=2) + "\n", encoding="utf-8", newline="\n")
print("Normalized evidence; raw copy retained under session scratch: " + label)
