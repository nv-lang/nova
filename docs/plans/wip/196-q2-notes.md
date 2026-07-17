<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 196 — Q2-хвост: `u64.try_from` в std/data (sonnet, worktree `nova-q2tf`, ветка `p196-q2-tryfrom`)

**Родитель:** `docs/plans/wip/196-ret-notes.md` §3 (Q2, `infer_method_level_return_for_sum`) — «остаток 16 хитов
в std/src/data, структурный класс `u64.try_from(a)` (D77-интринсик, намеренно стёртая репрезентация), отдельный
follow-up». **Задание этой сессии:** разобрать класс ДО КОНЦА (вердикт а/б, не половинить).

---

## Итог одной строкой

**Вердикт (б) — ЗАКОННЫЙ post-mono residual, снос/продюсер НЕ делается.** Все 16 хитов — ОДИН структурный
класс (`Result.ok` на receiver'е erased-репрезентации `u64.try_from(...)`), независимо переизмерен (свежая
временная non-dedup инструментация, `NOVA_TRACE_MLRFS_COUNT`, ревертнута до коммита) — **16/16, 0 дрейфа**
от замера волны-2 (`196.5-stage-d-wave2-notes.md`). Причина принципиальная (не пробел канала): B12h
(frozen) хардкодит C-тип receiver'а `NovaRes_nova_int_nova_str*` ВСЕГДА, независимо от фактического T/E —
честная аннотация `Result[u64,ParseIntError]` в канале разошлась бы с фактическим эмитом (Err — сырое
`nova_str`-сообщение, НЕ сконструированный `ParseIntError`). Продюсер-фикс невозможен без ПРЕДВАРИТЕЛЬНОГО
Plan 59 Ф.7.5 D3 (typed Result mono для этого интринсика) — отдельный, крупный, вне объёма Q2/Zone CH проект.
Найдена и задокументирована **миз-классификация** в `196-capstone-notes.md` §3.4: `B12h_path_try_from` сидит
в списке «33 core, ждут Zone CH channel-расширения», хотя структурно это ТОТ ЖЕ класс, что уже верно
классифицирован в §3.2 как «SHARED/структурно-заблокировано, блокер = Plan 59 Ф.7.5 D3, НЕ Zone CH» (его
родные соседи B11q/B11r). Предложение реклассификации — §4 ниже.

---

## 1. Карта сайтов

**Текстовые call-сайты в std/src/data** (грепом, ЕДИНСТВЕННЫЙ файл с `try_from`):

| Файл | Строка | Форма | Функция | Кормит |
|---|---|---|---|---|
| `std/src/data/semver.nv` | 234 | `match u64.try_from(s) { Ok(n)=>.., Err(_)=>.. }` | `parse_numeric_component` | B12h (receiver-тип), НЕ `.ok()` — не в счёт 16 |
| `std/src/data/semver.nv` | 402 | `u64.try_from(a).ok() ?? 0` | `compare_ident` | B12h → B11r → MLRFS `Result.ok` |
| `std/src/data/semver.nv` | 403 | `u64.try_from(b).ok() ?? 0` | `compare_ident` | B12h → B11r → MLRFS `Result.ok` |

Никаких других файлов в `std/src/data` (`semver_range.nv`, `sql.nv`, `*_test.nv`) НЕ содержат `try_from` —
подтверждено `grep -rn "try_from" std/src/data`.

**Почему 3 текстовых сайта дают 16 динамических хитов:** `nova test std/src/data` (директория, БЕЗ `--filter`)
компилирует ВСЕ 6 файлов директории как оговорённый в test-conventions.md прогон модуля (`nova test std/src/<модуль>`);
`compare_ident` (2 `.ok()`-сайта) транзитивно достижима из НЕСКОЛЬКИХ единиц компиляции этого прогона
(`semver.nv`, `semver_test.nv`, `semver_range.nv`, `semver_range_test.nv` — все либо определяют, либо
импортируют/вызывают `compare_ident` для сравнения pre-release идентификаторов). Solo-прогон одного файла
(`nova test std/src/data/semver_test` — ОДИН файл, БЕЗ соседей директории) даёт **0** хитов того же трейса —
подтверждает: множитель приходит от совместной компиляции директории, не от отдельного файла. Классовая
принадлежность (какой ИМЕННО легаси-путь) от этого не меняется — все 16 гомогенны: `sum=Result method=ok
resolved=true`.

**Трейс-подтверждение (frozen-ветки, читавшиеся, НЕ трогавшиеся):** debug-бинарь,
`NOVA_TRACE_ICR=1 NOVA_TRACE_MLRFS=1`, `nova test std/src/data`:

