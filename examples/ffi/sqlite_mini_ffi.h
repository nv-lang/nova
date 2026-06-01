/* SPDX-License-Identifier: MIT OR Apache-2.0
 *
 * sqlite_mini_ffi.h — Plan 115 D214 Ф.3 / A7 end-to-end FFI sample.
 *
 * Mini-sqlite-equivalent C-library, embedded as header-only inline implementation.
 * Provides sqlite-like API (open/exec/prepare/step/column/finalize/close) backed
 * by simple in-memory key-value store + sequence iterator.
 *
 * Зачем embedded mini-equivalent, не real libsqlite3:
 *   - Plan 115 V1 ships `nova build --c-shim` followup `[M-115-ffi-build-pipeline]` —
 *     user-provided shim linking infrastructure ещё не доступна.
 *   - A7 acceptance: «end-to-end FFI sample compiles + runs». Mini-equivalent
 *     proves FFI mechanism (typed handles, tuple-by-value returns, consume close
 *     semantics) end-to-end без external dependency.
 *   - Real libsqlite3 integration — followup `[M-115-examples-ffi-real-build]`:
 *     отдельный CI step с vcpkg sqlite3 install + replace `sqlite_mini_ffi.h` →
 *     `sqlite3_ffi.h` per docs/ffi-cookbook.md §«Example 1».
 *
 * API semantics (sqlite-compatible subset):
 *   - mini_sqlite_open(path) → (db_handle, rc). rc=0 on success.
 *   - mini_sqlite_exec(db, sql) → rc. Parses tiny SQL subset:
 *     "INSERT KEY=VAL" → store["KEY"] = VAL; "DELETE KEY" → store.remove(key).
 *   - mini_sqlite_prepare(db, sql "SELECT") → (stmt_handle, rc). SELECT only.
 *   - mini_sqlite_step(stmt) → rc (SQLITE_ROW=100 if next row, SQLITE_DONE=101).
 *   - mini_sqlite_column_int(stmt, col=0) → first int field of current row.
 *   - mini_sqlite_finalize(stmt) → 0.
 *   - mini_sqlite_close(db) → 0.
 *
 * Backing storage: static array per opened DB (max 64 KV pairs, key max 32 bytes).
 * No thread-safety (single-threaded test target).
 */

#ifndef NOVA_SQLITE_MINI_FFI_H
#define NOVA_SQLITE_MINI_FFI_H

#include <stdint.h>
#include <string.h>
#include <stdlib.h>
/* Plan 115 D214 [M-115-ffi-build-pipeline]: user shim header self-includes
 * nova_rt для access к nova_int / nova_ptr / nova_str typedefs. Path
 * относительный к cg_include (passed via clang -I). */
#include "nova_rt/nova_rt.h"

/* SQLite-compatible return codes (subset). */
#define MINI_SQLITE_OK   0
#define MINI_SQLITE_ROW  100
#define MINI_SQLITE_DONE 101
#define MINI_SQLITE_ERR  1

/* Backing storage. */
#define MINI_SQLITE_MAX_DBS 8
#define MINI_SQLITE_MAX_KV  64
#define MINI_SQLITE_KEY_MAX 32

typedef struct {
    char   key[MINI_SQLITE_KEY_MAX];
    int64_t value;
    int     used;  /* 0 = empty slot, 1 = active, 2 = tombstone */
} mini_sqlite_row_t;

typedef struct {
    int                opened;
    mini_sqlite_row_t  rows[MINI_SQLITE_MAX_KV];
} mini_sqlite_db_t;

typedef struct {
    mini_sqlite_db_t* db;
    int               cursor;     /* next row index to read */
    int               last_value; /* value at current cursor (for column_int) */
} mini_sqlite_stmt_t;

/* Static pools — заранее аллокированные. Возвращаем ptr внутрь pools[]. */
static mini_sqlite_db_t   mini_sqlite_db_pool[MINI_SQLITE_MAX_DBS];
static mini_sqlite_stmt_t mini_sqlite_stmt_pool[MINI_SQLITE_MAX_DBS * 4];

static inline mini_sqlite_db_t* mini_sqlite_alloc_db(void) {
    for (int i = 0; i < MINI_SQLITE_MAX_DBS; i++) {
        if (!mini_sqlite_db_pool[i].opened) {
            mini_sqlite_db_t* db = &mini_sqlite_db_pool[i];
            db->opened = 1;
            for (int j = 0; j < MINI_SQLITE_MAX_KV; j++) {
                db->rows[j].used = 0;
                db->rows[j].value = 0;
            }
            return db;
        }
    }
    return NULL;
}

static inline mini_sqlite_stmt_t* mini_sqlite_alloc_stmt(mini_sqlite_db_t* db) {
    for (size_t i = 0; i < sizeof(mini_sqlite_stmt_pool) / sizeof(mini_sqlite_stmt_pool[0]); i++) {
        if (mini_sqlite_stmt_pool[i].db == NULL) {
            mini_sqlite_stmt_t* s = &mini_sqlite_stmt_pool[i];
            s->db = db;
            s->cursor = 0;
            s->last_value = 0;
            return s;
        }
    }
    return NULL;
}

/* Tiny SQL parser. Handles только:
 *   "INSERT KEY=VAL"   → store[KEY] = VAL
 *   "DELETE KEY"       → store[KEY] tombstoned
 *   "SELECT"           → cursor iterates rows
 * Plus path arg ignored (in-memory, no file).
 */
