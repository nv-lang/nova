# Plan 221, атом A-V5 — vsix-артефакт VSCode-расширения

**Статус:** готово (артефакт собран, НЕ закоммичен — прикладывается к GitHub Release руками).

## Артефакт

- Путь (в worktree `p221-vsix`): `editors/vscode/nova-lang-0.1.0.vsix`
- Размер: 486 671 байт (475.26 KB по данным vsce, 333 файла)
- sha256: `13d72da0961e7e105592f8f3c79bbbcbc8221dd2174cf50ed685933e2832a2aa`

## Правки package.json (editors/vscode/package.json)

- `version`: `0.2.0` → `0.1.0` (синхронизация с релизом Nova v0.1.0)
- `publisher`: `nova-lang-local` → `nv-lang`
- добавлено поле `repository` (`https://github.com/nv-lang/nova.git`, `directory: editors/vscode`) —
  без него `vsce package` падал с ошибкой разрешения относительной ссылки `../INSTALL.md` в README.md

## Проверено содержимое .vsix (это zip, распакован через `vsce ls --tree`)

- `syntaxes/nova.tmLanguage.json` (10.73 KB) — TextMate-грамматика подсветки `.nv`
- `language-configuration.json` (0.82 KB) — скобки/комментарии/автозакрытие
- `out/client/extension.js` (+ .d.ts/.map) — скомпилированный клиент, поднимает LSP через
  `vscode-languageclient`, ищет `nova-lsp` бинарник (настройка `nova.lsp.path` / PATH /
  `target/release/nova-lsp[.exe]`)
- `node_modules/vscode-languageclient`, `vscode-jsonrpc`, `vscode-languageserver-protocol`,
  `vscode-languageserver-types` и их транзитивные зависимости (semver, minimatch,
  brace-expansion, balanced-match) — рантайм-зависимости клиента, включены закономерно
  (см. комментарий в `.vscodeignore`: "node_modules НУЖЕН")
- `package.json`, `readme.md` — присутствуют

## Предупреждения при сборке (допустимые, не блокеры)

- `WARNING LICENSE, LICENSE.md, or LICENSE.txt not found` — лицензия проекта не продублирована
  в editors/vscode/; для релиза не критично (корень репо содержит лицензию)
- `WARNING This extension consists of 333 files, out of which 169 are JavaScript files...
  you should bundle your extension` — рекомендация по бандлингу (esbuild/webpack), не сделано;
  расширение рабочее и в текущем виде, оптимизация — отдельная задача не в рамках A-V5

## Как пересобрать

```bash
cd editors/vscode
npm install          # если node_modules ещё нет
npm run compile      # tsc -p tsconfig.json → out/
npx @vscode/vsce package
# результат: nova-lang-0.1.0.vsix в editors/vscode/
```

Проверить содержимое без распаковки: `npx @vscode/vsce ls --tree`.

sha256 на Windows/Git Bash: `sha256sum nova-lang-0.1.0.vsix`.
