# Отчёт облачной сессии: `match` на применённой сумме (ветка `p274-carina-appsum`)

База: `origin/p274-carina-cloud` = `8405e5fcd`. Ветка от неё, `origin/main` не вливался.
Среда: Linux (облако), `libgc-dev` поставлен, оракул собран `cargo build --release`.

```
ПРАВИЛО: `match` на `Option[T]` и `Result[T, E]` проходит проверку и компилируется. Список
  применённых сумм с тегами оболочки — ОДНА дверь `sem.is_shell_applied_sum` (Option арности 1,
  Result арности 2); её спрашивают и проверка (`@report_if_applied_sum`), и эмиттер
  (`@emit_tag_test`). Ветви судятся как у простой суммы: вариант через объявление, payload через
  `payload_as_seen`, исчерпанность — теперь и на применённой сумме (`check/exhaustive.nv`).
  Эмиссия теста тега — новый файл `emit_c/emit_applied_sum.nv`:
    Option — через дверь Option, обе формы оболочки: `.tag == NOVA_TAG_Option_Some/None`
      (значение) и `.value != NULL` / `.value == NULL` (бестеговый указатель на запись);
    Result — экземпляр оболочки за указателем: `->tag == NOVA_TAG_Result_Ok/Err`
      (`sem.c_shell_tag`), тип `NovaRes_<T>_<E>*` (семейное имя в `sem/mangle_instance.nv`).
  Payload — прежними чтениями понижения, они и есть раскладка оболочки: `.value` у Option,
  объединение `->payload.<V>._0` у Result. Представление сумм в оболочке НЕ менялось —
  развилки «значение против указателя» не возникло: нужны были только имена тегов.
  Попутно (без этого Result из std не работал): аргумент типа `Name[Args]` читается как ТИП,
  а не как имя — `Option[[]int]` и `Result[[]u8, IoError]` больше не схлопываются в
  `Option[int]` / `Result[u8, IoError]` (`sem/typeref.nv`); Option над экземпляром, которого
  не встречала проба оболочки (`NovaOpt_Nova_Vec____nova_int_p`), Карина объявляет сама
  (`emit_c/emit_decls.nv`). /
ОСТАЛОСЬ ПОД ОТКАЗОМ:
  1) любая другая применённая сумма из переданных деклараций — достижимая сегодня
     `Outcome[T]` прелюдии: оболочка пишет её без поэкземплярной структуры, читать это написание
     Карина не умеет; отказ по имени, текст называет сумму и что Карина пишет только Option и Result;
  2) обобщённые суммы ПРОГРАММЫ (`type Tree[T] enum …`) до `match` не доходят — их объявление
     отвергается раньше («generic parameters are not compiled yet», E2-b2): экземпляров таких сумм
     Карина ещё не эмитит, поэтому третий пункт эмиссии не делался;
  3) `Result`, построенный самой программой (`Ok(1)` / `Err("x")`), — отвергается раньше (E2-b3:
     тип-аргумент не выводится из payload). Поэтому `Result[int, str]` в фикстуре нет: Result
     приходит только из std-вызова (`read`, `read_dir`);
  4) вложенный вариантный образец `Some(Some(x))` — ICE эмиттера, и это НЕ свойство применённой
     суммы: на простой сумме база `8405e5fcd` падает так же (дефект Д3 ниже). /
ФИКСТУРЫ: match_applied_sum/pos_1: байт-в-байт да (novac-e1-smoke; Option над int/str/записью/
  `[]int`, Result из `read` и `read_dir`; оператор, тело-значение, инициализатор, хвост блока,
  `match` в ветви другого; ветви `_`, связыватель, страж-условие);
  match_applied_sum/neg_1 (неисчерпанный на Option, «leaves a variant uncovered») — пришпилен;
  match_applied_sum/neg_2 (`Ok` в `match` на Option, «names a variant of a different sum») — пришпилен;
  applied_sum_arm/neg_1 переведён с `Option[int]` на `Outcome[int]` (остающийся отказ) — пришпилен.
  Соседние pos-фикстуры (option_ctor, iflet_else, applied_sum_arm, match_bool, qualified_arm,
  record_variant, shared_variant, or_pattern_binder, sum_eq, return_arm, tail_if_let_value,
  vec_own, handed_free_fn) — байт-в-байт; исход `check` по ВСЕМ фикстурам совпал с базой, кроме
  новой pos_1 (8 -> 0 ошибок).
  Проба в обе стороны: да — (а) вернул отказ `@report_if_applied_sum` -> pos_1 красная (7 отказов),
  снял -> байт-в-байт; (б) вернул правило исчерпанности к одному `TkSum` -> neg_1 без ошибки,
  восстановил -> ошибка есть. /
МЕРА 0.2: 8405e5fcd 145, ICE 0 -> после 139, ICE 0; double-build 122/142 -> 125/144 (в знаменателе
  +2 новых файла, оба приняты; плюс `parse/parse.nv` принят целиком — его единственными
  диагностиками были 2 отказа на `Option[Node]`; S5 по-прежнему не начат — предусловие не выполнено).
  Отказ «a `match` on an applied sum»: 12 -> 0.
  Новое в телах match (6, все в `main.nv`): `bytes.to_str()` на `[]u8` — «this call fits more than
  one method… (D84)» ×4 (L147, L189, L214, L497); `e.kind.is_dir()` — «the shell novac links into
  carries no `is_dir` for FileType» ×1 (L199); `!nm.ends_with(".nv")` — «prefix operator …
  dispatches through a protocol (D46)» ×1 (L207), каскад от неоднозначного `to_str` в L189.
  Тела в `read_dir` стали видны только благодаря починке чтения типа: payload `entries` теперь
  `[]DirEntry`, а не `DirEntry`. /
ГРУППЫ (топ-10 после): 27 different numeric types (D405) · 24 cast not compiled (E2-b) ·
  24 arm on a string or char literal (E2-b) · 6 no such method in the declarations ·
  6 not a callable novac knows (E2-b3) · 6 bare `None` has no type (E2-b3) ·
  5 fits more than one method (D84) · 4 unknown name · 4 the pattern … every payload field (D59) ·
  3 needs a mutable place (P14). /
СТРАЖИ: `novac-gate-guards.sh`: 98 запущено, 7 тяжёлых пропущено раннером; на финальном дереве
  96 ok, красных 2 -> 1 после последнего коммита:
  check-novac-commit-no-simplification -> известное (раннер передаёт `.` как файл сообщения,
    Errno 21); через швы стража каждый из 4 коммитов ok («упрощений внесено 1, все со сроком» у
    коммита волны, 0 у остальных);
  check-novac-line-length -> починен коммитом cf9762b27 (перепроверен отдельно вместе с
    mangling-one-way, no-default-branch, surface, file-size — все ok).
  По пути (первый прогон): check-novac-no-default-branch и check-novac-mangling-one-way — по
    `sem/mangle_instance.nv`, починены (коммит 2d1f722b3); check-novac-surface — база поднята со
    строкой хроники (builtins 59->60, sem 322->324: `RESULT_FAMILY_C`, `is_shell_applied_sum`,
    `c_shell_tag`, у каждого назван спрашивающий вне модуля); check-novac-local-only-work — ветка
    не была запушена, после пуша зелёный; check-novac-time-ledger — среда: клон был неглубоким,
    после `git fetch --unshallow` зелёный. /
МОДУЛЬНЫЕ ТЕСТЫ: novac/src/pipeline: прошло (новый `match_applied_sum_test.nv`, 4 теста с
  контролями: простая сумма сохраняет `NOVAC_TAG_`, `Outcome` отвергается по имени, исчерпанность
  и чужой вариант судятся, `Option[[]str]` против `[]int` по-прежнему отказ);
  novac/src/check: прошло. /
ОБОЛОЧКА: правок shell.tpl.c нет. /
КОММИТЫ:
  0da3fb2e8 novac: a type argument is read as a TYPE -- Option[[]int] is no longer Option[int]
  6e9981ec8 novac: a match on Option and Result compiles -- the shell's tags, the shell's instances
  2d1f722b3 novac: the Result family spelled in the Option spelling's shape; surface base raised
  cf9762b27 novac: the Result family name over two locals -- the line stays under 120
  (+ этот отчёт). Строк журнала времени не добавлял: потолок долей за день достигнут. /
ОТКРЫТО: Д3, Д4, Д5 ниже; `Outcome[T]` и обобщённые суммы программы — под отказом (см. выше). /
ВОПРОСЫ ИНТЕГРАТОРУ: см. раздел ниже — два.
```

