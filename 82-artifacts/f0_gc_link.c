/* SPDX-License-Identifier: MIT OR Apache-2.0
 *
 * f0_gc_link.c — Plan 82 Ф.0 §1.6: подтвердить, что Boehm GC API, на
 * которых строится GC-дизайн §5.2 (registry + precise push), реально
 * линкуются из статической `gc.lib` (vendored vcpkg, x64-windows-static).
 *
 * Заголовки уже проверены (Ф.0): gc_mark.h:300/311, gc.h:1472/1686.
 * Здесь — линковочный тест: берём адрес каждой функции (форсирует
 * резолюцию символа линкером) без исполнения опасных вызовов.
 */
#define GC_NOT_DLL          /* статическая линковка — GC_API без dllimport */
#define GC_THREADS
#include <gc/gc.h>
#include <gc/gc_mark.h>
#include <stdio.h>

int main(void) {
    /* Адрес каждой §1.6/§5.2-функции → линкер обязан резолвить символ. */
    void* sink[5];
    sink[0] = (void*)(GC_word)&GC_call_with_alloc_lock;  /* §5.2(1) alloc-lock */
    sink[1] = (void*)(GC_word)&GC_set_push_other_roots;  /* §5.2(2) push-колбэк */
    sink[2] = (void*)(GC_word)&GC_push_all;              /* §5.2 precise push  */
    sink[3] = (void*)(GC_word)&GC_set_stackbottom;       /* §1.6 (Linux-путь)  */
    sink[4] = (void*)(GC_word)&GC_get_stack_base;        /* §1.6 worker reg.   */
    printf("Plan 82 Ф.0 §1.6: Boehm GC API linked OK from gc.lib\n");
    printf("  GC_call_with_alloc_lock  = %p\n", sink[0]);
    printf("  GC_set_push_other_roots  = %p\n", sink[1]);
    printf("  GC_push_all              = %p\n", sink[2]);
    printf("  GC_set_stackbottom       = %p\n", sink[3]);
    printf("  GC_get_stack_base        = %p\n", sink[4]);
    return 0;
}
