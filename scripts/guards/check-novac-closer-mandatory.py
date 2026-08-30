# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-closer-mandatory.py — закрывающая скобка берётся
ОБЯЗАТЕЛЬНОЙ дверью (#809).

ПОЧЕМУ. У парсера две двери на «взять токен, если он тут»: `@push_if` говорит
«токен здесь необязателен, и его отсутствие — законный исход», `@push_closer`
сажает Err-лист и даёт чекеру отказ с позицией. Для запятой верна первая, для
закрывающей скобки — только вторая: открывающая уже съедена, и язык требует
пары.

ЦЕНА ОШИБКИ ЗАМЕРЕНА, а не воображена: до 2026-08-30 все шестнадцать
закрывающих (6 `)`, 5 `]`, 5 `}`) шли необязательной дверью, и `fn main() {`
без `}` компилировался с кодом 0 и БЕЗ ЕДИНОЙ диагностики — охота нашла это
пробой `brace_only_fn`, реестр 221.1 №809. Класс назван там же: «обязательный
закрывающий токен то опционален, то нет» — то есть одно правило, разошедшееся
на два.

ЧТО СУДИТСЯ: вызов `@push_if(<что-то>, TokenKind.RParen|RBracket|RBrace)` в
`.nv` под `novac/src`. Ноль — норма и база.

СУДЯТСЯ ТОЛЬКО ЖИВЫЕ СТРОКИ: комментарий, ЦИТИРУЮЩИЙ снятую форму, законен —
без этого страж стирал бы историю класса вместе с самим классом (правка
2026-08-17 в соседних стражах, тот же грабли-класс).

ПОЧЕМУ PYTHON: старт процесса дороже самой проверки (П14).

$1 — корень; $2 — override директории (шов самотеста).
"""
import os
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-closer-mandatory"
RE_OPTIONAL_CLOSER = re.compile(r"@push_if\([^()]*,\s*TokenKind\.(RParen|RBracket|RBrace)\s*\)")
RE_MANDATORY = re.compile(r"@push_closer\(")


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

    if not files:
        print(f"{NAME}: FAIL — в {src} нет ни одного .nv: страж потерял мишень", file=sys.stderr)
        return 1

    bad = []
    closers = 0
    for f in files:
        rel = str(f.relative_to(src)).replace("\\", "/")
        lines = f.read_bytes().decode("utf-8", "replace").split("\n")
        if lines and lines[-1] == "":
            lines.pop()
        for n, line in enumerate(lines, 1):
            if line.endswith("\r"):
                line = line[:-1]
            s = line.lstrip(" \t\v\f")
            # комментарий или док — не судим: история класса там законна
            if s.startswith("//"):
                continue
            m = RE_OPTIONAL_CLOSER.search(line)
            if m:
                bad.append(f"  {rel}:{n} — `{m.group(1)}` взят необязательной дверью: {s[:72]}")
            # Считаются ВЫЗОВЫ, а не объявление самой двери: иначе число в
            # вердикте на единицу больше правды, и читатель сверяет его с
            # грепом впустую.
            if not s.startswith("fn "):
                closers += len(RE_MANDATORY.findall(line))

    if bad:
        print(f"{NAME}: FAIL — закрывающая скобка взята дверью для НЕОБЯЗАТЕЛЬНОГО токена (#809):", file=sys.stderr)
        for b in bad:
            print(b, file=sys.stderr)
        print("  `@push_if` означает «токена может не быть, и это законно» — верно", file=sys.stderr)
        print("  для запятой, неверно для пары к уже съеденной скобке.", file=sys.stderr)
        print("  Закрывающие идут через `@push_closer`: он сажает Err-лист, и", file=sys.stderr)
        print("  чекер отказывает с позицией вместо тихого приёма (rc=0 на", file=sys.stderr)
        print("  `fn main() {` без `}` — реестр 221.1 №809).", file=sys.stderr)
        return 1

    print(f"{NAME} ok: файлов .nv: {len(files)}, закрывающих через обязательную дверь: {closers}, "
          f"через необязательную: 0")
    return 0


if __name__ == "__main__":
    sys.exit(main())
