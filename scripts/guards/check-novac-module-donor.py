# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-module-donor.py — заголовок модуля novac называет
донора-УКАЗАТЕЛЬ, роль, потребителя и механизм (конвенция П27).

ЧЕТЫРЕ СТРОКИ В ПЕРВЫХ СОРОКА:
  `// Donor: <кто> <сущность> — взято/не взято`  — донор без сущности не
      указатель: «Rust» ни на что не показывает, «rustc InternPool» показывает;
  `// Role: <место в карте слоёв, класс задачи>`;
  `// Used by: <кто читает, на каком этапе>`;
  `// Guarded by: <check-*.sh или check-*.py, тесты — кто ловит неверное
      использование>` — и названный страж обязан СУЩЕСТВОВАТЬ: выдуманный
      механизм хуже отсутствующего, на него ссылаются.

Донора нет — так и пишется: `// Donor: none — <причина>`, причина минимум пять
слов. Role и Used by нужны всё равно.

ТРИ ЗАПРЕТА, каждый по случаю:
  * Swift/C#/.NET без формы отказа («NOT taken…») — антипример выдан за донора;
  * Zig без его сущности (InternPool/Sema/…) — точечный донор без места;
  * ОРАКУЛ (nova-cli, compiler-codegen, emit_c.rs) донором быть не может (П25).

ПОЧЕМУ PYTHON: старт процесса дороже самой проверки (П14).

