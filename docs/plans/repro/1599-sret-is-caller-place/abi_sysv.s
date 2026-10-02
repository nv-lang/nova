	.file	"abi.c"
	.section	.rodata.cst16,"aM",@progbits,16
	.p2align	4, 0x0                          # -- Begin function make_big
.LCPI0_0:
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
.LCPI0_1:
	.quad	2                               # 0x2
	.quad	3                               # 0x3
	.text
	.globl	make_big
	.p2align	4
	.type	make_big,@function
make_big:                               # @make_big
	.cfi_startproc
# %bb.0:
	movq	%rsi, %xmm0
	pshufd	$68, %xmm0, %xmm0               # xmm0 = xmm0[0,1,0,1]
	movdqa	.LCPI0_0(%rip), %xmm1           # xmm1 = [0,0,0,0,0,0,0,0,1,0,0,0,0,0,0,0]
	paddq	%xmm0, %xmm1
	paddq	.LCPI0_1(%rip), %xmm0
	movq	%rdi, %rax
	movdqu	%xmm0, 16(%rdi)
	movdqu	%xmm1, (%rdi)
	retq
.Lfunc_end0:
	.size	make_big, .Lfunc_end0-make_big
	.cfi_endproc
                                        # -- End function
	.globl	make_mid                        # -- Begin function make_mid
	.p2align	4
	.type	make_mid,@function
make_mid:                               # @make_mid
	.cfi_startproc
# %bb.0:
	movq	%rdi, %rax
	movq	%rsi, (%rdi)
	leaq	1(%rsi), %rcx
	movq	%rcx, 8(%rdi)
	addq	$2, %rsi
	movq	%rsi, 16(%rdi)
	retq
.Lfunc_end1:
	.size	make_mid, .Lfunc_end1-make_mid
	.cfi_endproc
                                        # -- End function
	.section	.rodata.cst16,"aM",@progbits,16
	.p2align	4, 0x0                          # -- Begin function make_big_out
.LCPI2_0:
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
.LCPI2_1:
	.quad	2                               # 0x2
	.quad	3                               # 0x3
	.text
	.globl	make_big_out
	.p2align	4
	.type	make_big_out,@function
make_big_out:                           # @make_big_out
	.cfi_startproc
# %bb.0:
	movq	%rdi, %xmm0
	pshufd	$68, %xmm0, %xmm0               # xmm0 = xmm0[0,1,0,1]
	movdqa	.LCPI2_0(%rip), %xmm1           # xmm1 = [0,0,0,0,0,0,0,0,1,0,0,0,0,0,0,0]
	paddq	%xmm0, %xmm1
	paddq	.LCPI2_1(%rip), %xmm0
	movdqu	%xmm0, 16(%rsi)
	movdqu	%xmm1, (%rsi)
	retq
.Lfunc_end2:
	.size	make_big_out, .Lfunc_end2-make_big_out
	.cfi_endproc
                                        # -- End function
	.globl	use_init                        # -- Begin function use_init
	.p2align	4
	.type	use_init,@function
use_init:                               # @use_init
	.cfi_startproc
# %bb.0:
	subq	$40, %rsp
	.cfi_def_cfa_offset 48
	movq	%rdi, %rsi
	leaq	8(%rsp), %rdi
	callq	make_big
	movq	32(%rsp), %rax
	addq	8(%rsp), %rax
	addq	$40, %rsp
	.cfi_def_cfa_offset 8
	retq
.Lfunc_end3:
	.size	use_init, .Lfunc_end3-use_init
	.cfi_endproc
                                        # -- End function
	.globl	use_init_out                    # -- Begin function use_init_out
	.p2align	4
	.type	use_init_out,@function
use_init_out:                           # @use_init_out
	.cfi_startproc
# %bb.0:
	subq	$40, %rsp
	.cfi_def_cfa_offset 48
	leaq	8(%rsp), %rsi
	callq	make_big_out
	movq	32(%rsp), %rax
	addq	8(%rsp), %rax
	addq	$40, %rsp
	.cfi_def_cfa_offset 8
	retq
.Lfunc_end4:
	.size	use_init_out, .Lfunc_end4-use_init_out
	.cfi_endproc
                                        # -- End function
	.globl	use_expr                        # -- Begin function use_expr
	.p2align	4
	.type	use_expr,@function
use_expr:                               # @use_expr
	.cfi_startproc
# %bb.0:
	subq	$40, %rsp
	.cfi_def_cfa_offset 48
	movq	%rdi, %rsi
	leaq	8(%rsp), %rdi
	callq	make_big
	movq	16(%rsp), %rax
	addq	$40, %rsp
	.cfi_def_cfa_offset 8
	retq
.Lfunc_end5:
	.size	use_expr, .Lfunc_end5-use_expr
	.cfi_endproc
                                        # -- End function
	.ident	"clang version 22.1.5 (https://github.com/llvm/llvm-project 5ea218a153f4d2f815b8244eab3e4b4ba5e00e6c)"
	.section	".note.GNU-stack","",@progbits
	.addrsig
