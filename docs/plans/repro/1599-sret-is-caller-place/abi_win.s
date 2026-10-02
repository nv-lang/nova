	.def	@feat.00;
	.scl	3;
	.type	0;
	.endef
	.globl	@feat.00
@feat.00 = 0
	.file	"abi.c"
	.def	make_big;
	.scl	2;
	.type	32;
	.endef
	.globl	__xmm@00000000000000010000000000000000 # -- Begin function make_big
	.section	.rdata,"dr",discard,__xmm@00000000000000010000000000000000
	.p2align	4, 0x0
__xmm@00000000000000010000000000000000:
	.byte	0                               # 0x0
	.byte	0                               # 0x0
	.byte	0                               # 0x0
	.byte	0                               # 0x0
	.byte	0                               # 0x0
	.byte	0                               # 0x0
	.byte	0                               # 0x0
	.byte	0                               # 0x0
	.byte	1                               # 0x1
	.byte	0                               # 0x0
	.byte	0                               # 0x0
	.byte	0                               # 0x0
	.byte	0                               # 0x0
	.byte	0                               # 0x0
	.byte	0                               # 0x0
	.byte	0                               # 0x0
	.globl	__xmm@00000000000000030000000000000002
	.section	.rdata,"dr",discard,__xmm@00000000000000030000000000000002
	.p2align	4, 0x0
__xmm@00000000000000030000000000000002:
	.quad	2                               # 0x2
	.quad	3                               # 0x3
	.text
	.globl	make_big
	.p2align	4
make_big:                               # @make_big
# %bb.0:
	movq	%rdx, %xmm0
	pshufd	$68, %xmm0, %xmm0               # xmm0 = xmm0[0,1,0,1]
	movdqa	__xmm@00000000000000010000000000000000(%rip), %xmm1 # xmm1 = [0,0,0,0,0,0,0,0,1,0,0,0,0,0,0,0]
	paddq	%xmm0, %xmm1
	paddq	__xmm@00000000000000030000000000000002(%rip), %xmm0
	movq	%rcx, %rax
	movdqu	%xmm0, 16(%rcx)
	movdqu	%xmm1, (%rcx)
	retq
                                        # -- End function
	.def	make_mid;
	.scl	2;
	.type	32;
	.endef
	.globl	make_mid                        # -- Begin function make_mid
	.p2align	4
make_mid:                               # @make_mid
# %bb.0:
	movq	%rcx, %rax
	movq	%rdx, (%rcx)
	leaq	1(%rdx), %rcx
	movq	%rcx, 8(%rax)
	addq	$2, %rdx
	movq	%rdx, 16(%rax)
	retq
                                        # -- End function
	.def	make_big_out;
	.scl	2;
	.type	32;
	.endef
	.globl	make_big_out                    # -- Begin function make_big_out
	.p2align	4
make_big_out:                           # @make_big_out
# %bb.0:
	movq	%rcx, %xmm0
	pshufd	$68, %xmm0, %xmm0               # xmm0 = xmm0[0,1,0,1]
	movdqa	__xmm@00000000000000010000000000000000(%rip), %xmm1 # xmm1 = [0,0,0,0,0,0,0,0,1,0,0,0,0,0,0,0]
	paddq	%xmm0, %xmm1
	paddq	__xmm@00000000000000030000000000000002(%rip), %xmm0
	movdqu	%xmm0, 16(%rdx)
	movdqu	%xmm1, (%rdx)
	retq
                                        # -- End function
	.def	use_init;
	.scl	2;
	.type	32;
	.endef
	.globl	use_init                        # -- Begin function use_init
	.p2align	4
use_init:                               # @use_init
.seh_proc use_init
# %bb.0:
	subq	$72, %rsp
	.seh_stackalloc 72
	.seh_endprologue
	movq	%rcx, %rdx
	leaq	40(%rsp), %rcx
	callq	make_big
	movq	64(%rsp), %rax
	addq	40(%rsp), %rax
	.seh_startepilogue
	addq	$72, %rsp
	.seh_endepilogue
	retq
	.seh_endproc
                                        # -- End function
	.def	use_init_out;
	.scl	2;
	.type	32;
	.endef
	.globl	use_init_out                    # -- Begin function use_init_out
	.p2align	4
use_init_out:                           # @use_init_out
.seh_proc use_init_out
# %bb.0:
	subq	$72, %rsp
	.seh_stackalloc 72
	.seh_endprologue
	leaq	40(%rsp), %rdx
	callq	make_big_out
	movq	64(%rsp), %rax
	addq	40(%rsp), %rax
	.seh_startepilogue
	addq	$72, %rsp
	.seh_endepilogue
	retq
	.seh_endproc
                                        # -- End function
	.def	use_expr;
	.scl	2;
	.type	32;
	.endef
	.globl	use_expr                        # -- Begin function use_expr
	.p2align	4
use_expr:                               # @use_expr
.seh_proc use_expr
# %bb.0:
	subq	$72, %rsp
	.seh_stackalloc 72
	.seh_endprologue
	movq	%rcx, %rdx
	leaq	40(%rsp), %rcx
	callq	make_big
	movq	48(%rsp), %rax
	.seh_startepilogue
	addq	$72, %rsp
	.seh_endepilogue
	retq
	.seh_endproc
                                        # -- End function
	.section	.debug$S,"dr"
	.p2align	2, 0x0
	.long	4                               # Debug section magic
	.long	241
	.long	.Ltmp1-.Ltmp0                   # Subsection size
.Ltmp0:
	.short	.Ltmp3-.Ltmp2                   # Record length
.Ltmp2:
	.short	4353                            # Record kind: S_OBJNAME
	.long	0                               # Signature
	.byte	0                               # Object name
	.p2align	2, 0x0
.Ltmp3:
	.short	.Ltmp5-.Ltmp4                   # Record length
.Ltmp4:
	.short	4412                            # Record kind: S_COMPILE3
	.long	0                               # Flags and language
	.short	208                             # CPUType
	.short	22                              # Frontend version
	.short	1
	.short	5
	.short	0
	.short	22015                           # Backend version
	.short	0
	.short	0
	.short	0
	.asciz	"clang version 22.1.5 (https://github.com/llvm/llvm-project 5ea218a153f4d2f815b8244eab3e4b4ba5e00e6c)" # Null-terminated compiler version string
	.p2align	2, 0x0
.Ltmp5:
.Ltmp1:
	.p2align	2, 0x0
	.addrsig
