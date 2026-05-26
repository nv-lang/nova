# Nova Language — VSCode Extension

VSCode / Cursor / VSCodium расширение для Nova: **TextMate syntax highlighting +
LSP diagnostics** через `nova-lsp`.

→ Полная инструкция установки: **[editors/INSTALL.md](../INSTALL.md)**

## Что даёт расширение

| Feature | Источник | Требует |
|---|---|---|
| Syntax highlighting (keywords, types, effects, literals) | TextMate grammar | — |
| Auto-closing brackets `{}`, `[]`, `()`, `""`, `` ` `` | language-configuration.json | — |
| **LSP diagnostics** (ошибки компилятора в gutter) | nova-lsp via JSON-RPC | `nova-lsp` в PATH или `nova.lsp.path` |
| **LSP hover / completion** (будущие планы 104.2–104.6) | nova-lsp | `nova-lsp` |

## Быстрая установка (разработка)

```bash
# 1. Установи зависимости
cd editors/vscode
npm install

# 2. Скомпилируй TypeScript
npm run build

# 3. Открой VSCode в корне репы, нажми F5 — запустится Extension Development Host
#    Открой любой .nv файл — syntax highlighting + (если nova-lsp в PATH) diagnostics
```

## Конфигурация

| Setting | Default | Описание |
|---|---|---|
| `nova.lsp.path` | `null` (auto) | Путь к `nova-lsp` binary. Если `null` — ищет в PATH, потом в `target/release/` |
| `nova.lsp.enabled` | `true` | Включить/выключить LSP сервер |
| `nova.lsp.trace.server` | `off` | Трассировка JSON-RPC (`messages` / `verbose`) |

**Пример settings.json** (если nova-lsp собран в кастомном месте):
```json
{
    "nova.lsp.path": "C:/Users/me/.cargo/bin/nova-lsp.exe"
}
```

## Структура

```
editors/vscode/
├── client/
│   └── extension.ts         # LSP client (LanguageClient → nova-lsp)
├── tests/
│   ├── runTests.ts          # @vscode/test-electron runner
│   └── suite/
│       ├── index.ts         # Mocha root
│       └── extension.test.ts # test cases (pos1–pos4, neg1–neg2, edge1)
├── syntaxes/
│   └── nova.tmLanguage.json # TextMate grammar
├── package.json             # манифест (contributes, dependencies)
├── tsconfig.json            # TS build config
├── language-configuration.json # brackets, comments, indent
├── .vscodeignore            # исключить из .vsix packaging
└── README.md                # этот файл
```

## Troubleshooting

**Diagnostics не появляются:**
1. Проверь `Output → Nova LSP` — там причина.
2. Убедись что `nova-lsp` собран: `cargo build --release -p nova-lsp`
3. Добавь в PATH или укажи в `nova.lsp.path`

**"Too many errors" message:**
LSP упал 3+ раз подряд. Смотри `Output → Nova LSP` для stacktrace.
Обычно: binary не найден, или nova-lsp упал при парсинге файла.

**Syntax highlighting не работает:**
Расширение не активировалось. Убедись что файл имеет расширение `.nv`.
Или: `Ctrl+K M` → выбери `Nova`.
