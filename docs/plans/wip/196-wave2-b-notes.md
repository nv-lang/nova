<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 196 — ВОЛНА-B (sonnet, worktree `nova-w2b`, ветка `p196-wave2-b`)

**Родитель:** [196-wave2-map.md](196-wave2-map.md) §ВОЛНА-B (Q10-GEN). **Задание:** дожать
`resolve_mono_type_args` → канальный `_ch`-вариант, снести legacy-движки, зона `emit_c.rs` 19803-21096.
**База:** main `d9af43662`.

---

## 0. Перечитка зоны по свежему коду (координаты сдвинулись против карты, drift ~декалей строк)

- `resolve_mono_type_args` (легаси Q10-движок #1) — сейчас **19840** (карта: 19803, drift+37).
- `resolve_mono_type_args_ch` (канальная обёртка) — **20269** (карта: 20232).
- `compose_mono_type_args_ch` (Stage-C2 POST-mono композер) — **20373**.
- `infer_type_param_binding_from_ref` — **20550**, `infer_protocol_structural_binding` — **20610**,
  `infer_type_param_binding` — **20744** (карта: 20707).
- `resolve_instance_call_subst` (W1-i.A консолидатор) — **20967**.
- `resolve_method_level_subst` (легаси Q10-движок #2, но УЖЕ со встроенным channel-first) — **21133**
  (карта: 21096), тело до **21489** (карта недооценила размер — реальная функция куда длиннее
  заявленных координат; `find_first_throw_value` идёт следом на 21498).

**Call-site инвентарь (по коду, не по карте):**
- `resolve_mono_type_args` (сырой легаси) — **1 caller**: `resolve_mono_type_args_ch:20276` (легаси-плечо
  гейта). Прямых внешних вызовов НЕТ — обе точки входа emit_call (39440, 39763) уже идут через `_ch`.
  **Задача «перевод оставшихся call-сайтов на _ch» — УЖЕ ВЫПОЛНЕНА в текущем main** (не моя правка).
- `resolve_method_level_subst` — **5 callers** (34194, 34736, 35004, 37733, 38029, все вне frozen-зоны
  50542-52681) — функция САМА по себе channel-first (Stage-B2 встроен внутрь тела, не отдельная `_ch`-обёртка).
  Отдельного «сырого легаси»-варианта без канала не существует — Steps 1/2/2f/3 являются fallback-веткой
  ВНУТРИ этой же функции, не отдельной функцией. «Перевод на `_ch`» здесь не имеет смысла как отдельный
  шаг — уже единая точка входа.

**Census «три hand-duplicated engines» (доккомент строки 19826-19831, датирован 2026-07-12):**
третий движок («инлайн instance-method dispatch») **УЖЕ консолидирован** в `resolve_instance_call_subst`
(W1-i.A, доккомент 20917 «ЕДИНЫЙ POST-mono резолвер, консолидирующий... 6 legacy-армов»). Проверено грепом:
`resolve_instance_call_subst` имеет 4 caller'а — 1 вне frozen (47141, SHARED `infer_mono_method_ret_with_args`)
+ 3 внутри frozen (50948/50981/51109, капстоун B06a/B07/B07r). **Параллельного легаси нет** — доккомент у
`resolve_mono_type_args` о «трёх независимых движках» УСТАРЕЛ (описывал состояние ДО W1-i-консолидации);
актуально — ДВА легаси-Q10-движка (`resolve_mono_type_args` + `resolve_method_level_subst`), оба уже с
каналом, третий давно слит.

---

## 1. Перепись трафика — метод

Механизм трассировки для ЭТОЙ зоны — **`NOVA_NODE_SUBSTS_TRACE=1`** (не `NOVA_TRACE_ICR`, который гейтит
`icr_trace` — debug-only механику ТОЛЬКО frozen-зоны `infer_call_ret_c`/капстоуна). `NOVA_NODE_SUBSTS_TRACE`
НЕ гейтится `#[cfg(debug_assertions)]` (проверено грепом — голый `std::env::var_os`, без cfg-обвязки) →
**release-бинарь трассирует так же, как debug**, к тому же даёт количественные per-call-site hit/fallback
теги (`hit`/`hit-composed`/`fallback=miss|incomplete|mismatch`), что строго сильнее булева
reachability-сигнала `icr_trace`. Использован release-бинарь (`cargo build --release`) — быстрее debug на
порядок, для этой зоны эквивалентен по доказательной силе.

## 2. Перепись — std/src/{math,collections,time,encoding}

Команда: `nova test std/src/math std/src/collections std/src/time std/src/encoding --mode release`,
`NOVA_NODE_SUBSTS_TRACE=1`. Результат: **PASS 30/0/16skip** (std/src/math — 0 тестов в наборе, видимо каталог
пуст/поглощён другими; собрано фактически time+collections+encoding).

**`consumer=mono_type_args` (resolve_mono_type_args_ch):** 1027 событий.
| Вердикт | events | distinct call_id |
|---|---|---|
| `hit-composed` | 782 | 111 |
| `fallback=miss` | 245 | 23 |
| `hit` (прямой, не composed) | **0** | 0 |
| `fallback=incomplete`/`mismatch` | 0 | 0 |

**`consumer=resolve_method_level_subst`:** 1044 события (807 hit + 117 hit-composed + 119 fallback).
| Вердикт | events | distinct call_id |
|---|---|---|
| `hit` (прямой per-name) | 807 | 426 |
| `hit-composed` (Step1-early-exit) | 117 | 72 |
| `fallback reason=miss` | 119 | 74 |
| `fallback reason=partial` | 0 | 0 |

Fallback-контексты (`ctx=`) на этом сэмпле — **живой легаси-трафик, не гипотетический**:
`Vec____nova_byte.append` (94×), `Option.serialize` (7×), `str.serialize` (4×), `int.serialize` (3×),
`Vec____nova_int.append` (3×), `f64.serialize`/`Vec____nova_str.append`/`Vec____nova_int.extend`/
`Vec____Nova_Vec____nova_int_p.append`/`VecIter____nova_int.filter_map`/`LinkedList____nova_int.map`/
`LinkedList____nova_int.fold`/`HashMap____nova_str__NovaValue_ValRec.serialize` (1× каждый).
Доминанта = точный мотивирующий кейс из доккомента R2 (`fn Vec[T] mut @append[S AsSlice[T]](other S)`,
`S` только из arg-C-типа, residual на checker-уровне).

**★ НАХОДКА (логическая + эмпирическая): прямая «hit»-ветка в `resolve_mono_type_args_ch` (строки
20328-20359 по факту, «byte-for-byte per-name» проверка ПОСЛЕ `compose_mono_type_args_ch`) — 0 hits на
1027 событиях.** Доказательство by construction (не только выборка): `compose_mono_type_args_ch` для
КАЖДОГО `fn_decl.generics[i]` уже пробует channel-by-name (с тем же `resolved_type_to_c` лоурингом) ДО
turbofish-фоллбэка; прямая ветка требует `channel.len() == generics.len()` + те же имена + тот же лоуринг
— т.е. ЛЮБОЙ случай, где прямая ветка способна дать `hit`, `compose_mono_type_args_ch` уже даст
`hit-composed` РАНЬШЕ (composed вызывается первым). Обратное (несовпадение имён/лоуринга) роняет ОБЕ ветки
в fallback идентично. **Прямая ветка структурно недостижима** — composed её строго покрывает. Кандидат на
снятие (не «легаси-движок» Q10 в терминах карты, а внутренняя дублирующая ветка САМОЙ `_ch`-обёртки) —
census-ловушка «параллельного legacy нет» истолкована широко: эта дублирующая ветка — ровно такой случай.

## 3. Вердикт по легаси-движкам (промежуточный, до полного корпуса)

И `resolve_mono_type_args` (2), и легаси-fallback `resolve_method_level_subst` (Steps 1/2/2f/3) —
**НЕ 0-hit уже на узком std-подмножестве** (23 + 74 distinct call sites, реальные конформанс-формы:
`AsSlice[T]`-fluent-generic append, Serialize-деривация). Снос ОБОИХ движков **не валиден** без
дальнейшего расширения канала (Zone CH, types/mod.rs — вне моей зоны/полномочий, СТОП-пункт по заданию).
Продолжаю перепись на остальном корпусе для полноты картины, затем — точечная правка дублирующей
`_ch`-ветки (детач+panic-верификация) как единственного валидного снятия в этой волне.

---

(продолжение — d-фикстуры/standalone/aggregator и итоговая правка — следующие чекпоинты)
