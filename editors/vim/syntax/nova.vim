" Vim syntax file for Nova
" Language:    Nova
" Maintainer:  Nova Language project
" URL:         https://github.com/nv-lang/nova
" License:     MIT OR Apache-2.0
"
" Synchronized with editors/vscode/syntaxes/nova.tmLanguage.json
" Source-of-truth for keywords: compiler-codegen/src/lexer/mod.rs

if exists("b:current_syntax")
    finish
endif

" ============================================================================
" Keywords
" ============================================================================

" Declarations
" Plan 114 (D184): let/readonly retracted; ro/mut/consume binding triad.
syntax keyword novaDeclaration fn type alias effect handler protocol const module import export as use test external

" Storage modifiers
syntax keyword novaModifier ro mut consume

" Control flow
syntax keyword novaConditional if else match
syntax keyword novaRepeat for while loop
syntax keyword novaStatement break continue return throw with interrupt
syntax keyword novaStatement defer

" Concurrency primitives (D14, D50, D75)
syntax keyword novaConcurrency spawn detach parallel supervised cancel_scope race select with_timeout

" Memory / safety blocks (D6, D63, D64)
syntax keyword novaMemory region forbid realtime

" Operators / patterns
syntax keyword novaOperator is in
syntax keyword novaContract requires ensures invariant old

" Boolean literals
syntax keyword novaBoolean true false

" ============================================================================
" Types — prelude (D26)
" ============================================================================

" Sum-types and constructors
syntax keyword novaPreludeType Option Some None Result Ok Err Ordering Less Equal Greater
syntax keyword novaPreludeType Error RuntimeError Never Self

" Iterator / Range (D58)
syntax keyword novaPreludeType Iter Range RangeIter

" Concurrency types (D75, D79)
syntax keyword novaPreludeType Channel CancelToken Handler

" Conversion protocols (D73, D77)
syntax keyword novaPreludeType From Into TryFrom TryInto

" Structural protocols
syntax keyword novaPreludeType Hashable Eq Ord

" Builders (Plan 04 — not yet implemented)
syntax keyword novaPreludeType StringBuilder WriteBuffer ReadBuffer ReadBufferError

" ============================================================================
" Standard effects (D2, D62)
" ============================================================================

syntax keyword novaEffect Fail Io Net Db Fs Time Random Log Trace Ask Alloc Detach Blocking Mem

" ============================================================================
" Primitive types
" ============================================================================

syntax keyword novaPrimitiveType int i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 str bool char byte any

" ============================================================================
" Identifiers — PascalCase = type, SCREAMING_SNAKE = constant
" ============================================================================

syntax match novaType /\<[A-Z][a-zA-Z0-9_]*\>/
syntax match novaConstant /\<[A-Z][A-Z0-9_]\+\>/

" ============================================================================
" Self-field access via @
" ============================================================================

syntax match novaSelfField /@[a-z_][a-zA-Z0-9_]*/
syntax match novaSelfField /@\ze[^a-zA-Z_]/

" ============================================================================
" Numeric literals
" ============================================================================

syntax match novaNumber /\<\d\(_\?\d\)*\>/
syntax match novaNumber /\<\d\(_\?\d\)*\.\d\(_\?\d\)*\([eE][+-]\?\d\(_\?\d\)*\)\?\>/
syntax match novaNumber /\<\d\(_\?\d\)*[eE][+-]\?\d\(_\?\d\)*\>/
syntax match novaNumber /\<0x[0-9a-fA-F]\(_\?[0-9a-fA-F]\)*\>/
syntax match novaNumber /\<0b[01]\(_\?[01]\)*\>/
syntax match novaNumber /\<0o[0-7]\(_\?[0-7]\)*\>/

" ============================================================================
" Strings
" ============================================================================

" Regular string with interpolation ${...}
syntax region novaString start=/"/ skip=/\\./ end=/"/ contains=novaStringEscape,novaStringInterp
syntax match novaStringEscape /\\./ contained
syntax region novaStringInterp matchgroup=novaStringInterpDelim start=/\${/ end=/}/ contained contains=TOP

" Char literal: 'a' / '\n' / '\u{1F600}'
syntax match novaChar /'\(\\.\|[^']\)'/

" Tagged template literals: json`...`, sql`...`, regex`...`, bytes`...`
syntax region novaTagTemplate matchgroup=novaTagName start=/\<\(json\|sql\|regex\|bytes\)\zs`/ end=/`/

" ============================================================================
" Comments
" ============================================================================

syntax match novaDocComment "///.*$"
syntax match novaLineComment "//.*$"
syntax region novaBlockComment start="/\*" end="\*/"

" ============================================================================
" Operators (single-token, regex-matched for visual)
" ============================================================================

syntax match novaArrow /->\|=>/
syntax match novaErrorOp /??\|?/
syntax match novaRangeOp /\.\.=\?/

" ============================================================================
" Highlight link to standard groups
" ============================================================================

highlight default link novaDeclaration   Keyword
highlight default link novaModifier      StorageClass
highlight default link novaConditional   Conditional
highlight default link novaRepeat        Repeat
highlight default link novaStatement     Statement
highlight default link novaConcurrency   Statement
highlight default link novaMemory        Statement
highlight default link novaOperator      Operator
highlight default link novaContract      PreProc
highlight default link novaBoolean       Boolean

highlight default link novaPreludeType    Type
highlight default link novaEffect         Type
highlight default link novaPrimitiveType  Type
highlight default link novaType           Type
highlight default link novaConstant       Constant

highlight default link novaSelfField      Identifier

highlight default link novaNumber         Number

highlight default link novaString         String
highlight default link novaStringEscape   SpecialChar
highlight default link novaStringInterp   Special
highlight default link novaStringInterpDelim Special
highlight default link novaChar           Character
highlight default link novaTagName        Function
highlight default link novaTagTemplate    String

highlight default link novaDocComment     SpecialComment
highlight default link novaLineComment    Comment
highlight default link novaBlockComment   Comment

highlight default link novaArrow          Operator
highlight default link novaErrorOp        Operator
highlight default link novaRangeOp        Operator

let b:current_syntax = "nova"
