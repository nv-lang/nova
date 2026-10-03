// .opencode/plugins/nova-guards/index.ts — механизмы проекта для OpenCode V2.
// Каталог-пакет, а не одиночный файл: в `plugins` конфига OpenCode принимает
// только каталог («configured plugin path must be a directory», замер 2026-10-03).
//
// ЗАЧЕМ (реестр 221.1 №1715, замер 2026-10-03). В OpenCode хуки Claude Code из
// `.claude/settings.json` НЕ исполняются: за десять вызовов оболочки хук времени не
// подал ни строки, `guard-git.py` не судил ни одной команды, список запретов
// (чтение секретов, `git reset --hard`, `git clean -fd`) не действовал. Вдобавок
// оболочки фоновых субагентов получали окружение сервиса, а не лаунчера: `bash`
// там — WSL, `PYTHONUTF8` пуст, и страж `check-worktree-location` ответил «не
// git-репозиторий», ничего не проверив.
//
// ЧТО ДЕЛАЕТ — переносит МЕХАНИЗМЫ, а не их копии:
//   1. Хуки `PreToolUse` для оболочки — те же python-скрипты, прочитанные из
//      `.claude/settings.json` при КАЖДОМ вызове (дом хуков один; новый хук там
//      начинает действовать и здесь). Код 2 = отказ с текстом хука.
//   2. `permissions.deny` того же файла: `Read(...)` — запрет чтения по маске,
//      `Bash(...)`/`PowerShell(...)` — запрет команды по префиксу (`git -C <путь>`
//      перед подкомандой не спасает: путь срезается до сравнения).
//   3. Окружение КАЖДОЙ команды оболочки, кто бы её ни запустил: Git Bash первым
//      в PATH (Windows), `PYTHONUTF8=1`, `TEMP`/`TMP`/`TMPDIR` — каталог `Temp` на
//      диске репозитория, если он есть (правило `/integrator`, №1152). Только для
//      команд: сам процесс OpenCode с `TEMP` на другом диске не стартует (ошибка
//      126, замер 2026-10-03), поэтому лаунчер его не задаёт. В Windows
//      PowerShell — чтение и вывод в UTF-8 (без этого кириллица — кракозябры).
//   4. Время из машины в каждый запрос модели (правило `AGENTS.md`, «Время»;
//      в Claude Code это делал `show-local-time.py`).
//   5. Слэш-команды из `.claude/commands/*.md`: шаблон читается из файла при
//      вызове, копии нет.
//
// ЧЕГО НЕ ДЕЛАЕТ (названо, а не забыто): хуки `Write` (`guard-memory.py` судит
// память Claude Code, которой здесь нет), `Stop`, `SessionStart`, `PostToolUse`
// кроме времени — у OpenCode нет их точного аналога. Агенты `.claude/agents`
// API плагина добавить не позволяет (у редактора агентов нет `add`), поэтому они
// лежат указателями в `.opencode/agents/`.
//
// ОТКАЗ ХУКА ЛОМАЕТСЯ В СТОРОНУ ПРОПУСКА только при сбое самого плагина
// (не запустился python и т. п.) — так же, как у хуков Claude Code; каждый такой
// сбой пишется в журнал `<tmp>/nova-guards.log`.

import { spawn, execFileSync } from "node:child_process"
import { appendFileSync, existsSync, readFileSync, readdirSync } from "node:fs"
import os from "node:os"
import path from "node:path"
import { fileURLToPath } from "node:url"

// <корень>/.opencode/plugins/nova-guards/index.ts
const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..", "..")
const ROOT_POSIX = ROOT.replace(/\\/g, "/")
const LOG = path.join(os.tmpdir(), "nova-guards.log")
const HOOK_TIMEOUT_MS = 20_000

function log(line: string) {
  try {
    appendFileSync(LOG, `${new Date().toISOString()} ${line}\n`)
  } catch {}
}

