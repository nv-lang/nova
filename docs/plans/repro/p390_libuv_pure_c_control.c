/* №390: чистый тест на libuv БЕЗ Nova.
 *
 * Вопрос: приходит ли UV_EOF на ВТОРОЕ чтение, если пир прислал байт и закрылся,
 * а мы между чтениями делаем uv_read_stop + uv_read_start (узор нашего net.c)?
 *
 * Всё в ОДНОМ потоке и ОДНОМ цикле — так libuv и задумана. Значит если здесь
 * зависает, виновата libuv/Windows; если нет — виноват наш шим.
 *
 * Режимы (argv[1]):
 *   stopstart  — наш узор: после каждого куска uv_read_stop, потом снова start
 *   continuous — узор Node: uv_read_start один раз, никогда не stop
 */
#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include "uv.h"

static uv_loop_t*  loop;
static uv_tcp_t    server, client, incoming;
static uv_connect_t connect_req;
static uv_write_t  write_req;
static uv_timer_t  bail_timer;
static uv_idle_t   restart_idle;
static int   mode_deferred  = 0;
static void  restart_cb(uv_idle_t* h);

static char  scratch[64];
static int   mode_stopstart = 1;   /* 1 = наш узор, 0 = непрерывный */
static int   reads_done     = 0;   /* сколько раз получили данные */
static int   got_eof        = 0;
static int   server_closed  = 0;

static void alloc_cb(uv_handle_t* h, size_t suggested, uv_buf_t* buf) {
    (void)h; (void)suggested;
    buf->base = scratch;
    buf->len  = 1;                 /* как у нас: читаем ровно 1 байт */
}

static void on_incoming_closed(uv_handle_t* h) {
    (void)h;
    server_closed = 1;
    printf("[srv] соединение закрыто (FIN отправлен)\n"); fflush(stdout);
}

static void on_write(uv_write_t* req, int status) {
    (void)req;
    printf("[srv] записал 1 байт, статус=%d -> закрываю\n", status); fflush(stdout);
    uv_close((uv_handle_t*)&incoming, on_incoming_closed);
}

static void read_cb(uv_stream_t* s, ssize_t nread, const uv_buf_t* buf) {
    (void)buf;
    printf("[cli] read_cb: nread=%lld (UV_EOF=%d) %s\n",
           (long long)nread, (int)UV_EOF,
           nread == UV_EOF ? "<<< ЭТО EOF" : "");
    fflush(stdout);

    if (nread == UV_EOF) { got_eof = 1; uv_read_stop(s); uv_stop(loop); return; }
    if (nread < 0)       { printf("[cli] ошибка %s\n", uv_strerror((int)nread));
                           uv_read_stop(s); uv_stop(loop); return; }
    if (nread == 0)      { printf("[cli]   (пустое срабатывание, ждём дальше)\n");
                           fflush(stdout); return; }

    reads_done++;
    printf("[cli] получено %lld байт (чтение №%d)\n", (long long)nread, reads_done);
    fflush(stdout);

    if (mode_deferred) {
        /* ТОЧНАЯ модель Nova: стоп внутри колбэка, а перезапуск — ПОЗЖЕ,
         * вне контекста колбэка (у нас между ними файбер паркуется/просыпается).
         * Идл-хендл даёт ровно это: следующая итерация цикла. */
        uv_read_stop(s);
        printf("[cli] uv_read_stop; перезапуск ОТЛОЖЕН на следующую итерацию "
               "(модель парковки файбера)\n"); fflush(stdout);
        uv_idle_start(&restart_idle, restart_cb);
        return;
    }
    if (mode_stopstart) {
        uv_read_stop(s);
        printf("[cli] uv_read_stop -> и сразу uv_read_start (наш узор)\n"); fflush(stdout);
        int rc = uv_read_start(s, alloc_cb, read_cb);
        printf("[cli] uv_read_start rc=%d (%s)\n", rc, rc ? uv_strerror(rc) : "ok");
        fflush(stdout);
    }
}

/* Перезапуск чтения ВНЕ контекста read_cb — модель «файбер проснулся». */
static void restart_cb(uv_idle_t* h) {
    uv_idle_stop(h);
    int rc = uv_read_start((uv_stream_t*)&client, alloc_cb, read_cb);
    printf("[cli] ОТЛОЖЕННЫЙ uv_read_start rc=%d (%s)\n",
           rc, rc ? uv_strerror(rc) : "ok");
    fflush(stdout);
}

static void on_connect(uv_connect_t* req, int status) {
    (void)req;
    printf("[cli] connect статус=%d -> uv_read_start\n", status); fflush(stdout);
    int rc = uv_read_start((uv_stream_t*)&client, alloc_cb, read_cb);
    printf("[cli] uv_read_start rc=%d (%s)\n", rc, rc ? uv_strerror(rc) : "ok");
    fflush(stdout);
}

static void on_connection(uv_stream_t* s, int status) {
    if (status < 0) { printf("[srv] ошибка accept: %s\n", uv_strerror(status)); return; }
    uv_tcp_init(loop, &incoming);
    if (uv_accept(s, (uv_stream_t*)&incoming) == 0) {
        printf("[srv] принял соединение\n"); fflush(stdout);
        static char byte = 0x05;
        uv_buf_t b = uv_buf_init(&byte, 1);
        uv_write(&write_req, (uv_stream_t*)&incoming, &b, 1, on_write);
    }
    uv_close((uv_handle_t*)&server, NULL);
}

static void on_bail(uv_timer_t* t) {
    (void)t;
    printf("\n!!! ТАЙМАУТ 5с: EOF так и НЕ пришёл (чтений с данными: %d, "
           "сервер закрылся: %d)\n", reads_done, server_closed);
    fflush(stdout);
    uv_stop(loop);
}

int main(int argc, char** argv) {
    if (argc > 1 && strcmp(argv[1], "continuous") == 0) mode_stopstart = 0;
    if (argc > 1 && strcmp(argv[1], "deferred") == 0)   mode_deferred  = 1;
    printf("=== режим: %s ===\n",
           mode_deferred  ? "deferred (ТОЧНАЯ модель Nova: стоп в колбэке, старт позже)"
         : mode_stopstart ? "stopstart (стоп и старт внутри колбэка)"
                          : "continuous (узор Node)");
    fflush(stdout);

    loop = uv_default_loop();
    uv_idle_init(loop, &restart_idle);

    struct sockaddr_in addr;
    uv_ip4_addr("127.0.0.1", 0, &addr);
    uv_tcp_init(loop, &server);
    uv_tcp_bind(&server, (const struct sockaddr*)&addr, 0);
    uv_listen((uv_stream_t*)&server, 1, on_connection);

    struct sockaddr_storage bound;
    int len = sizeof(bound);
    uv_tcp_getsockname(&server, (struct sockaddr*)&bound, &len);
    int port = ntohs(((struct sockaddr_in*)&bound)->sin_port);
    printf("[srv] слушаю порт %d\n", port); fflush(stdout);

    struct sockaddr_in caddr;
    uv_ip4_addr("127.0.0.1", port, &caddr);
    uv_tcp_init(loop, &client);
    uv_tcp_connect(&connect_req, &client, (const struct sockaddr*)&caddr, on_connect);

    uv_timer_init(loop, &bail_timer);
    uv_timer_start(&bail_timer, on_bail, 5000, 0);

    uv_run(loop, UV_RUN_DEFAULT);

    printf("\n=== ИТОГ: чтений с данными=%d, EOF получен=%s ===\n",
           reads_done, got_eof ? "ДА" : "НЕТ");
    return got_eof ? 0 : 1;
}
