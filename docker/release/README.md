<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Nova release Docker image (v0.1.0)

Готовый Linux-образ с компилятором Nova (`nova`), стандартной библиотекой
(`std/`) и C-рантаймом (`nova_rt/`, включая libuv) — для сборки Nova-программ
без установки Rust/локального клона монорепозитория.

Отличается от старого `docker/` (Plan 40, 2026-05-12): тот каталог валидирует
M:N-рантайм под sanitizer'ами (TSan/ASan/UBSan) для разработки самого
компилятора; этот — тонкий образ для КОНЕЧНОГО пользователя языка,
собранный по верифицированному рецепту `docs/guide/linux-build.md` /
`.github/workflows/nova-gate.yml` (Ubuntu 22.04, clang + cmake + make +
libgc-dev + build-essential, rustup 1.85 только в build-стадии).

## Сборка образа

Собирается из корня репозитория — **build context обязан быть корнем**
(`Dockerfile` копирует `nova-cli/`, `compiler-codegen/`, `std/` как
top-level каталоги контекста, не через `git clone`):

```sh
git submodule update --init compiler-codegen/nova_rt/libuv   # если ещё не инициализирован
docker build -f docker/release/Dockerfile -t nova:0.1.0 .
```

Two-stage build:
1. **builder** — Ubuntu 22.04 + rustup (pinned 1.85, MSRV) + системные пакеты
   (`clang cmake make libgc-dev build-essential`) — `cargo build --release
   --manifest-path nova-cli/Cargo.toml`.
2. **runtime** — свежий Ubuntu 22.04 + те же системные пакеты (нужны, чтобы
   `nova` сама компилировала сгенерированный C и линковала Boehm GC/libuv) +
   собранный бинарь `nova` + `std/` + `nova_rt/` (полный, включая libuv
   submodule — Linux-сборка libuv компилируется из исходников при первом
   использовании, `nova` кеширует `libuv.a`/`libnova_rt.a` в `target/`
   внутри рабочей директории проекта).

Локально собранный образ (тег `nova:0.1.0-test`): **~1.01 GB**.

## Использование

Смонтируй рабочую директорию с Nova-проектом (`nova.toml` + исходники) в
`/work` (рабочая директория контейнера по умолчанию — `WORKDIR /work`):

```sh
docker run --rm -v "$PWD":/work nova:0.1.0 build hello.nv -o hello
docker run --rm -v "$PWD":/work --entrypoint sh nova:0.1.0 -c "./hello"
```

Entrypoint — `nova` (`ENTRYPOINT ["nova"]`, `CMD ["--version"]` — без
аргументов контейнер печатает версию). Т.е. любая подкоманда идёт сразу
после образа:

```sh
docker run --rm nova:0.1.0                       # nova --version
docker run --rm -v "$PWD":/work nova:0.1.0 test std/src/checksums
```

Минимальный проект (`nova.toml` + `hello.nv` в текущей директории хоста):

```toml
# nova.toml
[package]
name = "hello"
version = "0.1.0"
```

```nova
// hello.nv
module hello

fn main() {
    println("hello from nova docker")
}
```

```sh
docker run --rm -v "$PWD":/work nova:0.1.0 build hello.nv -o hello
docker run --rm -v "$PWD":/work --entrypoint ./hello nova:0.1.0
```

## Уже выставленные env vars (внутри образа)

`nova` ищет `std/`/C-рантайм НЕ относительно себя, а через эти 5
переменных окружения (та же поверхность, что `scripts/package-release.ps1`
генерирует в `setup-env.ps1` для Windows-дистрибутива — здесь unix-пути
уже прописаны в образе, ничего дополнительно настраивать не нужно):

| Переменная | Значение в образе | Назначение |
|---|---|---|
| `NOVA_STD_PATH` | `/opt/nova/std` | исходники стандартной библиотеки |
| `NOVA_CG_INCLUDE` | `/opt/nova` | родитель `nova_rt/` |
| `NOVA_RT_DIR` | `/opt/nova/nova_rt` | C-рантайм (`eventloop.c`, `libuv/`) |
| `NOVA_GC_LIB_DIR` | `/usr/lib/x86_64-linux-gnu` | Boehm GC `.so` (системный `libgc-dev`) |
| `NOVA_GC_INCLUDE_DIR` | `/usr/include` | Boehm GC заголовки (`gc.h`) |

## Smoke test (верифицировано)

```sh
docker run --rm --entrypoint sh -v "$PWD":/work nova:0.1.0-test \
    -c "nova --version && nova build hello.nv -o hello && ./hello"
```

Вывод:

```
nova 0.1.0
built: hello (10.82s)
hello from nova docker
```

(Первый прогон на новом volume дополнительно печатает разовую сборку
`libuv.a`/`libnova_rt.a` — кешируется в `target/` внутри смонтированной
директории проекта, последующие сборки быстрее.)

## Известные ограничения

- Унаследовано от `docker/README.md` (Plan 40, 2026-05-12): Boehm
  `GC_init` может падать на perf-бенчах под restricted Docker permissions
  (`GC_find_limit_with_bound`) — не относится к обычной компиляции/рантайму
  программ, этого образа не касается.
- `examples/` (в частности `examples/flagship/aggregator`) требует
  path-зависимость `../../nova-http` (сиблинг `nova-http` рядом с чекаутом
  `nova`) — для smoke/hello-world образ этого не требует; для сборки
  примеров из `examples/` внутри контейнера примонтируй оба репозитория и
  сохрани их относительное расположение.
- Образ не включает `nova-lsp` (языковой сервер) — только `nova` CLI.
