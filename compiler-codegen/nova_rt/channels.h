#ifndef NOVA_RT_CHANNELS_H
#define NOVA_RT_CHANNELS_H

/* ---- D79 Channels — coordination между fiber'ами ----
 *
 * Bootstrap (D71) — single-threaded cooperative. Send на полный буфер
 * yield'ит, recv на пустой yield'ит. Closed-семантика по spec D79:
 *   - send на closed → panic
 *   - recv после close: drain buffer, потом None
 *
 * Хранение: bounded ring-buffer (capacity > 0) или unbuffered
 * rendezvous (capacity == 0). В bootstrap'е unbuffered = capacity 1
 * с дополнительным флагом — упрощение, не строго rendezvous, но
 * семантически достаточно для single-threaded cooperative scheduler'а.
 *
 * Тип элемента — `nova_int` (универсальный 64-bit slot). Strong-typed
 * Channel[T] для не-int типов хранит boxed-pointers (как и массивы);
 * codegen генерирует cast'ы.
 */

#include "alloc.h"
#include "fibers.h"
#include <stdint.h>
#include <stdbool.h>

typedef struct Nova_Channel {
    nova_int* buf;       /* ring-buffer storage */
    int64_t   cap;       /* buffer capacity (0 → unbuffered semantics) */
    int64_t   head;      /* next slot to read */
    int64_t   count;     /* current number of items */
    bool      closed;
} Nova_Channel;

static inline Nova_Channel* nova_channel_new(int64_t capacity) {
    Nova_Channel* ch = (Nova_Channel*)nova_alloc(sizeof(Nova_Channel));
    /* Internally use cap=1 if user requested unbuffered (rendezvous);
     * cooperative scheduler makes this semantically equivalent for
     * D71 single-threaded mode. */
    int64_t actual = capacity > 0 ? capacity : 1;
    ch->buf    = (nova_int*)nova_alloc((size_t)actual * sizeof(nova_int));
    ch->cap    = actual;
    ch->head   = 0;
    ch->count  = 0;
    ch->closed = false;
    return ch;
}

/* `send` — block (yield) пока буфер полон. На closed channel — panic
 * (D79 + D13). */
static inline void nova_channel_send(Nova_Channel* ch, nova_int v) {
    while (true) {
        if (ch->closed) {
            nova_throw(nova_str_from_cstr("send on closed channel"));
        }
        if (ch->count < ch->cap) {
            int64_t tail = (ch->head + ch->count) % ch->cap;
            ch->buf[tail] = v;
            ch->count++;
            return;
        }
        nova_fiber_yield();
    }
}

/* `try_send` — non-blocking. Returns true если послали, false если
 * буфер полон или channel закрыт. */
static inline nova_bool nova_channel_try_send(Nova_Channel* ch, nova_int v) {
    if (ch->closed) return 0;
    if (ch->count >= ch->cap) return 0;
    int64_t tail = (ch->head + ch->count) % ch->cap;
    ch->buf[tail] = v;
    ch->count++;
    return 1;
}

/* `recv` — блокирует пока есть значение или channel закрыт. Returns
 * Option[T]: Some(v) если получили, None если closed и буфер пуст
 * (D79 drain semantics). */
static inline NovaOpt_nova_int nova_channel_recv(Nova_Channel* ch) {
    NovaOpt_nova_int r;
    while (true) {
        if (ch->count > 0) {
            r.tag = NOVA_TAG_Option_Some;
            r.value = ch->buf[ch->head];
            ch->head = (ch->head + 1) % ch->cap;
            ch->count--;
            return r;
        }
        if (ch->closed) {
            r.tag = NOVA_TAG_Option_None;
            r.value = 0;
            return r;
        }
        nova_fiber_yield();
    }
}

/* `try_recv` — non-blocking. Returns Some(v) если есть, None если пусто
 * (вне closed-семантики — same return для empty и closed-empty). */
static inline NovaOpt_nova_int nova_channel_try_recv(Nova_Channel* ch) {
    NovaOpt_nova_int r;
    if (ch->count > 0) {
        r.tag = NOVA_TAG_Option_Some;
        r.value = ch->buf[ch->head];
        ch->head = (ch->head + 1) % ch->cap;
        ch->count--;
        return r;
    }
    r.tag = NOVA_TAG_Option_None;
    r.value = 0;
    return r;
}

/* `close` — idempotent. После close: send → panic, recv → drain → None. */
static inline void nova_channel_close(Nova_Channel* ch) {
    ch->closed = true;
}

static inline nova_bool nova_channel_is_closed(Nova_Channel* ch) {
    return ch->closed;
}

static inline nova_int nova_channel_len(Nova_Channel* ch) {
    return (nova_int)ch->count;
}

static inline nova_int nova_channel_capacity(Nova_Channel* ch) {
    return (nova_int)ch->cap;
}

#endif /* NOVA_RT_CHANNELS_H */
