import ctypes
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

s = Path(__file__).resolve().parent
clang = Path('C:/Program Files/LLVM/bin/clang.exe')
p = s / (sys.argv[1] if len(sys.argv) > 1 else 'env-probe')
p.mkdir(exist_ok=False)
src = p / 'minimal.c'
src.write_text('int main(void) { return 0; }\n', encoding='ascii')
buf = ctypes.create_unicode_buffer(32768)
n = ctypes.windll.kernel32.GetShortPathNameW(str(s), buf, len(buf))
short = buf.value if n and n < len(buf) else None
base = os.environ.copy()
cases = [('inherited', {k: base.get(k) for k in ('TMPDIR','TEMP','TMP')}),
         ('windows', {k: str(s) for k in ('TMPDIR','TEMP','TMP')})]
if short:
    cases.append(('short', {k: short for k in ('TMPDIR','TEMP','TMP')}))
records = []
for name, vals in cases:
    for rep in range(2):
        env = base.copy()
        env.update({k: v for k, v in vals.items() if v is not None})
        record = dict(case=name, repeat=rep, variables=vals, cwd=str(s))
        record['directory_checks'] = []
        for key, value in vals.items():
            directory = Path(value)
            check = dict(variable=key, exists=directory.is_dir())
            try:
                f = directory / f'write-check-{name}-{rep}-{key}.txt'
                f.write_text('probe\n', encoding='ascii')
                check['write'] = 'ok'
            except OSError as e:
                check['write'] = str(e)
            record['directory_checks'].append(check)
        out = p / f'{name}-{rep}.exe'
        argv = [str(clang), str(src), '-o', str(out)]
        result = subprocess.run(argv, env=env, cwd=s, capture_output=True)
        record.update(argv=argv, rc=result.returncode,
                      stdout=result.stdout.decode('utf-8', errors='replace'),
                      stderr=result.stderr.decode('utf-8', errors='replace'))
        if out.exists():
            record['exe_sha256'] = hashlib.sha256(out.read_bytes()).hexdigest()
            run = subprocess.run([str(out)], env=env, cwd=s, capture_output=True)
            record['exe_rc'] = run.returncode
        records.append(record)
        print(json.dumps(record, ensure_ascii=False), flush=True)
(p / 'results.json').write_text(json.dumps(records, ensure_ascii=False, indent=2), encoding='utf-8')
(p / 'short-path.txt').write_text(short or '', encoding='utf-8')