static inline int mini_sqlite_parse_and_exec(mini_sqlite_db_t* db, nova_str sql) {
    if (!db || sql.len == 0) return MINI_SQLITE_ERR;
    char buf[256];
    if (sql.len >= sizeof(buf)) return MINI_SQLITE_ERR;
    memcpy(buf, sql.ptr, sql.len);
    buf[sql.len] = '\0';

    if (strncmp(buf, "INSERT ", 7) == 0) {
        char* rest = buf + 7;
        char* eq = strchr(rest, '=');
        if (!eq) return MINI_SQLITE_ERR;
        *eq = '\0';
        const char* key_str = rest;
        int64_t val = (int64_t)atoll(eq + 1);
        for (int i = 0; i < MINI_SQLITE_MAX_KV; i++) {
            if (db->rows[i].used == 0) {
                strncpy(db->rows[i].key, key_str, MINI_SQLITE_KEY_MAX - 1);
                db->rows[i].key[MINI_SQLITE_KEY_MAX - 1] = '\0';
                db->rows[i].value = val;
                db->rows[i].used = 1;
                return MINI_SQLITE_OK;
            }
        }
        return MINI_SQLITE_ERR;
    }
    if (strncmp(buf, "DELETE ", 7) == 0) {
        const char* key_str = buf + 7;
        for (int i = 0; i < MINI_SQLITE_MAX_KV; i++) {
            if (db->rows[i].used == 1 && strncmp(db->rows[i].key, key_str, MINI_SQLITE_KEY_MAX) == 0) {
                db->rows[i].used = 2;  /* tombstone */
                return MINI_SQLITE_OK;
            }
        }
        return MINI_SQLITE_OK;  /* no-op if not found */
    }
    if (strncmp(buf, "CREATE ", 7) == 0) {
        return MINI_SQLITE_OK;  /* no-op DDL */
    }
    return MINI_SQLITE_ERR;
}

/* ─── Plan 115 Ф.3 FFI surface — nova_fn_<name> convention. ─── */

/* Forward-declare mono'd tuple typedefs (matching Nova codegen). */
#ifndef NOVA_TUPLE_TYPEDEF__NovaTuple_2_8_nova_ptr_8_nova_int
#define NOVA_TUPLE_TYPEDEF__NovaTuple_2_8_nova_ptr_8_nova_int
typedef struct _NovaTuple_2_8_nova_ptr_8_nova_int {
    nova_ptr f0;
    nova_int f1;
} _NovaTuple_2_8_nova_ptr_8_nova_int;
#endif

/* Open: returns (db_handle, rc). Path argument ignored (in-memory). */
static inline _NovaTuple_2_8_nova_ptr_8_nova_int
nova_fn_mini_sqlite_open(nova_str path) {
    (void)path;  /* in-memory, path ignored */
    _NovaTuple_2_8_nova_ptr_8_nova_int r;
    mini_sqlite_db_t* db = mini_sqlite_alloc_db();
    r.f0 = (nova_ptr)db;
    r.f1 = db ? MINI_SQLITE_OK : MINI_SQLITE_ERR;
    return r;
}

/* Execute SQL (INSERT/DELETE/CREATE). Returns rc. */
static inline nova_int nova_fn_mini_sqlite_exec(nova_ptr db, nova_str sql) {
    return (nova_int)mini_sqlite_parse_and_exec((mini_sqlite_db_t*)db, sql);
}

/* Prepare SELECT statement. Returns (stmt_handle, rc). */
static inline _NovaTuple_2_8_nova_ptr_8_nova_int
nova_fn_mini_sqlite_prepare(nova_ptr db, nova_str sql) {
    _NovaTuple_2_8_nova_ptr_8_nova_int r;
    (void)sql;  /* parser stub — only SELECT supported */
    mini_sqlite_stmt_t* stmt = mini_sqlite_alloc_stmt((mini_sqlite_db_t*)db);
    r.f0 = (nova_ptr)stmt;
    r.f1 = stmt ? MINI_SQLITE_OK : MINI_SQLITE_ERR;
    return r;
}

/* Step: advance cursor. Returns SQLITE_ROW if more rows, SQLITE_DONE otherwise. */
static inline nova_int nova_fn_mini_sqlite_step(nova_ptr stmt) {
    mini_sqlite_stmt_t* s = (mini_sqlite_stmt_t*)stmt;
    if (!s) return MINI_SQLITE_ERR;
    while (s->cursor < MINI_SQLITE_MAX_KV) {
        if (s->db->rows[s->cursor].used == 1) {
            s->last_value = (int)s->db->rows[s->cursor].value;
            s->cursor++;
            return MINI_SQLITE_ROW;
        }
        s->cursor++;
    }
    return MINI_SQLITE_DONE;
}

/* Get column int value of current row. */
static inline nova_int nova_fn_mini_sqlite_column_int(nova_ptr stmt, nova_int col) {
    (void)col;  /* mini-sqlite only has 1 column (value) */
    mini_sqlite_stmt_t* s = (mini_sqlite_stmt_t*)stmt;
    return s ? (nova_int)s->last_value : (nova_int)0;
}

/* Finalize statement. */
static inline nova_int nova_fn_mini_sqlite_finalize(nova_ptr stmt) {
    mini_sqlite_stmt_t* s = (mini_sqlite_stmt_t*)stmt;
    if (s) { s->db = NULL; }
    return MINI_SQLITE_OK;
}

/* Close DB. */
static inline nova_int nova_fn_mini_sqlite_close(nova_ptr db) {
    mini_sqlite_db_t* d = (mini_sqlite_db_t*)db;
    if (d) { d->opened = 0; }
    return MINI_SQLITE_OK;
}

#endif /* NOVA_SQLITE_MINI_FFI_H */
