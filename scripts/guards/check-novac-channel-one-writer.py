# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-channel-one-writer.py — у канала чекера ОДИН
писатель, и вывод типа не уезжает ниже чекера.

ПРАВИЛО. В канал (`CheckOut`) пишет ТОЛЬКО `check/`; остальные ЧИТАЮТ
(`out.type_of(id)`). Нужен новый факт о типе — его записывает чекер, а не
вычисляет потребитель. Второй писатель — это вторая дверь к типу, класс плана
196: два места начинают отвечать на один вопрос и расходятся молча.

ПРОВЕРЯЕТ три вещи, и в этом порядке (порядок — часть вывода):
  A. вызов писателя канала (`record_type`/`record_callee`/`record_subst`/`record_implicit`) вне
     `check/`;
  D. ПРЯМАЯ запись в таблицу канала мимо двери (`.types[...] =`, `.callees =`);
  B. вывод типа вне `check/` (`unify(`, `fresh_var(`, `infer_*(`, `type_of(`
     как свободный вызов) — вторая дверь к типу;
  E. дверь ПОДБОРА вне `check/` и `resolve/` (274.4 шаг 4): `lookup`,
     `only/first/next_row_of`, `has_name`, `has_method_for`, `missing_sig_of`,
     `field_type`, `has_field`, `entry_row`. Разрешение имени — решение, и
     решает его чекер через резольвер; `is_entry_row` (предикат по СТРОКЕ)
     дверью не является и не ловится.
Плюс (C): мишень на месте — файл канала с `export type CheckOut` обязан
найтись, иначе страж молча судил бы пустоту (класс №519).

ПОЧЕМУ PYTHON: shell-редакция была уже свёрнута в один awk, но разбор его
вывода поднимал `grep`/`cut`/`sort`/`awk` на КАЖДЫЙ раздел — 1.3с там, где
работы на 40мс (П14).

$1 — корень; $2 — override сканируемой директории (шов самотеста).
"""
import os
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-channel-one-writer"
CF = "types|callees|substs|subst_args"

RE_WRITER = re.compile(r"record_type\(|record_callee\(|record_subst\(|record_implicit\(")
RE_DIRECT_IDX = re.compile(r"\.(" + CF + r")\[[^]]*\][ \t]*=[^=]")
RE_DIRECT = re.compile(r"\.(" + CF + r")[ \t]*=[^=]")
RE_INFER = re.compile(r"unify\(|fresh_var\(|infer_[a-z_]*\(|[^.a-zA-Z_]type_of\(")
RE_RESOLVE = re.compile(
    r"\.lookup\(|only_row_of\(|first_row_of\(|next_row_of\(|has_name\(|"
    r"has_method_for\(|missing_sig_of\(|field_type\(|has_field\(|"
    r"(?<![a-z_])entry_row\(")


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    src = pathlib.Path(a[2]) if len(a) > 2 else root / "novac" / "src"

    if not src.is_dir():
        print(f"{NAME} ok: судить нечего (нет {src})")
        return 0

    files = []
    for dirpath, _dirs, names in os.walk(src):
        for nm in names:
            if nm.endswith(".nv"):
                files.append(pathlib.Path(dirpath) / nm)
    files.sort(key=lambda p: str(p).replace("\\", "/"))

    # --- (C) мишень на месте ------------------------------------------------
    chan = None
    for f in files:
        if f.name == "channel.nv":
            chan = f
            break
    if chan is None or "export type CheckOut" not in chan.read_text(encoding="utf-8", errors="replace"):
        print(f"{NAME}: FAIL — не найден файл канала с 'export type CheckOut': "
              f"страж потерял мишень (класс №519)", file=sys.stderr)
        return 1

    hits = {"A": [], "D": [], "B": [], "E": []}
    nfiles = 0
    for f in files:
        nfiles += 1
        rel = str(f.relative_to(src)).replace("\\", "/")
        is_check = rel.startswith("check/")
        is_chan = rel == "sem/channel.nv"
        is_resolve = rel.startswith("resolve/")
        for n, raw in enumerate(f.read_bytes().decode("utf-8", "replace").split("\n"), 1):
            if raw.endswith("\r"):
                raw = raw[:-1]
            if raw.lstrip(" \t\v\f").startswith("//"):
                continue
            if not is_check and not is_chan and RE_WRITER.search(raw):
                hits["A"].append((rel, f"{n}:{raw}"))
            if not is_chan and (RE_DIRECT_IDX.search(raw) or RE_DIRECT.search(raw)):
                hits["D"].append((rel, f"{n}:{raw}"))
            if not is_check and not is_chan and RE_INFER.search(raw):
                hits["B"].append((rel, f"{n}:{raw}"))
            if not is_check and not is_resolve and RE_RESOLVE.search(raw):
                hits["E"].append((rel, f"{n}:{raw}"))

    bad = []
    for tag, note in (("A", "зовёт писателя канала вне check/"),
                      ("D", "ПРЯМАЯ запись в таблицу канала мимо двери"),
                      ("B", "вывод типа вне check/ (вторая дверь к типу, класс плана 196)"),
                      ("E", "дверь подбора вне check/ и resolve/ (вторая дверь к вызываемому, 274.4 шаг 4)")):
        cur = ""
        # sort -u -t'|' -k1,1 -k2,2: по файлу, затем по строке КАК ТЕКСТУ
        for rel, entry in sorted(set(hits[tag])):
            if rel != cur:
                cur = rel
                bad.append(f"  {rel} — {note}:")
            bad.append(f"      {entry}")

    if bad:
        print(f"{NAME}: FAIL — у канала чекера появился второй писатель или "
              f"вывод типа уехал ниже чекера:", file=sys.stderr)
        for b in bad:
            print(b, file=sys.stderr)
        print("  Правило: пишет ТОЛЬКО check, остальные ЧИТАЮТ (out.type_of(id)). Нужен новый", file=sys.stderr)
        print("  факт о типе — его записывает чекер, а не вычисляет потребитель.", file=sys.stderr)
        return 1

    nw = 0
    checkdir = src / "check"
    if checkdir.is_dir():
        for f in sorted(checkdir.glob("*.nv")):
            for raw in f.read_bytes().decode("utf-8", "replace").split("\n"):
                if RE_WRITER.search(raw):
                    nw += 1

    print(f"{NAME} ok: файлов .nv: {nfiles}, вызовов писателей канала: {nw} (все в check/), "
          f"вывода типа вне чекера: 0, дверей подбора вне резольвера: 0")
    return 0


if __name__ == "__main__":
    sys.exit(main())
