# p273 — [M-realtime-nogc-silent-no-enforce]

Worktree: `d:/Sources/nv-lang/nova-p273`, branch `p273` off `main` (f9bbd5474).
(NB: `PROGRESS.md` в корне — от другой, уже закрытой задачи (перевод
`///`-комментариев), не трогал; этот чекпоинт-файл — отдельно.)

## Находка (подтверждена, шире исходного отчёта)

1. `nogc_blacklisted_call` (`compiler-codegen/src/types/mod.rs`) — блэклист
   allocating-имён для `#realtime nogc fn` — не включал `.of` (D259
   канонический вариадик-конструктор `Vec[T].of(...)`).
2. **Более глубокий баг**: `check_capabilities_at`'s path-extraction не
   разворачивал `ExprKind::TurboFish` (explicit generic type arg на
   receiver'е, `Vec[int].new()` → `Member{obj: TurboFish{base:Ident("Vec"),..},
   name:"new"}`, D38). Любой вызов вида `Vec[int].*` падал в defensive
   `_ => return`, silently skipping ВСЕ capability-проверки этой функции
   (nogc-alloc, `forbid`, effect-row), не только nogc. Из-за этого
   `Vec[int].new()` тоже НЕ ловился, хотя `.new` был в блэклисте с самого
   начала.

## Фикс (checker-канал, types/mod.rs, ОДИН файл)

- `nogc_blacklisted_call`: добавлен `"of"` в match-арм `[]T.*` и
  `HashMap/Set/Vec/Deque/LinkedList/Lru/BloomFilter.*`.
- `check_capabilities_at`: peel `ExprKind::TurboFish{base,..}` перед
  матчингом obj.kind.
- Оба diagnostic-сообщения получили bracket-коды: `[E_REALTIME_NOGC_ALLOC]`,
  `[E_BLOCKING_NOGC_ALLOC]` (второй — зарезервирован, ветка недостижима,
  `blocking_body_active` мёртв — отдельный tracked-дефект
  `[M-dead-exprkind-blocking-vestigial]`, НЕ трогал).

## Тесты

- `spec_tests/conformance/neg/d172_realtime_nogc_alloc_neg.nv` (новый,
  `.of`) — PASS (negative).
- `spec_tests/conformance/neg/d172_realtime_nogc_turbofish_neg.nv` (новый,
  `.new()` regression guard для TurboFish-фикса) — PASS (negative).
- `spec_tests/conformance/d172_realtime_blocking_attrs.nv` — добавлен
  позитивный scalar-only `#realtime nogc fn` тест — PASS.
- Все три верифицированы `nova test` (полный codegen) индивидуально:
  `PASS: 3 FAIL: 0`.

## Спека

- `spec/decisions/06-concurrency.md` D172: добавлен `#realtime nogc fn` в
  §«Что», новая §7 (механизм/пределы/находка), `E_REALTIME_NOGC_ALLOC` +
  `E_BLOCKING_NOGC_ALLOC` в §3, амендмент в «Эволюция».
- `spec/decisions/04-effects.md` D64: врезка — block-form retracted,
  живая семантика в D172 §7.
- `docs/plans/113-realtime-blocking-attribute-only.md`: исправлена ложная
  запись A5 ✅ (несуществующий тест `negative_capability/realtime_nogc_alloc`).

## Гейты (статус)

- `nova check std/src` — `PASS: 147 FAIL: 26 WARN: 60` — байт-в-байт
  совпадает с эталоном интегратора. Регрессий нет.
- `nova check spec_tests/conformance` (весь каталог, НЕ мега-CU) — см. вывод
  фонового прогона (ожидается).
- Флагман (`examples/flagship/aggregator`) — падает на
  НЕСВЯЗАННОЙ ошибке `TlsStream.read_bytes` (межрепо nova-polaris/nova-tls
  рассинхрон) ДО того, как компилятор доходит до моих изменений; по
  канону интегратора флагман — на его стороне, не гонял дальше.
- emit_c.rs НЕ тронут (0 diff) — ratchet lines<=64311 не мог измениться
  моей правкой.

## Открытый вопрос брифа п.4 (Vec.of из #realtime — whitelist или дыра)

Дыра, не легитимный whitelist. `Vec[T].of` — обычная exported .nv-функция
(`std/src/collections/vec/core.nv:192`), не имеет `#realtime`-аннотации и
не входит ни в какой allow-list для callee-дисциплины `#realtime`
(D172 §4 callee-guarantee модель — про suspend/park/wake, не про
allocation). Она просто не ловилась из-за независимого nogc-alloc-блэклиста,
который был неполным (см. выше). Обычные (без nogc) `#realtime fn` МОГУТ
звать `Vec.of` легально — это не суспенд-эффект.

## Не тронуто (сознательно, вне периметра №273)

- `blocking_body_active`/`ExprKind::Blocking` — мёртвый код,
  уже tracked под `[M-dead-exprkind-blocking-vestigial]`.
- `E_REALTIME_SYNC_PARK`/`WAKE`/`NESTED_SYNC_VIA_FN` — живут в emit_c.rs
  (legacy channel), не в checker-канале; `nova check` их не ловит вообще
  (только `nova test`/`build` через codegen). Пред-существующая
  архитектурная асимметрия, отдельный дефект, не №273.
- Plan 144.0 may-GC анализ (`gc-effect-analyze`) как честная замена
  блэклиста — оценено, не принято: другая фаза пайплайна (пост-mono
  codegen-tier vs pre-mono checker-walk), интеграция — отдельная задача.