$1 — корень; $2 — override директории (шов самотеста).
"""
import os
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-module-donor"
RE_DONOR = re.compile(r"^///? *Donor *[:—-]")
RE_ROLE = re.compile(r"^///? *Role *[:—-]")
RE_USED = re.compile(r"^///? *Used by *[:—-]")
RE_GUARDED = re.compile(r"^///? *Guarded by *[:—-]")
RE_CONT = re.compile(r"^///? +")
RE_GUARD_NAME = re.compile(r"check-[a-z0-9-]+\.(?:sh|py)")
RE_NONE = re.compile(r"^none[ \t]*(—|--|-)", re.I)
RE_NONE_STRIP = re.compile(r"^[Nn][Oo][Nn][Ee][ \t]*(—|--|-)[ \t]*")
RE_SWIFT = re.compile(r"(^|[^a-z])swift([^a-z]|$)", re.I)
RE_NOT_TAKEN = re.compile(r"not taken|anti-example|not a donor|not from", re.I)
RE_ZIG = re.compile(r"(^|[^a-z])zig([^a-z]|$)", re.I)
RE_ZIG_ENTITY = re.compile(r"InternPool|Sema|StaticStringMap|OptionalIndex|std\.")
RE_ORACLE = re.compile(r"oracle|orakul|nova-cli|compiler-codegen|emit_c\.rs", re.I)
RE_HONEST_GUARD = re.compile(r"^compiler|^acceptance|nova test|nova lint|fuzz")


def words(s):
    s = s.strip(" \t")
    return len(re.split(r"[ \t]+", s)) if s else 0


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    src = pathlib.Path(a[2]) if len(a) > 2 else root / "novac" / "src"

    if not src.is_dir():
        print(f"{NAME} ok: судить нечего (нет {src})")
        return 0

    guard_dir = root / "scripts" / "guards"
    guards = {p.name for p in guard_dir.iterdir()} if guard_dir.is_dir() else set()

    files = []
    for dirpath, _dirs, names in os.walk(src):
        for nm in names:
            if nm.endswith(".nv") and not nm.endswith("_test.nv"):
                files.append(pathlib.Path(dirpath) / nm)
    files.sort(key=lambda p: str(p).replace("\\", "/"))

    bad = []
    for f in files:
        rel = str(f.relative_to(src)).replace("\\", "/")
        donor = role = used = guarded = dblock = ""
        seen_donor = False
        in_d = False
        for n, line in enumerate(f.read_bytes().decode("utf-8", "replace").split("\n"), 1):
            if n > 40:
                break
            if line.endswith("\r"):
                line = line[:-1]
            if RE_DONOR.match(line):
                if not seen_donor:
                    donor = RE_DONOR.sub("", line).lstrip(" ")
                    seen_donor = True
                in_d = True
            elif RE_ROLE.match(line):
                in_d = False
                if not role:
                    role = RE_ROLE.sub("", line).lstrip(" ")
            elif RE_USED.match(line):
                if not used:
                    used = RE_USED.sub("", line).lstrip(" ")
            elif RE_GUARDED.match(line):
                if not guarded:
                    guarded = RE_GUARDED.sub("", line).lstrip(" ")
                else:
                    guarded += " " + line
            elif guarded and RE_CONT.match(line):
                # продолжение строки Guarded by переносом
                cont = RE_CONT.sub("", line)
                if RE_GUARD_NAME.search(cont):
                    guarded += " " + cont
            if in_d:
                dblock += " " + line

        def say(msg):
            bad.append(f"  {rel}: {msg}")

        if not seen_donor:
            say("нет строки '// Donor:' в заголовке (первые 40 строк)")
            continue

        body = donor
        if RE_NONE.match(body):
            reason = RE_NONE_STRIP.sub("", body)
            if words(reason) < 5:
                say("'Donor: none' без причины (нужно минимум пять слов)")
            body = "none reason ok"
        if words(body) < 2:
            say(f"'Donor:' без сущности — одно имя не указатель: «{donor}»")

        if RE_SWIFT.search(dblock) or "C#" in dblock or ".NET" in dblock:
            if not RE_NOT_TAKEN.search(dblock):
                say("Donor называет Swift/C# без формы отказа «NOT taken ...» — "
                    "антипример выдан за донора")
        if RE_ZIG.search(dblock) and not RE_ZIG_ENTITY.search(dblock):
            say("Donor называет Zig без его сущности (InternPool/Sema/...) — "
                "точечный донор без места")
        if RE_ORACLE.search(dblock):
            say("'Donor:' называет ОРАКУЛ (нынешний компилятор) донором — запрещено (П25)")

        if words(role) < 4:
            say("нет строки '// Role:' с местом в общей картине (минимум четыре слова)")
        if words(used) < 3:
            say("нет строки '// Used by:' — кто потребитель дальше и когда")
        if words(guarded) < 1:
            say("нет строки '// Guarded by:' — кто автоматически проверяет правило")
        else:
            n_found = 0
            for m in RE_GUARD_NAME.finditer(guarded):
                n_found += 1
                if m.group(0) not in guards:
                    say(f"'Guarded by' называет {m.group(0)}, а такого стража нет "
                        f"в scripts/guards — механизм выдуман")
            if n_found == 0 and not RE_HONEST_GUARD.search(guarded):
                say("'Guarded by' не называет ни стража, ни теста, ни честного "
                    "compiler/acceptance")

    if bad:
        print(f"{NAME}: FAIL — модуль novac без донора-указателя в заголовке (конвенция П27):",
              file=sys.stderr)
        for b in bad:
            print(b, file=sys.stderr)
        print("  В первых 40 строках три строки: '// Donor: <кто> <сущность> — взято/не взято',",
              file=sys.stderr)
        print("  '// Role: <место в карте слоёв, класс задачи>', "
              "'// Used by: <кто читает, на каком этапе>',", file=sys.stderr)
        print("  '// Guarded by: <check-*.sh или check-*.py, тесты — кто автоматически ловит "
              "неверное использование>'.", file=sys.stderr)
        print("  Донора нет — честно: '// Donor: none — <причина>'; Role и Used by нужны всё равно.",
              file=sys.stderr)
        return 1

    if not files:
        # МИШЕНЬ УЕХАЛА, А НЕ «НАРУШЕНИЙ НЕТ» (класс №911, страж
        # check-guard-empty-root): каталог есть, подсудных файлов ноль —
        # печатать здесь правдоподобный счёт значит выдавать пустоту за
        # проверенное. Формулировка донорская, от check-novac-file-size.py.
        print(f"{NAME} ok: судить нечего (0 модулей в {src})")
        return 0

    print(f"{NAME} ok: модулей novac: {len(files)}, у всех донор назван указателем "
          f"или честно отсутствует")
    return 0


if __name__ == "__main__":
    sys.exit(main())