```
[ICR-HIT] B12h_path_try_from          (frozen, emit_c.rs ~52309 — infer_call_ret_c)
[ICR-HIT] B11r_result_like_methods    (frozen, emit_c.rs ~51910 — infer_call_ret_c)
[MLRFS-HIT] sum=Result method=ok resolved=true   (Q2, infer_method_level_return_for_sum)
```

(Полный список ICR-HIT для контекста в этом прогоне — B06/B06b/B06d/B11a/B01/B07/B07r/B11d/B10j×2/B12p; ни
одна другая sum-method-related ветка ни разу не показала `resolved=false` — легаси НЕ ломается, просто не
channel-first.)

---

## 2. D77-механика — почему представление принципиально стёрто

`u64.try_from(s str) -> Result[u64, ParseIntError]` (комментарии в semver.nv, D77 §«Правило») **НЕ имеет
`.nv`-декларации нигде в std** (грепом: только `u8.try_from`/`char.try_from` — реально объявлены в
`std/src/runtime/char.nv`; численный str→u64/i64/f64/bool/char парсинг — ЧИСТЫЙ codegen-интринсик, без
`FnDecl` в реестре). Emit-путь (`emit_c.rs` ~38906-38984, Member-форма) хардкодит:

```rust
let res_c_ty = self.result_repr_c_type("nova_int", "nova_str");   // ВСЕГДА эти литералы
// Ok: (nova_int) значение (после range-check под фактическую ширину T)
// Err: СЫРАЯ nova_str-строка "{target}.try_from: parse error" — НЕ сконструированный ParseIntError
```

Path-форма (`emit_c.rs` ~52309, B12h, **frozen**):
```rust
if method_name == "try_from" {
    self.icr_trace("B12h_path_try_from");
    return "NovaRes_nova_int_nova_str*".into();   // одна и та же строка для ЛЮБОГО T (u64/u8/i16/f32/bool/char…)
}
```

Т.е. representation shared/erased **по конструкции** (доккомментарий на месте прямо это называет: «Plan 59
Ф.7.5 D3: erased mono Result-инстанс») — ОДИН C-struct `NovaRes_nova_int_nova_str` обслуживает ВСЕ инстансы
численного `T.try_from(str)` независимо от факт. ширины T и от заявленного в спеке E (`ParseIntError`,
`ParseVersionError` и т.п.). Downstream `B11r_result_like_methods` (frozen, ~51910) подтверждает диагноз:
`resolve_result_te(obj, &obj_ty)` восстанавливает пару `(T,E)` **парсингом СТРОКИ типа** (`"nova_int"`,
`"nova_str"`), а не через чекер-канал — это классический post-mono round-trip, а не пробел producer'а.

**Почему чекер не может (и не должен) аннотировать честно:** если бы `resolved_types[call.id]` для
`u64.try_from(a)` материализовал реальный `Result[u64, ParseIntError]` (синтаксически это возможно —
`ParseIntError` существует как объявленный тип), downstream-потребитель канала (маппер TypeRef→C-mono-имя)
вычислил бы C-тип типа `NovaRes_nova_int_Nova_ParseIntError*` (с реально сконструированным полем ошибки) —
**но реально эмитится** `NovaRes_nova_int_nova_str*` с сырым текстовым сообщением. Разъезд честной аннотации
и фактического эмита = сломанный byte-parity / type-mismatch в сгенерированном C. Разблокировка требует
СНАЧАЛА починки самого интринсика (Plan 59 Ф.7.5 D3 — реально конструировать типизированные error-инстансы
на каждую численную ширину), что вне объёма Q2/Zone CH одного окна.

---

## 3. Вердикт: (б) — законный residual, НЕ трогать

Соответствует существующей классификации `196-capstone-notes.md` §3.2 (ветка `p196-capstone`, worktree
`nova-196cap`, НЕ эта сессия) — B11q/B11r там УЖЕ верно помечены «блокер = Plan 59 Ф.7.5 D3, НЕ Zone CH».
Эта сессия НЕЗАВИСИМО пришла к тому же выводу (без чтения capstone-notes ДО собственного анализа) и добавляет:
1. Свежее число (16/16, 0 дрейфа) — временная инструментация, ревертнута (см. §5).
2. Полную причинную цепочку B12h→B11r→MLRFS (а не только «B11r видит 16 хитов»).
3. **Находку миз-классификации** B12h — см. §4.

