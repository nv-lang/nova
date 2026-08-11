# p109fix — [M-polaris-static-embeddeddir-oom] (bug-sweep №109)

Worktree: `d:/Sources/nv-lang/nova-p109f`, branch `p109fix` off `main` (6fdbbd02b,
уже включает №274-фикс ABI-стабов auto-by-ref). Полярис-сторона: worktree
`d:/Sources/nv-lang/nova-polaris`, branch `p109f-probe` off `main`.

## Проверка «жив/ушёл»

Пересобран свежий release-бинарь на main (6fdbbd02b). Репро сведён живым:
два зачейненных `.get("/a/{*path}", static_handler(...))!!.get("/b/{*path}",
static_handler(...))!!` на реальном `EmbeddedDir` — `nova: out of memory`.
Инструментированный `alloc_boehm.c` (ring-buffer размеров, ВРЕМЕННО, откачен
после диагностики) показал точный порченный размер: **1987590606432 байт
(0x1CEC5A1CE60, ~1.85 TiB)** в одном прогоне, **2776855596640** в другом —
недетерминированно между прогонами байт-в-байт того же бинаря/входа.

Уточнение исходного диагноза «один вызов — работает»: НЕ подтвердилось на
этой машине/сборке — ОДИНОЧНАЯ регистрация `static_handler` тоже падает
(вероятно, luck-зависимо от стек-мусора в исходном наблюдении; корень
одинаков для одного и двух вызовов).

## Локализация

Control-эксперимент: замена `static_handler(...)` на рукописное замыкание
`fn(req) => serve_path(site(), Static.new(), ..., req)` (ровно форма
примера 06 до этого окна) — **не падает**. Значит дефект — в самом
`static_handler`, не в `serve_path`/`EmbeddedDir`/router.

Чтение сгенерированного C (`--keep-artifacts`): `static_handler`'s
`ro c = cfg` эмитится как голое присвоение указателя
(`NovaValue_Static* c = cfg;`), где `cfg` — указатель на STACK-temp
ВЫЗЫВАЮЩЕЙ функции (`(&_nv_tmp_2958)` в call site `build_router()`). Этот
указатель уносится в env эскейпящего `Handler`-замыкания
(`_nv_tmp_3218->_nv_fv_10_c = c;`) БЕЗ deref-копии. После возврата
`build_router()` указатель висит на мёртвом стек-фрейме; к моменту
диспетча запроса (много позже, через `serve_router`/`Router.dispatch`)
чтение `cfg.cache_control`/`.index` (`str={ptr,len}`) читает МУСОР — тот
самый порченный размер, кормящийся дальше в Vec/str-аллокацию.

Корневая причина порядка событий: для НЕ-generic функций такой
by-ref value-record параметр ловится `ref_params`
(`build_free_fn_byref_map`, Plan 172.14) — escaping-замыкание корректно
делает deref-и-heap-box. Для GENERIC функций (`static_handler[F ReadFs]`)
этот pre-pass **структурно** не флагует параметр (собственный
doc-комментарий: pre-pass идёт ДО монофикации, mono-имена ещё не в
`value_struct_field_tys` → «флаг false естественно»). Установленная
[M-16]-идиома «локальная копия перед захватом» (`ro c = cfg`) поэтому
вырождается в голый alias.

## Фикс (emit_c.rs, `emit_lambda` free-var capture classification)

`compiler-codegen/src/codegen/emit_c.rs`, коммит `f44a58ca5`:

- `free_var_is_mut`/`free_var_is_ref_param_src` теперь ТАКЖЕ true для
  свободных переменных, чей C-тип сам является value-struct-указателем
  (`is_value_struct_ptr` — уже существующий name-pattern хелпер, не новый),
  независимо от `ref_params`.
- Новый `free_var_box_ty`: для такого случая box должен снимать
  sizeof/тип с POINTEE (`ty` минус хвостовой `*`), а не с самого `ty` —
  `ref_params`-источники хранят в `var_types` уже дереференцированный тип,
  голый value-struct-ptr alias — сам указатель; смешивать нельзя.
