#!/bin/sh
# scripts/guards/lib/family.sh — где лежат соседние репозитории и рабочие деревья.
# Подключение: . "$ROOT/scripts/guards/lib/family.sh"
#
# nova_family_root ROOT — каталог основных репозиториев (nova, nova-http,
#   nova-tls, ...): родитель ГЛАВНОЙ рабочей копии, то есть первой записи
#   `git worktree list`, а НЕ родитель ROOT. С 2026-10-05 рабочие деревья живут
#   в `<каталог>/worktrees/<имя>` (задача владельца, страж
#   check-worktree-location), и «родитель ROOT», запущенный из дерева, указывает
#   на папку деревьев, где соседей нет: пакеты числились бы отсутствующими
#   молча. Не git-репозиторий — родитель ROOT, как было.
#
# nova_worktree_dir ROOT — папка рабочих деревьев: `NOVA_WORKTREE_DIR`, иначе
#   `<nova_family_root>/worktrees`. Единственный дом этого правила; страж
#   check-worktree-location судит по нему же.

nova_family_root() {
    _fr_main=$(git -C "$1" worktree list --porcelain 2>/dev/null | sed -n 's|^worktree ||p' | head -1)
    if [ -n "$_fr_main" ]; then
        printf '%s' "${_fr_main%/*}"
    else
        (cd "$1/.." 2>/dev/null && pwd)
    fi
}

nova_worktree_dir() {
    if [ -n "${NOVA_WORKTREE_DIR:-}" ]; then
        printf '%s' "$NOVA_WORKTREE_DIR"
    else
        printf '%s/worktrees' "$(nova_family_root "$1")"
    fi
}
