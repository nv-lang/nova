# examples/stdlib/ — статус относительно bootstrap-codegen

Это **демо-материалы** показывающие spec-faithful Nova код для базовых
структур данных и парсеров. Они написаны как **аспирационные**: они
демонстрируют как код *должен* выглядеть в зрелом Nova, но bootstrap-
codegen в текущей итерации не покрывает все используемые фичи.

Запуск через `.\run_tests.ps1 -IncludeStdlib` — 11 примеров, **0 из 11
сейчас компилируются** в bootstrap'е. Это ожидаемо. Список причин ниже —
для приоритезации будущих compiler-задач.

## Закрытые блокеры (2026-05-07)

- **char-литералы** ('a' / '\n' / '\u{...}') — реализованы (Q-char-literals,
  commit 7852ced). Разблокировало complex.nv и json.nv в начальных строках.
- **throw в expression position** (D25/D65) — реализован (commit cfa53ca).
  Разблокировало sql.nv от старого блокера на 163.
- **Match scrutinee parsing** — fix `match foo() { ... }` парсился как
  call-with-trailing-block (commit d467cd2). Разблокировало semver.nv и sql.nv.
- **Leading `||` / `&&` newline-tolerance** (commit 781bb43, spec 1073295).
  Boolean-expression может продолжаться с leading || / && на новой строке.
  Разблокировало semver.nv (251 → 449).

## Текущие блокеры

| File | Блокирующая фича | Stage | Прогресс |
|---|---|---|---|
| complex.nv | multi-line if-else (`expected '{', got newline`) на 560 | parser | 317 → 560 (после char-литералов) |
| duration.nv | multi-line if-else на 252 | parser | без изменений |
| hashmap.nv | `&` operator (referencing) на 219 | spec/lexer | без изменений |
| json.nv | pattern parser (`expected pattern, got ','`) на 98 | parser | 163 → 98 (после char-литералов) |
| linkedlist.nv | `effect` keyword в позиции type на 48 | parser/spec | без изменений |
| queue.nv | `in` keyword в выражении (loop?) на 26 | parser | без изменений |
| range.nv | anonymous record literal `{ field: val }` без spread | codegen | без изменений |
| semver.nv | `assert match` (нет такого) + handler-lambda `(e) => interrupt Some(e)` на 449 | parser/spec | 136 → 251 → 449 (после match + leading-`||`) |
| set.nv | `&` operator (referencing) на 152 | spec/lexer | без изменений |
| sql.nv | `expected '=>', got '=='` на 295 (match-arm с условием guard?) | parser | 201 → 295 (после match-arm fix) |
| vec.nv | `expected identifier, got '['` на 25 (turbofish/generic syntax?) | parser | без изменений |

## Группы блокеров

**A. ~~Char-литералы~~** — ✅ закрыто (commit 7852ced).

**B. `&` operator** — 2 файла: hashmap.nv, set.nv. Это referencing
   синтаксис который Nova spec **отвергает** (D6: managed heap, нет
   ownership-borrowing). Файлы написаны до этого решения. Чинятся
   убиранием `&` в коде stdlib-файлов.

**C. Multi-line if-else / continuation** — duration.nv, complex.nv.
   D49 newline-tolerance может не покрывать все cases. Конкретные
   места — multiline expression continuation.

**D. Generic syntax** — vec.nv. Spec D16: дженерики через `[T]`.
   Конкретное место — `vec[T]` в начале файла, парсер ожидает ident
   после `vec`.

**E. Anonymous record literal** — range.nv. Bootstrap codegen
   эксплицитно говорит «not supported». Spec D55 описывает coercion в
   позиции с явным типом — нужна inferred-type-context реализация.

**F. effect keyword as type** — linkedlist.nv. `effect` в позиции type
   не предусмотрен Nova spec'ом. Скорее всего — баг в файле.

**G. ~~throw в expression position~~** — ✅ закрыто (commit cfa53ca).

**H. Pattern parsing** — json.nv expecting pattern but got comma.
   Возможно — паттерны внутри tuple-deconstruction или другая
   композиция, не поддержанная парсером.

**I. `||` operator** — semver.nv. Boolean-or `||` в expression position.
   Скорее всего парсер не справляется в каком-то контексте.

**J. Match-arm guards (`=>` after `==`)** — sql.nv. Возможно `Some(x) if cond => ...`
   где `if` не распознан, или другая arm-form.

**K. `in` keyword в expression** — queue.nv. Может быть `for ... in ...`
   в неожиданном контексте, либо использование `in` для membership-check
   (что Nova spec отвергает).

## Что делать

Это **не приоритетные баги**, а **gap'ы между spec-aspiration и
bootstrap-возможностями**. Рекомендуемая последовательность:

1. **Spec-clarification:** group B (`&` — переписать stdlib-файлы),
   group F (effect-type — bug в файле), group K (in-keyword).
2. **Парсер-доработки:** group C (multi-line continuation), group D
   (vec generic syntax), group H (pattern composition), group I (`||`),
   group J (match-arm guards).
3. **Codegen-доработки:** group E (anonymous record literal с
   inferred-type-context).

После каждой группы — recompile и продвигаться по списку. Финальная
цель — **11/11 stdlib examples PASS** через `run_tests.ps1 -IncludeStdlib`.