- Внутри тела лямбды `var_types` для захваченного имени временно
  переопределяется на `box_ty` (значение, не указатель) на время эмиссии
  тела — иначе call-argument материализация (передача `c` дальше в
  `serve_path`) рассинхронизируется с фактическим (теперь
  дереференцированным через `var_boxed`) видом переменной.

Маршрутизация переиспользует УЖЕ существующий deref-и-heap-box путь
`ref_params`-источника (`[M-effect-handler-mutex-hashmap-value-capture]`,
2026-08-01) — не новый механизм, вторая инстанциация принятого паттерна.

## Ratchet

`lines` 64311→64384 (+73), `infer` без сдвига (348≤349) — обоснование
внесено в `scripts/guards/arch-ratchet.baseline` тем же протоколом, что и
все предыдущие «Путь B» записи. Коммит `7c94e8674`.

## Фикстура-регресс

`nova-polaris/src/static_test.nv` (коммит `bd2e441`, ветка `p109f-probe`):
новый тест `"static: TWO chained static_handler() registrations both serve
correctly (№109 regression)"` — точная форма репро (два `.get()!!.get()!!`
со `static_handler` на реальном `EmbeddedDir`), в составе стандартного
`./nova.sh test src --strict-effects` (37/0/18).

## Обход снят

`examples/06-static-site`: `/assets/{*path}` переведён на канонический
`static_handler(fs, cfg, "path")`; №109-комментарий и секция README «A gap
this example works around» (en+ru) удалены. Curl+sha256 всех трёх ассетов
через оба маршрута (`/` и `/assets/*`) — байт-в-байт совпадение.

## Гейты (вердикты дословно)

- `polaris ./nova.sh test src --strict-effects` — **PASS: 37 FAIL: 0 SKIP: 18**
  (ровно требуемое 37/0/18, δ0; включает новый регресс-тест).
- `nova check std/src` — **PASS: 147 FAIL: 26 WARN: 60** (байт-в-байт с
  эталоном).
- `scripts/guards/arch-ratchet.sh` — `arch-ratchet ok: lines=64384 <= 64384`,
  `arch-ratchet ok: infer=348 <= 349`.
- Реальный polaris-репро (два `static_handler` на `EmbeddedDir`): 6/6
  запросов 200, sha256 всех ассетов через `/a/*` и `/b/*` совпадают с
  исходными файлами байт-в-байт, процесс жив после всех запросов.
- Пример 06 канонический: sha256 трёх ассетов через `/` и `/assets/*`
  совпадают с исходными файлами.
- `nova lint` изменённых `.nv`-файлов — 0 находок в МОЁМ коде (3 находки —
  в пред-существующем `hdr()`/`ServerRequest_stub()`, вне периметра).

Мега-CU (`spec_tests/conformance`) и флагман (`examples/flagship/aggregator`)
— по канону этого проекта на стороне интегратора, не гонял.

## Не тронуто (сознательно, вне периметра №109)

- `nova_rt/alloc_boehm.c`/`alloc.c` — инструментация для диагностики (ring
  buffer размеров) добавлена и **полностью откачена** (`git diff` пуст) —
  не часть фикса.
- `[M-nova-alloc-abort-no-fflush]` (реестр №278) — родственная находка
  предыдущего p109-окна (отсутствие `fflush` перед `abort()` в
  `nova_alloc`/`nova_alloc_uncollectable`) — НЕ тронул, помечено как
  довесок к отдельному rt-окну, не этому.
- `docs/plans/backlog-followups.md`/`230-polaris-examples.md` — исторические
  narrative-записи про №109 (диагноз-в-моменте) не правил, они архивны;
  авторитетные реестры (`221.1-bug-sweep.md`, `221-release-v0-1.md`)
  обновлены.

## Модель

sonnet (Claude Sonnet 5).