Никакого продюсер-расширения в `types/mod.rs` эта сессия НЕ делала — честная аннотация невозможна без
предварительного Plan 59 Ф.7.5 D3 (см. §2), продюсер был бы либо ложью, либо no-op (frozen-ветка B12h всё
равно хардкодит erased-тип независимо от канала).

---

## 4. Реклассификация для capstone-интегратора (НЕ применено — другой worktree/ветка)

`docs/plans/196-capstone-notes.md` (worktree `nova-196cap`, ветка `p196-capstone`, монопольная frozen-зона —
эта сессия её НЕ трогала) §3.4 «Core (33)» перечисляет `B12h` наравне с ~32 генуинно channel-блокированными
ветками («не снимаемо без Zone CH channel-расширения»). По факту этого разбора `B12h_path_try_from` —
структурно ТОТ ЖЕ класс, что его сосед `B11r_result_like_methods` (уже в §3.2, «блокер = Plan 59 Ф.7.5 D3,
НЕ Zone CH»). Предлагаемая правка (для следующего капстоун-захода, НЕ для этой сессии — чужая монополия):

- **Убрать** `B12h` из §3.4 (33 → 32).
- **Добавить** в §3.2 существующий пункт про B11q/B11r: «…(включая B12h_path_try_from — та же erased-
  репрезентация receiver'а, ОДИН источник обеих веток)».

Эффект: «33 ядра ждут CH» становится «32 ядра ждут CH + 1 переклассифицирован в терминальную SHARED-категорию
(Plan 59 Ф.7.5 D3, отдельный проект)» — на один пункт меньше в списке, ожидающем Zone CH, без утраты трассируемости
(причина и блокер уже описаны в §3.2 текстом, просто список нужно исправить).

---

## 5. Инфраструктура сессии

- Worktree `d:/Sources/nv-lang/nova-q2tf`, ветка `p196-q2-tryfrom`, база main `99f0021f9`. libuv submodule +
  `target/libuv-cache/libuv.lib` скопированы из main, вложенный `.git`-файл libuv удалён (стандартный рецепт).
- Бинари: `cargo build --release --manifest-path nova-cli/Cargo.toml` (чисто) + `cargo build --manifest-path
  nova-cli/Cargo.toml` (debug, для `NOVA_TRACE_ICR`/`NOVA_TRACE_MLRFS`, `#[cfg(debug_assertions)]`-гейтед).
- **Временная инструментация** (не в финальном диффе): `trace_mlrfs` (emit_c.rs ~47209) на короткое время
  получила доп. non-dedup счётчик под ОТДЕЛЬНЫМ env-гейтом `NOVA_TRACE_MLRFS_COUNT` (мимикрирует уже
  реверченный `NOVA_TRACE_D1HOLE1_ICR` волны-2, тот же метод: «временная трасса, снята revert'ом» —
  `196.5-stage-d-wave2-notes.md` §Дыра-1). Пересобрана debug, замерено (`grep -c "\[MLRFS-COUNT\]"` → **16**,
  все `sum=Result method=ok resolved=true`), затем `git checkout -- compiler-codegen/src/codegen/emit_c.rs`
  (`git diff --stat` пуст ПОСЛЕ ревёрта, подтверждено). Ни одной строки в frozen-зоне (`infer_call_ret_c`,
  emit_c.rs 50289-52381) не тронуто — правка была ТОЛЬКО в теле `trace_mlrfs` (Q2-собственная функция вне
  frozen-диапазона).

---

## 6. Верификация (до/после)

| Прогон | Результат |
|---|---|
| Baseline (release-бинарь, ДО любых правок) `nova test std/src/data` | PASS 3 FAIL 0 SKIP 3 |
| Финал (release-бинарь, ПОСЛЕ ревёрта временной инструментации) `nova test std/src/data` | PASS 3 FAIL 0 SKIP 3 |
| MLRFS non-dedup счётчик (debug-бинарь, временный, ревертнут) `nova test std/src/data` | **16** хитов, 100% `sum=Result method=ok resolved=true` |
| Solo `nova test std/src/data/semver_test` (один файл, для локализации множителя) | 0 хитов того же счётчика (подтверждает: множитель — от совместной компиляции директории) |

**Регрессий нет** (вердикт = документирование, код не менялся). `git status --short` в worktree — чисто
(единственный новый файл — этот notes-документ).

---

## 7. Коммиты этой сессии

1. `docs(196): Q2-хвост — u64.try_from разобран до конца, вердикт (б) законный residual, находка миз-
   классификации B12h для capstone` — этот checkpoint-файл (единственный коммит; код не менялся, временная
   инструментация ревертнута ДО коммита).

**В main НЕ мёржено, push НЕ делался** (по заданию).
