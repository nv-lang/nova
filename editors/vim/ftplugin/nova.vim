" Filetype settings for Nova
" Loaded automatically when filetype=nova

if exists("b:did_ftplugin")
    finish
endif
let b:did_ftplugin = 1

" Comments
setlocal commentstring=//\ %s
setlocal comments=://,:///,s1:/*,mb:*,ex:*/

" Indentation — 4 spaces, no tabs
setlocal expandtab
setlocal shiftwidth=4
setlocal softtabstop=4
setlocal tabstop=4

" Auto-indent inside braces
setlocal autoindent
setlocal smartindent

" Word characters — for `w`, `b` motions; allow @ as part of word
" so that @field navigation works
setlocal iskeyword+=@-@

let b:undo_ftplugin = "setlocal commentstring< comments<" .
    \ " expandtab< shiftwidth< softtabstop< tabstop<" .
    \ " autoindent< smartindent< iskeyword<"
