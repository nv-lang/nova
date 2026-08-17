# -*- coding: utf-8 -*-
"""Матрица форм языка: что принимает ОРАКУЛ и что разбирает NOVAC.

Опись §9.4а плана 274 снимается этим прогоном, а не перечислением по памяти.
Каждая форма — отдельная папка (папка = один модуль), обе стороны отвечают на
один и тот же файл.

ПОРЯДОК ЧТЕНИЯ КОЛОНОК. Пока форму не принял оракул, колонка novac ничего не
доказывает: отказ по кривой пробе и отказ по дыре в парсере выглядят одинаково.
Шесть проб из двадцати в первой редакции были кривыми — имя типа короче двух
символов, атрибут эффектов не на своём месте, путь модуля через слэш вместо
точки, — и каждая притворялась дырой в novac. Строка `oracle` с REFUSED значит
«проба не готова», а не «форма вне языка».

Запуск:  python scripts/tools/novac-grammar-matrix.py
         python scripts/tools/novac-grammar-matrix.py --only variadic,as_cast
"""
import os
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
BASE = pathlib.Path(os.environ.get("TEMP", "/tmp")) / "novac-grammar-matrix"
ORACLE = ROOT / "nova-cli/target/release/nova.exe"
NOVAC = ROOT / "novac/target/novac.exe"

# Класс A -- отказ novac называет ЛОЖНУЮ причину (дороже всех).
# Класс B -- отказ безымянный: «construct not in the MVP grammar».
# Класс C -- отказ честный: форма названа, этап назван (целевое поведение).
FORMS = [
    ("variadic", "A", 'fn total(...args []int) -> int => args.len()\n\n'
                      'fn main() { println(total(1, 2)) }\n'),
    ("default_param", "A", 'fn bump(v int, by int = 1) -> int => v + by\n\n'
                           'fn main() { println(bump(1)) }\n'),
    # без значения по умолчанию: иначе форму заслоняет отказ про дефолт,
    # и колонка меряет не то, что названа мерить
    ("named_arg", "A", 'fn bump(v int, by int) -> int => v + by\n\n'
                       'fn main() { println(bump(1, by: 2)) }\n'),
    ("unit_return", "A", 'fn nothing() -> () { }\n\nfn main() { nothing() }\n'),
    ("for_over_vec", "A", 'fn main() {\n    mut s = 0\n'
                          '    for x in []int.of(1, 2) { s += x }\n    println(s)\n}\n'),
    ("as_cast", "A", 'fn main() { println(1 as u8) }\n'),

    ("typed_local", "B", 'fn main() {\n    ro x int = 1\n    println(x)\n}\n'),
    ("if_expr", "B", 'fn main() {\n    ro v = if true { 1 } else { 2 }\n    println(v)\n}\n'),
    # D452: однострочные плечи разделяются запятой -- форма законна
    ("match_oneline", "B", 'fn pick(n int) -> int => match n { 1 => 10, _ => 2 }\n\n'
                           'fn main() { println(pick(1)) }\n'),
    ("str_interp", "B", 'fn main() {\n    ro n = 2\n    println("n=${n}")\n}\n'),
    # E_TYPE_NAME_TOO_SHORT (Plan 167): имя типа не короче двух символов
    ("field_access", "B", 'type Pt {\n    x int\n}\n\nfn Pt @get() -> int => @x\n\n'
                          'fn main() {\n    ro p = Pt { x: 5 }\n    println(p.get())\n}\n'),
    # атрибут эффектов стоит ПЕРЕД fn, отдельной строкой
    ("effects_attr", "B", '#realtime nogc\nfn pure_add(a int, b int) -> int => a + b\n\n'
                          'fn main() { println(pure_add(1, 2)) }\n'),
    # у Vec метода map нет вовсе -- замыкание живёт на LinkedList
    ("closure_pipe", "B", 'import std.collections.linkedlist.{LinkedList}\n\nfn main() {\n'
                          '    ro l = LinkedList[int].new()\n    ro d = l.map(|x| x * 2)\n'
                          '    println(d.len())\n}\n'),
    ("test_block", "B", 'fn main() { println(1) }\n\ntest "one" {\n    assert(1 == 1)\n}\n'),
    # путь модуля пишется ТОЧКАМИ, не слэшами
    ("import_braced", "B", 'import std.collections.deque.{Deque}\n\nfn main() {\n'
                           '    ro d = Deque[int].new()\n    println(d.len())\n}\n'),
    ("import_whole", "B", 'import std.time.duration\n\nfn main() { println(1) }\n'),

    ("match_stmt", "C", 'type Col enum Red | Green\n\nfn code(c Col) -> int {\n'
                        '    match c {\n        Red => 1\n        Green => 2\n    }\n}\n\n'
                        'fn main() { println(code(Col.Red)) }\n'),
    # E_READONLY_COERCE не даёт вернуть ro-биндинг наружу, генерик остаётся
    ("generic_free_fn", "C", 'fn tag[T](v T) -> int => 1\n\nfn main() { println(tag(7)) }\n'),
    ("tuple_type", "C", 'fn pair() -> (int, int) => (1, 2)\n\nfn main() {\n'
                        '    ro p = pair()\n    println(p.0)\n}\n'),

    ("while_loop", "-", 'fn main() {\n    mut i = 0\n    while i < 3 { i += 1 }\n    println(i)\n}\n'),
    ("range_for", "-", 'fn main() {\n    mut s = 0\n    for x in 0..3 { s += x }\n    println(s)\n}\n'),
]