function readSettings(): any {
  try {
    return JSON.parse(readFileSync(path.join(ROOT, ".claude", "settings.json"), "utf8"))
  } catch (e) {
    log(`settings unreadable: ${e}`)
    return {}
  }
}

// ── хуки PreToolUse ─────────────────────────────────────────────────────────

function preToolCommands(settings: any, tool: string): string[] {
  const out: string[] = []
  for (const entry of settings?.hooks?.PreToolUse ?? []) {
    let re: RegExp
    try {
      re = new RegExp(`^(?:${entry.matcher ?? ".*"})$`)
    } catch {
      continue
    }
    if (!re.test(tool)) continue
    for (const h of entry.hooks ?? []) if (h.type === "command" && h.command) out.push(h.command)
  }
  return out
}

function runHook(command: string, payload: unknown, sessionID: string): Promise<{ code: number; stderr: string }> {
  const cmd = command.replace(/\$\{?CLAUDE_PROJECT_DIR\}?/g, ROOT_POSIX)
  return new Promise((resolve) => {
    let stderr = ""
    let done = false
    const finish = (code: number) => {
      if (done) return
      done = true
      resolve({ code, stderr })
    }
    try {
      const child = spawn(cmd, {
        cwd: ROOT,
        shell: true,
        windowsHide: true,
        env: { ...process.env, CLAUDE_PROJECT_DIR: ROOT, CLAUDE_CODE_SESSION_ID: sessionID, PYTHONUTF8: "1", PYTHONIOENCODING: "utf-8" },
      })
      const timer = setTimeout(() => {
        log(`hook timeout: ${cmd}`)
        child.kill()
        finish(0)
      }, HOOK_TIMEOUT_MS)
      child.stderr?.on("data", (d) => (stderr += d.toString("utf8")))
      child.on("error", (e) => {
        clearTimeout(timer)
        log(`hook spawn failed: ${cmd}: ${e}`)
        finish(0)
      })
      child.on("close", (code) => {
        clearTimeout(timer)
        finish(code ?? 0)
      })
      child.stdin?.end(JSON.stringify(payload))
    } catch (e) {
      log(`hook crashed: ${cmd}: ${e}`)
      finish(0)
    }
  })
}

// ── permissions.deny ────────────────────────────────────────────────────────

type DenyRule = { tool: string; pattern: string }

function denyRules(settings: any): DenyRule[] {
  const out: DenyRule[] = []
  for (const raw of settings?.permissions?.deny ?? []) {
    const m = /^(\w+)\((.*)\)$/.exec(String(raw))
    if (m) out.push({ tool: m[1], pattern: m[2] })
  }
  return out
}

