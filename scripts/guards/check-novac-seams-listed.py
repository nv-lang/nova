# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-seams-listed.py — каждый шов яруса novac стоит в
перечне SEAMS gate-novac.sh, и каждая строка SEAMS кем-то читается (реестр №992).

ПОЧЕМУ ЭТОТ СТРАЖ ПОЯВИЛСЯ (2026-09-06). Производитель вердикта яруса novac
(№988) метит прогон `TIER=novac-sample` РОВНО ПО НЕПУСТОМУ `$SEAMS`: шов, не
попавший в этот список, оставляет метку полного `novac`, и прогон с погашенным
стражем выглядит полным ярусом — он способен открыть слияние в main. Так и
было: `NOVAC_EMISSION=0` гасил страж объёма эмиссии, а в SEAMS его не было.
Список честен ровно настолько, насколько он полон, и полноту держит этот страж,
а не внимание того, кто заведёт шестой шов через месяц.

ЧТО СЧИТАЕТСЯ ШВОМ: чтение переменной `NOVAC_<ИМЯ>` в форме «выключено нулём» —
`${NOVAC_<X>:-1}` — в `scripts/guards/check-novac-*.{sh,py}` и
`scripts/tools/novac-*.sh`. Настройки без такого умолчания (`NOVAC_TIER`,
`NOVAC_JOBS`, `NOVAC_BIN`, `NOVAC_PROVE_DEADLINE`, `NOVAC_SMOKE_CACHE`) — не
швы: они ничего не гасят.
ЧТО СВЕРЯЕТСЯ, в обе стороны: (1) каждый шов, прочитанный стражем, стоит
строкой `[ "${NOVAC_<X>:-1}" = "0" ] && SEAMS="$SEAMS NOVAC_X=0"` в gate-novac.sh;
(2) каждая такая строка имеет читателя среди стражей — иначе это мёртвая метка,
обещающая проверку, которой нет.

ПОЧЕМУ PYTHON: старт процесса дороже самой проверки (П14).

$1 — корень репозитория; $2 — override каталога стражей, $3 — override файла
гейта (швы самотеста).
"""
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-seams-listed"
RE_READ = re.compile(r"\$\{NOVAC_([A-Z_]+):-1\}")
RE_SEAM_LINE = re.compile(r'^\[ "\$\{NOVAC_([A-Z_]+):-1\}" = "0" \] && SEAMS="\$SEAMS NOVAC_\1=0"\s*$')


def seam_readers(guards_dir, tools_dir):
    """{seam: [file, ...]} — who reads each seam in the switch-off form."""
    found = {}
    files = sorted(guards_dir.glob("check-novac-*.sh")) + sorted(guards_dir.glob("check-novac-*.py"))
    if tools_dir.is_dir():
        files += sorted(tools_dir.glob("novac-*.sh"))
    for p in files:
        text = p.read_text(encoding="utf-8", errors="replace")
        for m in RE_READ.finditer(text):
            found.setdefault(m.group(1), []).append(p.name)
    return found


def seams_listed(gate_file):
    listed = set()
    for line in gate_file.read_text(encoding="utf-8", errors="replace").split("\n"):
        m = RE_SEAM_LINE.match(line.strip())
        if m:
            listed.add(m.group(1))
    return listed


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    guards_dir = pathlib.Path(a[2]) if len(a) > 2 else root / "scripts" / "guards"
    gate_file = pathlib.Path(a[3]) if len(a) > 3 else root / "scripts" / "gate-novac.sh"
    tools_dir = root / "scripts" / "tools"
    if not guards_dir.is_dir() or not gate_file.is_file():
        print(f"{NAME} ok: судить нечего (нет {guards_dir} или {gate_file})")
        return 0
    readers = seam_readers(guards_dir, tools_dir)
    listed = seams_listed(gate_file)
    unlisted = sorted(s for s in readers if s not in listed)
    dead = sorted(s for s in listed if s not in readers)
    if unlisted or dead:
        print(f"{NAME}: FAIL — перечень SEAMS в {gate_file.name} расходится со швами стражей (№992)", file=sys.stderr)
        for s in unlisted:
            print(f"  NOVAC_{s}=0 читают {', '.join(readers[s])}, а в SEAMS его НЕТ: прогон с ним получит метку полного", file=sys.stderr)
            print("  яруса `novac` вместо `novac-sample` и способен открыть слияние", file=sys.stderr)
        for s in dead:
            print(f"  NOVAC_{s}=0 стоит в SEAMS, но его не читает ни один страж: мёртвая метка, обещающая проверку,", file=sys.stderr)
            print("  которой нет", file=sys.stderr)
        print('  Форма строки SEAMS: [ "${NOVAC_<X>:-1}" = "0" ] && SEAMS="$SEAMS NOVAC_X=0"; шов — только', file=sys.stderr)
        print("  чтение вида ${NOVAC_<X>:-1} в scripts/guards/check-novac-* и scripts/tools/novac-*.", file=sys.stderr)
        return 1
    print(f"{NAME} ok: швов у стражей {len(readers)}, в SEAMS gate-novac.sh {len(listed)}, расхождений 0 "
          f"({', '.join('NOVAC_' + s for s in sorted(readers))})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