## Дефекты (номера даёт интегратор)

**Д1 — аргумент типа читался как ИМЯ (починен в 0da3fb2e8).** `sem/typeref.nv`: ветка
`Generics` ссылки на тип читала аргументы дверью объявления `names_of_generics` (первый
идентификатор каждой записи). Итог — ТИХО неверный тип вместо обещанного докой `no_ty()`:
`Option[[]int]` -> `Option[int]`, `Result[[]u8, IoError]` -> `Result[u8, IoError]`. Видно было
прямо в тексте меры: отказы называли `Result[u8, IoError]` и `Result[DirEntry, IoError]` у
вызовов, возвращающих срезы. Проба на базе: `fn f(o Option[[]int]) -> int => match o { Some(v)
=> v.len() None => 0 }` + `fn g(v []int) -> Option[[]int] => Some(v)` — «no such method» и
«cannot return `Option[Vec[int]]` from `-> Option[int]`». Класс: К7-соседний — неверный тип
молча. Контроль в тесте: `Option[[]str]` против `Some([]int)` — отказ остаётся.

**Д2 — исчерпанность не судилась на применённой сумме (починен в 6e9981ec8, был латентным).**
`check/exhaustive.nv` выходил раньше на всём, что не `TkSum`. Пока голова `match` отвергала
применённую сумму, это было недостижимо; первая сборка волны ПРИНЯЛА `match o { Some(v) => v }`,
который оракул отвергает (E_MATCH_NON_EXHAUSTIVE). Закреплён `neg_1` и пробой в обе стороны.

