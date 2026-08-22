# -*- coding: utf-8 -*-
"""scripts/tools/gate-tier-audit.py — что лежит в каждом ярусе гейта и может ли
ярус `loop` поднять компилятор.

ЗАЧЕМ. Г4 говорит: проверка, которая ЗАПУСКАЕТ компилятор, в цикл не попадает
НИКОГДА. Это утверждение о составе яруса, и доказывать его рассуждением
нельзя — нужен перечень. Инструмент читает `scripts/gate.sh`, приписывает
каждую исполняемую строку ярусу того шага, к которому она относится, и
отдельно просматривает КАЖДОГО стража, которого ярус зовёт.

ЧТО СЧИТАЕТСЯ ЗАПУСКОМ. Бинарь в позиции команды (`"$NOVA" build`,
`cargo test`, `./nova-cli/target/release/nova check`). Упоминание внутри
`echo`/`printf` и внутри регулярки `grep` запуском НЕ является — на этих двух
формах первая редакция и попалась: `check-driver-channel-parity` печатает слово
«nova build» в своей строке `ok:`, а `check-background-build-verified` ИЩЕТ
`cargo build` регуляркой. Оба — чистый текст, и отнести их к запуску значило бы
дать ложную красноту, которая дороже отсутствующей проверки.

ИСПОЛЬЗОВАНИЕ:
  python scripts/tools/gate-tier-audit.py [КОРЕНЬ]
Код возврата: 0 — в `loop` запусков нет; 1 — есть, и они перечислены.

План: docs/plans/275-gate-cost.md, Ф.2/Ф.3.
"""
import io
import pathlib
import re
import sys

EXEC = re.compile(
    r'(^|[;&|(]\s*|\s)(("?\$\{?NOVA\b[^"]*"?)|(\./)?nova(-cli)?(\.exe)?|cargo)\s+'
    r'(build|test|check|lint|run|doc|fmt)\b')
SAYS = re.compile(r'^\s*(echo|printf)\b')
GUARD_REF = re.compile(r"scripts/guards/([a-z0-9._-]+\.(?:sh|py))")


def real_call(line):
    """Запуск, а не упоминание: не печать и не образец для grep."""
    if SAYS.match(line):
        return False
    m = EXEC.search(line)
    if not m:
        return False
    head = line[:m.start()]
    if re.search(r"\bgrep\b|\bawk\b|\bsed\b", head):
        return False
    return True


def code_lines(path):
    text = io.open(path, encoding="utf-8", errors="replace").read()
    for i, line in enumerate(text.replace("\r", "").split("\n"), 1):
        s = line.strip()
        if s and not s.startswith("#"):
            yield i, line


def main():
    root = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else ".").resolve()
    gate = root / "scripts" / "gate.sh"
    if not gate.is_file():
        print(f"gate-tier-audit: нет {gate}", file=sys.stderr)
        return 1

    tier = None
    guards = {"loop": set(), "push": set(), "full": set()}
    inline = {"loop": [], "push": [], "full": []}
    steps = {"loop": 0, "push": 0, "full": 0}
    for n, line in enumerate(io.open(gate, encoding="utf-8").read()
                             .replace("\r", "").split("\n"), 1):
        if line.startswith("step "):
            tier = line.split()[1]
            if tier in steps:
                steps[tier] += 1
            continue
        if tier is None:
            continue
        s = line.strip()
        if not s or s.startswith("#"):
            continue
        m = GUARD_REF.search(line)
        if m:
            guards[tier].add(m.group(1))
        if real_call(line):
            inline[tier].append((n, s[:90]))

    # Ярусы накопительные: шаг яруса loop идёт и в push, и в full.
    order = ["loop", "push", "full"]
    for i, t in enumerate(order):
        acc_steps = sum(steps[x] for x in order[:i + 1])
        print(f"{t}: шагов {acc_steps} (своих {steps[t]}), стражей своих {len(guards[t])}, "
              f"запусков компилятора в теле гейта {len(inline[t])}")

    dirty = []
    for g in sorted(guards["loop"]):
        p = root / "scripts" / "guards" / g
        if not p.is_file():
            print(f"  ОТСУТСТВУЕТ страж {g}", file=sys.stderr)
            continue
        hits = [(n, l.strip()[:90]) for n, l in code_lines(p) if real_call(l)]
        if hits:
            dirty.append((g, hits))

    print()
    print("ЯРУС loop, стражи по именам:")
    for g in sorted(guards["loop"]):
        print(f"  {g}")

    if inline["loop"] or dirty:
        print()
        print("gate-tier-audit: FAIL — ярус loop поднимает компилятор:", file=sys.stderr)
        for n, s in inline["loop"]:
            print(f"  gate.sh:{n} {s}", file=sys.stderr)
        for g, hits in dirty:
            for n, s in hits[:4]:
                print(f"  {g}:{n} {s}", file=sys.stderr)
        return 1

    print()
    print(f"gate-tier-audit ok: ярус loop — {len(guards['loop'])} стражей и ноль запусков "
          f"компилятора (Г4)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