function globToRegExp(glob: string): RegExp {
  let g = glob.replace(/\\/g, "/").replace(/^\.\//, "")
  let re = ""
  for (let i = 0; i < g.length; i++) {
    const c = g[i]
    if (c === "*" && g[i + 1] === "*") {
      re += g[i + 2] === "/" ? "(?:.*/)?" : ".*"
      i += g[i + 2] === "/" ? 2 : 1
    } else if (c === "*") re += "[^/]*"
    else if (c === "?") re += "[^/]"
    else re += c.replace(/[.+^${}()|[\]\\]/g, "\\$&")
  }
  return new RegExp(`^${re}$`, "i")
}

function readDenied(rules: DenyRule[], file: string): string | undefined {
  const abs = path.resolve(ROOT, file).replace(/\\/g, "/")
  const rel = abs.toLowerCase().startsWith(ROOT_POSIX.toLowerCase() + "/") ? abs.slice(ROOT_POSIX.length + 1) : abs
  for (const r of rules) {
    if (r.tool !== "Read") continue
    if (globToRegExp(r.pattern).test(rel)) return `Read(${r.pattern})`
  }
  return undefined
}

// `git -C <путь>` и `git -c k=v` срезаются: иначе `git -C . reset --hard`
// проходил бы мимо запрета `git reset --hard` в обеих средах.
function normalizeSegment(seg: string): string {
  return seg
    .trim()
    .replace(/^git((?:\s+-[Cc]\s+(?:"[^"]*"|'[^']*'|\S+))+)/, "git")
    .replace(/\s+/g, " ")
}

function shellDenied(rules: DenyRule[], command: string): string | undefined {
  const segments = command.split(/&&|\|\||;|\||\r?\n/).map(normalizeSegment).filter(Boolean)
  for (const r of rules) {
    if (r.tool !== "Bash" && r.tool !== "PowerShell") continue
    const prefix = r.pattern.replace(/:\*$/, "").replace(/\s+/g, " ").trim()
    if (segments.some((s) => s === prefix || s.startsWith(prefix + " ") || s.startsWith(prefix))) return `${r.tool}(${r.pattern})`
  }
  return undefined
}

function shellCommandOf(ev: any): string {
  const meta = ev?.metadata ?? {}
  if (typeof meta.command === "string" && meta.command) return meta.command
  const res: string[] = Array.isArray(ev?.resources) ? ev.resources : []
  return res.join("\n")
}

// ── окружение команд ────────────────────────────────────────────────────────

function gitBashDir(): string | undefined {
  if (process.platform !== "win32") return undefined
  try {
    const exec = execFileSync("git", ["--exec-path"], { encoding: "utf8", windowsHide: true }).trim()
    // <git>/mingw64/libexec/git-core -> <git>/bin
    const bin = path.resolve(exec, "..", "..", "..", "bin")
    return existsSync(path.join(bin, "bash.exe")) ? bin : undefined
  } catch (e) {
    log(`git --exec-path failed: ${e}`)
    return undefined
  }
}

function runTempDir(): { win: string; posix: string } | undefined {
  if (process.platform !== "win32") return undefined
  const drive = /^([A-Za-z]):/.exec(ROOT)?.[1]
  if (!drive) return undefined
  const win = `${drive.toUpperCase()}:\\Temp`
  return existsSync(win) ? { win, posix: `/${drive.toLowerCase()}/Temp` } : undefined
}

const PS_UTF8 =
  "[Console]::OutputEncoding=[Text.Encoding]::UTF8;$OutputEncoding=[Text.Encoding]::UTF8;" +
  "$PSDefaultParameterValues['Get-Content:Encoding']='UTF8';$PSDefaultParameterValues['Select-String:Encoding']='UTF8';\n"

function envKey(env: Record<string, string | undefined>, name: string): string {
  return Object.keys(env).find((k) => k.toLowerCase() === name.toLowerCase()) ?? name
}

// ── команды ─────────────────────────────────────────────────────────────────

function commandFiles(): { name: string; file: string; description: string }[] {
  const dir = path.join(ROOT, ".claude", "commands")
  if (!existsSync(dir)) return []
  return readdirSync(dir)
    .filter((f) => f.endsWith(".md"))
    .map((f) => {
      const file = path.join(dir, f)
      const head = /^---\r?\n([\s\S]*?)\r?\n---/.exec(readFileSync(file, "utf8"))?.[1] ?? ""
      const description = /^description:\s*"?(.*?)"?\s*$/m.exec(head)?.[1] ?? ""
      return { name: f.slice(0, -3), file, description }
    })
}

function commandBody(file: string): string {
  return readFileSync(file, "utf8").replace(/^---\r?\n[\s\S]*?\r?\n---\r?\n?/, "")
}

// ── плагин ──────────────────────────────────────────────────────────────────

export default {
  id: "nova.guards",
  async setup(ctx: any) {
    const bashDir = gitBashDir()
    const temp = runTempDir()
    log(`setup root=${ROOT} gitbash=${bashDir ?? "-"} temp=${temp?.win ?? "-"}`)

    await ctx.permission.hook("evaluate", async (ev: any) => {
      try {
        if (ev.effect === "deny") return
        const settings = readSettings()
        const rules = denyRules(settings)
        if (ev.action === "read") {
          for (const f of ev.resources ?? []) {
            const hit = readDenied(rules, String(f))
            if (hit) {
              ev.effect = "deny"
              ev.message = `nova-guards: ${hit} (.claude/settings.json permissions.deny)`
              log(`deny read ${f} by ${hit}`)
              return
            }
          }
          return
        }
        if (ev.action !== "shell") return
        const command = shellCommandOf(ev)
        if (!command) return
        const hit = shellDenied(rules, command)
        if (hit) {
          ev.effect = "deny"
          ev.message = `nova-guards: ${hit} (.claude/settings.json permissions.deny)`
          log(`deny shell by ${hit}: ${command}`)
          return
        }
        const payload = { session_id: ev.sessionID, hook_event_name: "PreToolUse", tool_name: "Bash", tool_input: { command }, cwd: ROOT }
        for (const hook of preToolCommands(settings, "Bash")) {
          const r = await runHook(hook, payload, String(ev.sessionID ?? ""))
          if (r.code === 2) {
            ev.effect = "deny"
            ev.message = r.stderr.trim() || `nova-guards: refused by ${hook}`
            log(`deny shell by hook ${hook}: ${command}`)
            return
          }
        }
      } catch (e) {
        log(`evaluate failed: ${e}`)
      }
    })

    await ctx.shell.hook("create.before", (ev: any) => {
      try {
        const env = ev.env
        env.PYTHONUTF8 = "1"
        if (bashDir) {
          const k = envKey(env, "PATH")
          const cur = env[k] ?? ""
          if (!cur.toLowerCase().startsWith(bashDir.toLowerCase() + ";")) env[k] = `${bashDir};${cur}`
        }
        if (temp) {
          env[envKey(env, "TEMP")] = temp.win
          env[envKey(env, "TMP")] = temp.win
          env.TMPDIR = temp.posix
        }
        // Windows PowerShell 5.1 пишет вывод в кодовой странице консоли (cp866),
        // а OpenCode читает его как UTF-8 — кириллица приходила кракозябрами, и
        // `Get-Content` без `-Encoding` читал UTF-8-файлы как ANSI (замер
        // 2026-10-03). Чтение и вывод переводятся на UTF-8; ЗАПИСЬ не трогается:
        // `*:Encoding` у Set-Content/Out-File в 5.1 ставил бы BOM в файлы.
        if (/powershell/i.test(String(ev.shell ?? "")) && !String(ev.command).startsWith(PS_UTF8)) {
          ev.command = PS_UTF8 + ev.command
        }
      } catch (e) {
        log(`shell env failed: ${e}`)
      }
    })

    await ctx.session.hook("context", (ev: any) => {
      try {
        const d = new Date()
        const hh = String(d.getHours()).padStart(2, "0")
        const mm = String(d.getMinutes()).padStart(2, "0")
        ev.system.push({ type: "text", text: `ВРЕМЯ СЕЙЧАС ${hh}:${mm} (местное, часы машины; nova-guards)` })
      } catch (e) {
        log(`time inject failed: ${e}`)
      }
    })

    try {
      const existing = new Set<string>()
      try {
        const list = await ctx.command.list()
        for (const c of list?.data ?? list ?? []) if (c?.name) existing.add(String(c.name))
      } catch {}
      const cmds = commandFiles().filter((c) => !existing.has(c.name))
      await ctx.command.transform((editor: any) => {
        for (const c of cmds) {
          editor.add({
            name: c.name,
            description: c.description,
            execute: async ({ sessionID, prompt, delivery }: any) => {
              const args = prompt?.text ?? ""
              const text = commandBody(c.file).split("$ARGUMENTS").join(args)
              await ctx.session.prompt({ ...prompt, sessionID, text, delivery })
            },
          })
        }
      })
      log(`commands registered: ${cmds.map((c) => c.name).join(",")}`)
    } catch (e) {
      log(`commands failed: ${e}`)
    }
  },
}
