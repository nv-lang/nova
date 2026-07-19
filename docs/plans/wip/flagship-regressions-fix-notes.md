# Флагман-regressions: латентная strict-effects дыра — фикс-заметки

**Дата:** 2026-07-20. **Модель:** sonnet. **Worktree:** `../nova-flagregr` (branch `p-fix-flagship-regressions`).

## Симптом

`nova check --strict-effects examples/flagship/aggregator` (вся папка, вкл.
`regressions/`) → FAIL 1: `monotonic_now_bare_binding.nv:16` —
`E_UNDECLARED_TRANSITIVE_EFFECT` call to `Monotonic.now` requires effect
`Time`. Латентно, потому что флагман-гейт (CI/скрипт) гонит только
`src/main.nv`, а `regressions/*` — own-CU подпапки (перенос 71f732f3e Plan
187), под `--strict-effects` никогда не гонялись.

## Прогон по ВСЕМ regressions/-подпапкам (before fix)

| Подпапка | `check --strict-effects` |
|---|---|
| errorkind_variant_arity_collision | OK (только unused-import warn) |
| interp_to_str_fallback_valuerecord_recv | OK |
| **monotonic_now_bare_binding** | **FAIL — E_UNDECLARED_TRANSITIVE_EFFECT ×3 (строки 16,17,23)** |
| nested_spawn_scope_var | OK |
| serde_encode_pointer_op | OK |
| spawn_throw_multifield_payload | OK |

Только ОДНА красная — `monotonic_now_bare_binding`. Других латентных дыр не
найдено (не только monotonic — прогнал весь список, остальные 5 чисты).

## Диагноз

`fn m187_monotonic_bare() -> int` и
`fn m187_monotonic_bare_plus_duration(budget Duration) -> int` зовут
`Monotonic.now()`, но не несут `Time` в эффект-row сигнатуры. Компилятор
прав (D62 §Правило 1) — это неполная эффект-декларация фикстуры, не
компилятор-регрессия. Рабочий `aggregate.nv:175`
`fn deadline_elapsed_ms(t0 Monotonic) Time -> int` — образец корректной
декларации.

Как test-блоки разрешают эффект: сверился с `spawn_throw_multifield_payload.nv:46`
(`ro t0 = Monotonic.now()` прямо ВНУТРИ `test { }`, без обёртки
`with Time = handler {}`) — этот тест УЖЕ проходил `--strict-effects` ДО
фикса. Значит `test { }`-блоки несут ambient-разрешение эффектов (не
подпадают под E_UNDECLARED_TRANSITIVE_EFFECT), а дыра была именно в
эффект-row самих `fn`. `with Time = ...` НЕ потребовался.

## Фикс

`examples/flagship/aggregator/regressions/monotonic_now_bare_binding/monotonic_now_bare_binding.nv`:
- `fn m187_monotonic_bare() -> int` → `fn m187_monotonic_bare() Time -> int`
- `fn m187_monotonic_bare_plus_duration(budget Duration) -> int` → `fn m187_monotonic_bare_plus_duration(budget Duration) Time -> int`

Суть регресс-фикстуры сохранена: bare-binding `ro t0 = Monotonic.now()` БЕЗ
аннотации типа `Monotonic` — не тронут (это ось [M-flagship-monotonic-now-bare-binding-ice],
независимая от эффект-оси). Изменён ТОЛЬКО эффект-row.

## Верификация

- `check --strict-effects` на `monotonic_now_bare_binding/` → OK (только
  unused-import warn).
- `nova test` на всех 6 regressions/-подпапках → PASS 6 / FAIL 0 (ICE не
  вернулся).
- `check --strict-effects` на ВСЕЙ папке `examples/flagship/aggregator` —
  наткнулся на ОТДЕЛЬНЫЙ, НЕ связанный с этой задачей, дефект: конкурентное
  слияние `p-consume-enforce-a` (merge `bcb3c6bd6`, докатилось на `main`
  ПОСЛЕ точки ветвления этого worktree) добавило правило
  `E_CONSUME_PATTERN_REQUIRED`; после синка ветки с `main`
  (`git merge main`, fast-forward, без конфликтов) фикс подтверждён
  воспроизводимым и на ПРИСТИННОМ `main` (не тронутом этой задачей): FAIL на
  `src/app/aggregate.nv` и `src/main.nv` из-за закешированной зависимости
  `nova-tls` (`~/.nova/git/co/nova-tls-.../src/handshake_test.nv:29`,
  `Ok(s)` без `consume`) — pinned-хэш `nova-tls` в lockfile ещё не
  подхватил consume-миграцию. Это ВНЕ рамок задачи (отдельный, уже
  идущий план consume-enforce-a/216/217) — НЕ трогал.
- Целевой набор (regressions/*) под `--strict-effects` — FAIL 0. Прогнал
  индивидуально по каждой подпапке ПОСЛЕ синка с main — все 6 чисты
  (включая monotonic).

## Предложение (не реализовано без слова владельца)

Включить `regressions/` в авторитетный флагман-гейт, чтобы латентные
strict-effects дыры ловились впредь.

**Точное место:** `.github/workflows/nova-gate.yml`, шаг «Flagship examples
gate — build under --strict-effects (5 targets)» (строки 177-213). Сейчас
это `nova build <entry.nv> --strict-effects -o ...` по фиксированному
списку из 5 одно-файловых entry-point'ов (`targets=(...)` строки 182-188),
единственная aggregator-запись — `"aggregator|examples/flagship/aggregator/src/main.nv"`
(строка 183). `regressions/*` НЕ входит.

**Почему `nova build`, а не `nova check` для regressions/:** `nova build`
на entry-point требует `fn main` (или как минимум путь до исполняемого
таргета); `regressions/*/X.nv` — test-only фикстуры (только `fn` + `test {
}`, без `main`) — под `nova build` не соберутся структурно (не про
strict-effects). Для regressions/ нужен `nova check --strict-effects` (тот
же флаг, но type-check, не сборка exe) — это ровно то, чем я верифицировал
фикс здесь.

**Предлагаемая правка шага** (после существующего `for entry in
"${targets[@]}"; do ... done`, тем же шагом или отдельным):

```bash
echo "::group::check flagship regressions under --strict-effects"
regr_failed=""
for d in examples/flagship/aggregator/regressions/*/; do
  name="$(basename "$d")"
  if ! ./nova-cli/target/release/nova check --strict-effects "$d"; then
    regr_failed="${regr_failed} ${name}"
  fi
done
echo "::endgroup::"
if [ -n "${regr_failed}" ]; then
  echo "::error::flagship regressions strict-effects check failed for:${regr_failed}"
  exit 1
fi
```

Именно ПО ОТДЕЛЬНОСТИ на каждую `regressions/*` подпапку (не всей
`regressions/` разом, и НЕ всей `examples/flagship/aggregator` разом) —
иначе current-CU объединение ловит cross-cutting дефекты вне
regressions-фикстур как таковых (см. выше: nova-tls consume-lag зацепляет
`src/main.nv`/`src/app/aggregate.nv` при whole-folder прогоне — шум, не
относящийся к regressions-контракту).
