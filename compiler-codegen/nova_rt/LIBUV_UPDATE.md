# libuv submodule update procedure

libuv подключён через **git submodule**, pinned на конкретный release tag
для reproducible builds.

## Текущая версия

См. `git submodule status compiler-codegen/nova_rt/libuv` — выводит
точный commit (привязан к release tag, не к ветке).

## Как обновить libuv

```powershell
# 1. Войти в submodule
cd compiler-codegen/nova_rt/libuv

# 2. Получить новые tag'и
git fetch --tags

# 3. Посмотреть доступные релизы
git tag | Select-String "^v1\." | Select-Object -Last 10

# 4. Переключиться на новый release (пример: v1.53.0)
git checkout v1.53.0

# 5. Вернуться в parent repo и зарегистрировать новый commit
cd ../../..
git add compiler-codegen/nova_rt/libuv
git commit -m "deps: bump libuv to v1.53.0"
```

## Как клонировать репо с submodule'ом

```powershell
# Свежий clone:
git clone --recursive https://github.com/<user>/nova-lang.git

# Если репо уже clone'нут без --recursive:
git submodule update --init --recursive
```

## Политика обновления

- **Только release tag'и** (`v1.X.Y`), никогда не master/main libuv.
- **Проверка breaking changes** через libuv CHANGELOG перед bump'ом.
- **CI прогон** на новой версии до merge'а bump-коммита.
- libuv source **никогда не патчится** — обёртки только в `nova_rt/`
  (политика [feedback_third_party_libs](../../memory/feedback_third_party_libs.md)).
  Если требуется fix в libuv — upstream patch, не in-tree.

## Список используемых API

Plan 22 использует:

- `uv_loop_t`, `uv_default_loop`, `uv_loop_close`, `uv_walk`,
  `uv_loop_alive`, `uv_run` (`UV_RUN_NOWAIT`, `UV_RUN_ONCE`,
  `UV_RUN_DEFAULT`)
- `uv_timer_t`, `uv_timer_init`, `uv_timer_start`, `uv_timer_stop`
- `uv_handle_t`, `uv_close`, `uv_is_closing`
- `uv_strerror`
- `uv_version_string`

Future Plan'ы добавят:
- `uv_tcp_t`, `uv_read_start`/`uv_read_stop` (Plan 23+ std.net)
- `uv_fs_t`, `uv_cancel` (Plan 23+ std.fs)
- `uv_signal_t` (Plan 22 Ф.5 future SIGINT support)
- `uv_async_t` (Plan 23 M:N cross-worker wake)

## Что не используется (избегать)

- `uv_thread_t` API в bootstrap (single-thread N:1).
- `uv_mutex_t` / `uv_rwlock_t` / `uv_cond_t` — пока нет shared
  multi-thread state.
- libuv allocator (`uv_replace_allocator`) — Nova имеет свой GC.
