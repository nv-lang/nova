# Nova Language — Vim / Neovim syntax highlighting

Vim plugin для подсветки синтаксиса `.nv` файлов.

## Структура

```
editors/vim/
├── ftdetect/nova.vim          ассоциация *.nv → filetype=nova
├── ftplugin/nova.vim          comment-string, indent settings
├── syntax/nova.vim            подсветка (keywords, types, операторы)
└── README.md                  этот файл
```

## Установка

### Vim native (без plugin manager'а)

**Linux/Mac:**

```sh
mkdir -p ~/.vim/{ftdetect,ftplugin,syntax}
ln -s /path/to/nova-lang/editors/vim/ftdetect/nova.vim   ~/.vim/ftdetect/nova.vim
ln -s /path/to/nova-lang/editors/vim/ftplugin/nova.vim   ~/.vim/ftplugin/nova.vim
ln -s /path/to/nova-lang/editors/vim/syntax/nova.vim     ~/.vim/syntax/nova.vim
```

**Windows:**

```powershell
$vim_home = "$env:USERPROFILE\vimfiles"
New-Item -ItemType Directory -Force "$vim_home\ftdetect", "$vim_home\ftplugin", "$vim_home\syntax" | Out-Null
New-Item -ItemType SymbolicLink -Path "$vim_home\ftdetect\nova.vim" `
    -Target "d:\Sources\nova-lang\editors\vim\ftdetect\nova.vim"
New-Item -ItemType SymbolicLink -Path "$vim_home\ftplugin\nova.vim" `
    -Target "d:\Sources\nova-lang\editors\vim\ftplugin\nova.vim"
New-Item -ItemType SymbolicLink -Path "$vim_home\syntax\nova.vim" `
    -Target "d:\Sources\nova-lang\editors\vim\syntax\nova.vim"
```

### Neovim native

То же что и Vim, но путь — `~/.config/nvim/` (Linux/Mac) или
`%LOCALAPPDATA%\nvim\` (Windows):

```sh
mkdir -p ~/.config/nvim/{ftdetect,ftplugin,syntax}
ln -s /path/to/nova-lang/editors/vim/ftdetect/nova.vim   ~/.config/nvim/ftdetect/nova.vim
ln -s /path/to/nova-lang/editors/vim/ftplugin/nova.vim   ~/.config/nvim/ftplugin/nova.vim
ln -s /path/to/nova-lang/editors/vim/syntax/nova.vim     ~/.config/nvim/syntax/nova.vim
```

### Через plugin manager

**vim-plug** (`~/.vimrc`):

```vim
Plug '/path/to/nova-lang/editors/vim'
```

**packer.nvim / lazy.nvim** (Neovim, Lua):

```lua
-- packer.nvim
use { dir = '/path/to/nova-lang/editors/vim' }

-- lazy.nvim
{ dir = '/path/to/nova-lang/editors/vim', name = 'nova-vim' }
```

Если репо опубликовано в GitHub — заменить `dir = '...'` на
`'org/nova-vim'`.

## Проверка

1. Открой любой `.nv` файл (`examples/basics/hello.nv`).
2. Команда `:set filetype?` должна вернуть `filetype=nova`.
3. Код должен раскраситься (зависит от colorscheme).
4. Если ничего не раскрасилось — `:syntax on` в `.vimrc` обязательно.

## Что подсвечивается

То же что в VSCode-версии — keywords (declaration, control flow,
concurrency, memory), prelude-типы, эффекты, примитивы, числовые/
строковые литералы, char-литералы, tagged templates (`json`/`sql`/
`regex`/`bytes`), комментарии, `@field`-доступ, операторы.

Полный list синхронизирован с
[`../vscode/syntaxes/nova.tmLanguage.json`](../vscode/syntaxes/nova.tmLanguage.json).
Source-of-truth для keyword'ов —
[`compiler-codegen/src/lexer/mod.rs`](../../compiler-codegen/src/lexer/mod.rs)
функция `lex_ident_or_keyword`.

## Рекомендуемые plugin'ы

Для лучшего experience с Nova-кодом (как в VSCode bracket pair
colorization):

- **[rainbow](https://github.com/luochen1990/rainbow)** или
  **[rainbow-delimiters.nvim](https://github.com/HiPhish/rainbow-delimiters.nvim)** —
  парные `{}`/`()`/`[]` подсвечиваются разными цветами по уровню
  вложенности.
- **[indentLine](https://github.com/Yggdroot/indentLine)** (Vim) или
  **[indent-blankline.nvim](https://github.com/lukas-reineke/indent-blankline.nvim)**
  (Neovim) — вертикальные направляющие отступов.
- **[vim-matchup](https://github.com/andymass/vim-matchup)** —
  подсветка парных скобок и `if`/`else`/match-arms.

В `~/.vimrc` или `init.lua`:

```vim
" Vim
let g:rainbow_active = 1
syntax on

" Per-Nova settings (если нужно отдельно)
augroup NovaSettings
    autocmd!
    autocmd FileType nova setlocal expandtab shiftwidth=4 tabstop=4
augroup END
```

```lua
-- Neovim
require('rainbow-delimiters.setup').setup{}
require('ibl').setup{}
```

## Что не работает

То же что в VSCode-версии:
- **Семантический анализ** — нет (нужен LSP-сервер).
- **Auto-complete** — только из открытых буферов (`<C-n>`/`<C-p>`).
- **Go to definition** — нет (можно через `:tag`/ctags если сгенерируешь
  tag-file самостоятельно).
- **Подсветка ошибок** — нет.

Для полноценной IDE-интеграции в Neovim — нужен LSP через
`nvim-lspconfig` (когда LSP-сервер появится).

## Как править

| Что изменилось | Куда |
|---|---|
| Новый keyword | `syntax/nova.vim` секция «Keywords» |
| Новый stdlib-effect | секция «Standard effects» |
| Новый prelude-тип | секция «Types — prelude» |
| Новый примитив | секция «Primitive types» |
| Новый оператор | секция «Operators» |

После правки — `:source ~/.vim/syntax/nova.vim` или просто перезагрузи
буфер (`:e`).

## Известные ограничения

1. **Vim regex weaker than Oniguruma** (используется в TextMate). Некоторые
   сложные паттерны (вложенная интерполяция строк) могут работать менее
   точно чем в VSCode.
2. **Tagged template literals** — region-подсветка работает для
   фиксированных тегов (`json`/`sql`/`regex`/`bytes`). User-defined теги
   подсвечиваются как обычные backtick-строки.
3. **PascalCase identifier** подсвечивается как `Type`, но Vim не
   различает «декларация типа» от «использование типа».

Эти ограничения — фундаментальные для Vim regex syntax. Для tree-sitter
based Neovim 0.5+ возможна более точная подсветка через отдельный
tree-sitter grammar (отдельный проект, не сделан).
