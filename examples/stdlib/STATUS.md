# examples/stdlib/ — статус относительно bootstrap-codegen

Это **демо-материалы** показывающие spec-faithful Nova код для базовых
структур данных и парсеров. Они написаны как **аспирационные**: они
демонстрируют как код *должен* выглядеть в зрелом Nova, но bootstrap-
codegen в текущей итерации не покрывает все используемые фичи.

Запуск через `.\run_tests.ps1 -IncludeStdlib` — 11 примеров, **0 из 11
сейчас компилируются** в bootstrap'е. Это ожидаемо. Список причин ниже —
для приоритезации будущих compiler-задач.

| File | Блокирующая фича | Stage |
|---|---|---|
| complex.nv | char-литералы `'+'` `'-'` (для парсинга строки) | lexer |
| duration.nv | `expected '{', got newline` — multi-line if-else? | parser |
| hashmap.nv | `&` operator (referencing/borrowing) | spec/lexer |
| json.nv | char-литералы `'"'` `'\\\\'` для парсера | lexer |
| linkedlist.nv | `effect` keyword в позиции type | parser |
| queue.nv | `in` keyword в выражении (loop?) | parser |
| range.nv | anonymous record literal `{ field: val }` без spread | codegen |
| semver.nv | `=>` в неожиданном месте (handler-like construct?) | parser |
| set.nv | `&` operator (referencing) | spec/lexer |
| sql.nv | match-arm separator (parser ambiguity) — после fix throw-as-expr продвинулся со 163 до 201 | parser |
| vec.nv | `expected identifier, got '['` — turbofish/generic syntax? | parser |

## Проблемы по группам

**A. Char-литералы (`'c'`)** — 2 файла: complex.nv, json.nv. Открытый
   вопрос **Q-char-literals** в [spec/open-questions.md:3629](../../spec/open-questions.md)
   уже описывает proposed-синтаксис, грамматику и прецеденты. Нужно
   зафиксировать как D-решение и реализовать lexer + AST + codegen.
   Альтернатива (без char-литералов): использовать `s.char_at(i)` +
   `nova_str` сравнение, но это менее эргономично для парсеров.

**B. `&` operator** — 2 файла: hashmap.nv, set.nv. Это referencing
   синтаксис который Nova spec **отвергает** (D6: managed heap, нет
   ownership-borrowing). Файлы написаны до этого решения. Чинятся
   убиранием `&` (просто пропустить — Nova передаёт по reference сам).

**C. Multi-line if-else или handler block** — duration.nv, semver.nv.
   Возможно D49 newline-rules не покрывают этих случаев.

**D. Generic syntax** — vec.nv. Spec D16: дженерики через `[T]`.
   Конкретное место надо смотреть.

**E. Anonymous record literal** — range.nv. Bootstrap codegen
   эксплицитно говорит «not supported». Spec D55 описывает coercion в
   позиции с явным типом — это другой случай.

**F. effect keyword as type** — linkedlist.nv. Возможно, `Iter[T]` или
   `Iterable[T]` где T = effect-type? Не должно быть в spec.

**G. throw в expression position** — sql.nv. По spec throw — это
   statement (D25). Файл ожидает что throw возвращает Never и работает
   как expression.

## Что делать

Это **не приоритетные баги**, а **gap'ы между spec-aspiration и
bootstrap-возможностями**. Рекомендуемая последовательность:

1. **Spec-clarification:** проверить group A (char литералы),
   group B (&), group F (effect-type), group G (throw expr).
2. **Парсер-доработки:** group C (newline-tolerance в if-else цепях),
   group D (vec.nv).
3. **Codegen-доработки:** group E (anonymous record literal — ввести
   inferred-type-coercion как при obvious-context'е).

После каждой группы — recompile и продвигаться по списку. Финальная
цель — **11/11 stdlib examples PASS** через `run_tests.ps1 -IncludeStdlib`.