**Д3 — вложенный вариантный образец роняет эмиттер (НЕ починен, существующий).** На базе
`8405e5fcd`, на ПРОСТЫХ суммах:
```
type Inner enum P(int) | Q
type Outer enum X(Inner) | Y
fn deep(a Outer) -> int => match a {
    X(P(n)) => n
    X(Q) => -1
    Y => -2
}
fn main() { println(deep(X(P(5)))) }
```
`novac check` — 0 диагностик, `novac emit` — ICE «channel.nv:529: no type recorded for this node».
Проверка принимает форму, которую понижение не умеет (тест внутреннего тега не строится — в
`emit_match.nv` прямо сказано «patterns are one level deep»). Класс: проверка шире эмиссии.
Предложение: либо отказ по имени на вложенном ВАРИАНТНОМ подобразце в `match_arms.nv`, либо
понижение вложенного теста тега (вторая — отдельная волна).

**Д4 — `bytes.to_str()` на `[]u8` неоднозначен (D84), 4 места в `main.nv`.** Открылось в телах
снятых `match`. Это та же группа, где до волны стоял 1 случай (`out.to_str()` в pipeline.nv); в
фикстуре воспроизводится тем же текстом. К моему классу не относится, не трогал.

**Д5 — `o is Some` на `Option` отвергается с неверным текстом.** Карина: «`is` tests a value of
type `any` or a sum — the type of this value is known statically (E_IS_NON_ANY)», а `Option` —
сумма; оракул программу принимает. Проба: `fn f(o Option[int]) -> int { if o is Some { return 1 }
0 }`. Тот же класс, что эта волна (тест тега на применённой сумме), но другая дверь
(`emit_c/emit_is.nv` + проверка `is`); не трогал, чтобы не расширять волну.

## Вопросы интегратору

1. **Страж `check-novac-mangling-one-way` и правило «`_p` вместо `*`».** Правило П24 запрещает
   строковую операцию на результате двери мэнглинга; регулярка стража ловит `c_type(...).op(`,
   только если внутри скобок нет вложенного вызова. Семейное имя Option всегда писалось
   `c_type(ctx, ctx.tys.arg(t, 0)).replace("*", "_p")` и проходит лишь поэтому; так же
   `mono_tuple_name` и `option_payload_is_pointer` (`.ends_with("*")`). Моя первая редакция
   вынесла это в помощник `c_type(ctx, a).replace(...)` и покраснела; я вернул прежнюю форму
   (коммит 2d1f722b3 так и говорит). Решить: (а) завести в двери мэнглинга функцию «написание
   типа внутри имени» (`*` -> `_p`) и вопрос «передаётся ли тип указателем», а регулярку
   расширить на вложенные вызовы; (б) признать `*` -> `_p` законным ВНУТРИ модуля мэнглинга и
   исключить его из правила. Рекомендую (а): это одна функция и три места, и тогда страж ловит
   то, что заявляет, а не то, что пропускает регулярка.
2. **`is` на применённой сумме (Д5).** Сделать тем же путём — `emit_is.nv` спрашивает
   `is_shell_applied_sum` и `@applied_sum_tag_test`, проверка `is` пускает Option/Result —
   небольшой следующей волной? Рекомендую да: дверь уже есть, и иначе `match` и `is` на одной
   сумме отвечают по-разному.
