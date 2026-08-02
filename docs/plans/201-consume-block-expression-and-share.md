<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 201 — `consume X { }` как выражение + `@share()`-канон (alias vs Clone)

**Статус:** ✅ ЗАКРЫТ 2026-07-13 [sonnet, worktree `nova-174`, ветка `d188-consume-block`] (D188 v1/v2/multi-var/v3/v3.1 + `@share`/refcount в nv + M-178 прямой move в consume-поле; спек-амендменты в тех же слияниях; conformance 104/0).
**Приоритет:** СРОЧНО (владелец 2026-07-13). **Спека:** амендмент D188 + share-переименование —
В ТЕХ ЖЕ слияниях (язык-меняющее).

## Решения владельца (2026-07-13, зафиксированы дословно по обсуждению)

1. **`consume X { body }`** — короткая форма D188: re-consume СУЩЕСТВУЮЩЕГО owned-биндинга
   (consume-параметр / owned-локал); cleanup exactly-once на всех путях (R2).
2. **Блок — ВЫРАЖЕНИЕ.** Единственная легальная форма выноса владения — **tail-значение `X`
   или `return X`** (голый `X`, не `f(X)`): на этом пути cleanup ДИЗАРМИТСЯ, владение уходит
   результату блока / из функции.
3. Остальные формы выноса — передача `X` в **consume-параметр** и присваивание в
   **consume-поле** — ошибка **`E_CONSUME_BLOCK_MOVE_OUT`** (сейчас падают generic
   ro-диагностикой — заменить на точную). Внутри блока `X` = ro-view + методы.
4. **give/release/дизарм-примитив — ОТВЕРГНУТЫ** (обсуждены варианты Rust
   ScopeGuard::into_inner / Swift consume / Zig errdefer; errdefer у нас ретрактирован D189).
5. **`@share()` вместо `clone` для alias-семантики** («второй владелец того же ресурса,
   закрывает последний»): Clone-протокол у нас = независимая глубокая копия — имя clone для
   alias запрещено. Переименовать **`ChanWriter.clone()` → `share()`** (+ ChanReader/Channel,
   если есть), добавить **`TcpStream @share()`**.
6. **TcpStream @share() — refcount ЦЕЛИКОМ В NV** (конвенция §3): разделяемый handle-рекорд
   `fd + rc AtomicInt` (std/runtime/sync), `share` = fetch_add + owned-копия обёртки;
   `close`/cleanup = fetch_sub, реальный extern-close только при 0. НОЛЬ нового C.
   (OS-dup отвергнут: межпроцессный молоток, та же alias-семантика, дороже.)
7. Канонический TLS-паттерн (заменяет ручные `stream.close()` ×8 в nova-tls connect/accept):
   ```nova
   ro s = consume stream { ...handshake, ошибки → cleanup...; stream }
   Ok(TlsStream.wrap(s, session))
   ```

## Объём

- Спека: амендмент D188 (форма-выражение, tail-вынос, E_CONSUME_BLOCK_MOVE_OUT, Rejected:
  give/release) + share-переименование в channels-доке.
- Компилятор: парсер (`consume IDENT {` без `=`), чекер (owned+Cleanup[E]; точная диагностика;
  tail/`return X`-дизарм в consume-анализе), desugar (существующий D188-механизм + значение блока).
- std: `impl Cleanup` TcpStream + `TcpStream @share()` (nv-refcount); Channel-семейство
  clone→share (emit-строка `nova_chan_writer_clone`, docs/guide/channels.md(+.ru), call-sites).
- nova-tls: `impl Cleanup` TlsStream; connect/accept на канон п.7 (раскладка `tls.tls` пока
  как есть — уплощение = отдельный вопрос D78, см. открытые).

## Взаимодействие с `parallel for` / `spawn` (капчур после 173.1 — владелец 2026-07-13)

Нововведение 173.1: капчур в `spawn`/`parallel for` стал by-value (`by_value = !is_mut`). Для
consume/linear-типов это ОПАСНО: by-value копия обёртки (`TcpStream{fd,...}`) в N файберов = N
алиасов одного ресурса БЕЗ учёта владения → double-close/интерференция.

Правила пакета 201:
1. **Захват owned linear-значения (consume-тип) в тело `spawn`/`parallel for` — ОШИБКА чекера**
   (если существующий D133-анализ это уже ловит — добавить точную диагностику; если НЕ ловит —
   это дыра, закрыть здесь: `E_LINEAR_CAPTURE_IN_FIBER`, текст «возьмите @share() per-fiber»).
2. **Канон многофиберного доступа = `@share()` на каждую итерацию/файбер**
   (паттерн docs/channels: `ro w = tx.share(); spawn { ...w... }`) — refcount гарантирует
   «закрывает последний».
3. `consume X { ... }`-блок ЦЕЛИКОМ внутри тела итерации (свой ресурс на итерацию) — легален,
   без особенностей.
4. Тест-матрица: neg capture-consume-в-spawn/parfor; pos share-per-fiber (tcp и chan);
   pos consume-блок-в-итерации.

## Гейты

conformance один-CU зелёный (база 98) + d188-тесты (pos: tail-вынос, cleanup-пути; neg:
E_CONSUME_BLOCK_MOVE_OUT ×2 формы, не-owned, не-Cleanup) + share-тесты (tcp: оригинал закрыт →
клон жив → последний close = один real-close; конкурентные share; chan multi-writer под новым
именем); targeted std/net, std/http, nova-tls.

## Открытые (НЕ в этом плане)

- D78-амендмент «корневой `module <package>` для файлов в `src/`» (убить `tls.tls`) — ждёт
  отдельного решения владельца.
- Ретро-аудит прочих alias-под-именем-clone в std (кандидаты ищутся при переименовании).
