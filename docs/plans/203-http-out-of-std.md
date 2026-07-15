# План 203 — вынос http из std в nv-lang/nova-http

**Статус:** ✅ ЗАКРЫТ 2026-07-13 (Ф.1-Ф.3 влиты — `b6818a137`: http выехал из std в репу-сиблинг nova-http по root peers D78 rev-4, std снова самодостаточен, +2 фикса резолвера).
**Мотив:** std сейчас зависит от внешней репы (`std/src/http/transport/real.nv` → `import tls`) —
стандартная библиотека не самодостаточна. Голый http без tls практически не используется;
связка http+tls неразрывна. Ни одна экосистема не держит «http в std, tls снаружи»:
Go/Python/Node — оба внутри (ценой бандла криптографии), Rust/Swift — оба снаружи.
Выбрана школа Rust/Swift, в продолжение плана 193 (вынос tls).

**Целевая картина:** публичная репа-сиблинг `nv-lang/nova-http` (пакет `http`, эталон —
`../nova-tls` после 202-Ф.3), зависит от `nova-tls` через `[dependencies]`. В std остаётся
самодостаточный `net` (TCP/UDP-сокеты). Module-path потребителей НЕ меняется:
`import http.{...}` / `import http.client.{...}` работают как раньше (пакет = `http`).

## Фазы

### Ф.1 — перенос
- `git init d:/Sources/nv-lang/nova-http`; структура: `nova.toml` (`[lib] src="src"`,
  `[dependencies] tls = { path = "../nova-tls" }` — форму зависимости взять из рабочего
  cross-package прецедента 202-Ф.2/Ф.3; **path-dep = dev-форма, временно до Plan 204**
  (git+semver+nova.lock, Q-dependency-versioning) — при закрытии 204 заменить на
  `{ git = "https://github.com/nv-lang/nova-tls", version = "..." }`), `src/*.nv` (module `http`, root peers D78 rev-4),
  подпапки `src/client/` и т.д. — как в текущем `std/src/http/**` (папка = подмодуль,
  `http.client`, `http.transport`, ...).
- Лицензии MIT+Apache и README — по образцу nova-tls.
- `*_test.nv` едут вместе с модулями (конвенция «тесты рядом»).

### Ф.2 — потребители
- Инвентарь: `grep -rn "import http" std/ examples/ spec_tests/ nova_tests/` + флагман
  (`examples/flagship/aggregator`). Потребители получают зависимость `http` через
  `[dependencies]` своих манифестов (или workspace-механику — по прецеденту tls).
- `std/**` больше НЕ импортирует http (проверить обратные зависимости: если что-то в std
  использует http — это кандидат на переезд вместе с ним или на разрыв; доложить списком
  ДО переноса).

### Ф.3 — вычистка std
- Удалить `std/src/http/**`, поправить `std/nova.toml`/workspace-members.
- Гейты: conformance полный; `nova check std` (дельта = только исчезнувшие http-строки);
  `nova test src` в nova-http (все http-тесты живы на новом месте); сборка флагмана;
  cross-package smoke (`import http.{...}` из независимого пакета).

### Ф.4 — публикация
- `gh repo create nv-lang/nova-http --public` + push (только после зелёных гейтов Ф.3).
- Доки: read-project.md (карта реп-сиблингов), 07-modules примеры, если упоминают std/http.

## Границы
- Язык НЕ меняется (D-амендмент не нужен); D78 rev-4 уже покрывает структуру.
- `std/src/net` НЕ трогается (остаётся в std).
- Vendored-сборка mbedTLS (хвост 193) — вне объёма.
