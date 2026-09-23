<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# №1324 — продолжение строки: отступ следующей строки и CRLF

**Норма:** D467 §3 (`spec/decisions/03-syntax.md`) — `\` перед переводом строки
съедает и перенос, и ведущие пробелы следующей строки. Оракул
(`compiler-codegen/src/lexer/mod.rs`, ветка `b'\r' | b'\n'` разбора escape) съедает
`\n` или `\r\n`, затем пробелы и табы.

**Проба LF:** [probe_lf.nv.txt](probe_lf.nv.txt) — `"ab\` + перенос + `cd"` и
`"x\` + перенос + `  y"` (со сдвигом).

**Проба CRLF** в репозиторий не кладётся: git приводит текстовый файл к LF, и проба
перестала бы быть собой. Её делает одна строка:

```sh
python -c "import sys; sys.stdout.buffer.write(open('docs/plans/repro/1324-string-continuation-indent/probe_lf.nv.txt','rb').read().replace(b'\n', b'\r\n'))" > probe_crlf.nv
```

Запуск — смоук поведения (novac против оракула), из корня:

```sh
cp docs/plans/repro/1324-string-continuation-indent/probe_lf.nv.txt probe_lf.nv
sh scripts/tools/novac-e1-smoke.sh probe_lf.nv
sh scripts/tools/novac-e1-smoke.sh probe_crlf.nv
```

| проба | оракул | novac до `e583aba20` | novac после |
|---|---|---|---|
| LF | `abcd`, `xy` | `abcd`, **`x  y`** — тихо неверная строка | байт в байт с оракулом |
| CRLF | `abcd`, `xy` | `check` отказ «this string escape is not compiled yet» на `\r` | байт в байт с оракулом |

**Два места, два дефекта:**

1. `novac/src/sem/mangle.nv` — дверь «литерал Nova → текст C» отдавала продолжение
   C-склейке второй фазы, а та сохраняет отступ следующей строки. Теперь дверь
   (`literal_as_c_text`) вырезает продолжение по правилу оракула.
2. `novac/src/check/strings.nv` — дверь экранирований знала `\` + `\n`, но не
   `\` + `\r\n`, хотя лексер (`lex/lex.nv`, CRLF-ветка разбора escape) читал это
   как одно продолжение.

**Почему это касалось самосборки:** файл, переписанный слиянием, выписывается с
CRLF (в хранилище он с LF). В дереве окна Карины у `pipeline/subset_test.nv` было 305
продолжений `\` + CRLF и 69 ложных отказов, у `pipeline/pipeline_test.nv` — 39. Счёт
отвергнутых файлов зависел от выписки: 91 в этом дереве против 95 в полностью CRLF.

**Названное расхождение:** голый `\` + CR без LF для оракула — продолжение, для novac —
отказ (лексер novac не читает его продолжением). Громко; контроль в
`check/check_test.nv`.

**Проба в обе стороны:** откат `check/strings.nv` краснит тест `check_test.nv`
«a line continuation on a CRLF file is one continuation» на `diagnose(crlf) == 0`; без
правки двери смоук давал `x  y` на обеих пробах.
