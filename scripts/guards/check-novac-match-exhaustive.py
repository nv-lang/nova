# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-match-exhaustive.py — match по СУММЕ novac
называет ветку для каждого варианта.

ЗАЧЕМ. Оракул этого НЕ ловит (проба 2026-08-16: непокрытый вариант даёт пустое
значение и код 0), поэтому решает страж.

ПРАВИЛО (перенесено из shell-редакции слово в слово, 2026-08-19):
  1. Собираются суммы: `type X enum` и следующие за ней строки `| Variant`.
     Комментарий и пустая строка ВНУТРИ перечисления его не заканчивают —
     `///`-док у варианта обрывал сбор, и у TokenKind собиралось 26 имён из 64,
     после чего ни один match по нему не опознавался (2026-08-18).
  2. Собираются match'и: армы на отступе `match` + 4. Перенесённый арм
     (`A | B |` и продолжение ниже) склеивается — иначе match с длинной
     OR-группой уходил «вне суда» молча.
  3. Судится match, у которого набор армов лежит РОВНО в одной сумме. Арм `_`
     или пустой набор — вне суда. Непокрытые варианты — красное.

ПОЧЕМУ PYTHON: shell-редакция поднимала `tr`+`awk` на каждый файл и три прохода
awk сверху — 3.0с там, где работы на доли секунды (П14).

$1 — корень репозитория; $2 — override пути к novac/src (шов самотеста).
"""
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-match-exhaustive"


def collect_variants(files):
    """{сумма: {варианты}} — по всем файлам подряд, как `cat` в shell."""
    vars_ = {}
    sum_ = ""
    for p in files:
        for raw in p.read_text(encoding="utf-8", errors="replace").replace("\r", "").split("\n"):
            m = re.match(r"^(export )?type ([A-Z][A-Za-z0-9_]*) enum", raw)
            if m:
                sum_ = m.group(2)
                continue
            if sum_ and re.match(r"^[ \t]*\|[ \t]*[A-Z]", raw):
                line = re.sub(r"^[ \t]*\|[ \t]*", "", raw)
                mm = re.match(r"^[A-Za-z0-9_]+", line)
                if mm:
                    vars_.setdefault(sum_, set()).add(mm.group(0))
                continue
            if sum_ and re.match(r"^[ \t]*//", raw):
                continue
            if sum_ and not raw.strip():
                continue
            if sum_:
                sum_ = ""
    return vars_


def collect_matches(files, src):
    """[(loc, [армы])] — по каждому файлу отдельно, как в shell."""
    out = []
    for p in files:
        rel = p.relative_to(src).as_posix()
        depth = 0
        starts, lines, arms, pend = {}, {}, {}, {}
        for i, raw in enumerate(p.read_text(encoding="utf-8", errors="replace").replace("\r", "").split("\n"), 1):
            ind = len(raw) - len(raw.lstrip(" "))
            body = raw.lstrip()
            while depth > 0 and body.startswith("}") and ind <= starts[depth]:
                out.append((f"{rel}:{lines[depth]}", arms[depth]))
                depth -= 1
            if re.match(r"^match .*\{[ \t]*$", body):
                depth += 1
                starts[depth], lines[depth], arms[depth], pend[depth] = ind, i, [], ""
                continue
            if depth > 0 and ind == starts[depth] + 4 and "=>" not in body and re.search(r"\|[ \t]*$", body):
                pend[depth] = pend[depth] + " " + body
                continue
            if depth > 0 and ind == starts[depth] + 4 and "=>" in body:
                head = pend[depth] + " " + body
                pend[depth] = ""
                head = re.sub(r"=>.*$", "", head)
                for part in head.split("|"):
                    pat = part.strip()
                    mm = re.match(r"^[A-Za-z_][A-Za-z0-9_]*", pat)
                    if mm:
                        arms[depth].append(mm.group(0))
        while depth > 0:
            out.append((f"{rel}:{lines[depth]}", arms[depth]))
            depth -= 1
    return out


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    src = pathlib.Path(a[2]) if len(a) > 2 else root / "novac" / "src"

    if not src.is_dir():
        print(f"{NAME} ok: судить нечего (нет {src})")
        return 0

    files = sorted(src.rglob("*.nv"))
    if not files:
        print(f"{NAME}: FAIL — в {src} нет ни одного .nv: страж потерял мишень", file=sys.stderr)
        return 1

    vars_ = collect_variants(files)
    if not vars_:
        print(f"{NAME}: FAIL — не найдено ни одной суммы: разбор сломался", file=sys.stderr)
        return 1

    matches = collect_matches(files, src)

    judged = skip = 0
    bad = []
    for loc, arm_list in matches:
        arms, wild = set(), False
        for x in arm_list:
            if not x:
                continue
            if x == "_":
                wild = True
                continue
            arms.add(x)
        if wild or not arms:
            skip += 1
            continue
        cands = [s for s, vs in vars_.items() if arms <= vs]
        if len(cands) != 1:
            skip += 1
            continue
        judged += 1
        miss = sorted(vars_[cands[0]] - arms)
        if miss:
            bad.append(f"  {loc} — match по сумме {cands[0]} не покрывает: {' '.join(miss)}")

    if bad:
        print(f"{NAME}: FAIL — match по сумме novac оставляет варианты без ответа:", file=sys.stderr)
        for b in bad:
            print(b, file=sys.stderr)
        print("  Оракул это НЕ ловит (проба 2026-08-16: непокрытый вариант даёт", file=sys.stderr)
        print("  пустое значение и код 0), поэтому решает страж: назови ветку для", file=sys.stderr)
        print("  каждого варианта — хоть ice(), но осознанно.", file=sys.stderr)
        return 1

    print(f"{NAME} ok: сумм {len(vars_)}, match'ей {len(matches)} — "
          f"судимых по сумме {judged} (все полные), вне суда {skip}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