# Ложь, которую каждая форма класса A выдавала до 2026-08-18. Счётчик ищет
# именно эти строки: «отказано» и «отказано не по той причине» — разные вещи,
# и мерить надо вторую.
LIES = {
    "variadic": "requires a declared return type",
    "default_param": "requires a declared return type",
    "named_arg": "requires a declared return type",
    "unit_return": "ends without a value",
    "for_over_vec": "the head of a for must be a range",
    "as_cast": "unknown name",
}

env = dict(os.environ)
env.setdefault("NOVA_GC_INCLUDE_DIR",
               str(ROOT / "compiler-codegen/vcpkg_installed/x64-windows-static/include"))
env.setdefault("NOVA_GC_LIB_DIR",
               str(ROOT / "compiler-codegen/vcpkg_installed/x64-windows-static/lib"))
env["NOVA_STD_PATH"] = "std/src"
env["LC_ALL"] = "C"


def run(cmd, cwd=None):
    try:
        p = subprocess.run(cmd, capture_output=True, timeout=180, env=env,
                           cwd=str(cwd) if cwd else None)
        return p.returncode, (p.stdout + p.stderr).decode("utf-8", "replace")
    except subprocess.TimeoutExpired:
        return 124, "TIMEOUT"


def main():
    only = None
    if "--only" in sys.argv:
        only = set(sys.argv[sys.argv.index("--only") + 1].split(","))

    lies = 0
    unnamed = 0
    for name, cls, body in FORMS:
        if only and name not in only:
            continue
        d = BASE / name
        d.mkdir(parents=True, exist_ok=True)
        (d / "m.nv").write_text("module m\n\n" + body, encoding="utf-8", newline="\n")

        rc_o, out_o = run([str(ORACLE), "check", str(d / "m.nv")])
        if rc_o == 0:
            oracle = "ok"
        else:
            err = [l for l in out_o.splitlines() if "error" in l]
            oracle = "REFUSED: " + (err[0].split("error: ")[-1][:44] if err else "?")

        rc_n, out_n = run([str(NOVAC), "check", str(d / "m.nv")], cwd=ROOT)
        if rc_n == 0:
            novac = "ok"
        elif "NOVAC_ICE" in out_n:
            novac = "ICE"
        elif '"message":"' in out_n:
            novac = out_n.split('"message":"')[1].split('"')[0][:56]
        else:
            novac = out_n.strip().replace("\n", " ")[:56]

        if oracle == "ok" and "not in the MVP grammar" in novac:
            unnamed += 1
        # Класс A закрыт не тогда, когда форма ПРИНЯТА, а когда ушла ЛОЖНАЯ
        # причина: отказ по имени формы — это и есть цель §9.4.
        if cls == "A" and oracle == "ok" and LIES.get(name, "@@") in novac:
            lies += 1
        print("%-16s %s | %-52s | %s" % (name, cls, oracle, novac))

    print("")
    print("class A still lying about the cause: %d" % lies)
    print("refusals still unnamed ('not in the MVP grammar'): %d" % unnamed)
    print("(A closes first: a false cause is found weeks later, and looked for"
          " in the wrong place.)")


main()
