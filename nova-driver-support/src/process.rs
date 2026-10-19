//! Compiler-free child-process capture and timeout utilities.
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

pub fn wait_with_timeout(
    child: &mut Child,
    timeout: Duration,
) -> std::io::Result<Option<ExitStatus>> {
    let start = Instant::now();
    // Plan 26 Ф.16 #8: adaptive poll backoff. 1ms → 2 → 5 → 10 → 25 → 50 ms.
    // На fast тестах (<10ms) overhead был 100% c fixed 10ms; теперь <1ms
    // на первой итерации. Для long тестов экономим CPU 5× через 50ms cap.
    let poll_steps_ms = [1, 2, 5, 10, 25, 50];
    let mut step = 0usize;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        if start.elapsed() >= timeout {
            // Best-effort kill. На Windows TerminateProcess, на Unix SIGKILL.
            let _ = child.kill();
            // Дренируем zombie, иначе fd-leak.
            let _ = child.wait();
            return Ok(None);
        }
        let poll_ms = poll_steps_ms[step.min(poll_steps_ms.len() - 1)];
        std::thread::sleep(Duration::from_millis(poll_ms));
        step = (step + 1).min(poll_steps_ms.len() - 1);
    }
}

/// Plan 26 Ф.16 #2: join thread с safety-timeout. Возвращает результат
/// если поток закончил в течение `timeout`, иначе detach + empty default.
/// Cross-platform через mpsc channel — std::thread::JoinHandle не
/// предоставляет timed join.
fn join_with_timeout(handle: std::thread::JoinHandle<Vec<u8>>, timeout: Duration) -> Vec<u8> {
    use std::sync::mpsc;
    // Re-wrap join'а в отдельном thread'е → result через channel.
    // Если channel.recv_timeout вернул Err — оригинальный поток detach'нут
    // (он живёт до конца process'а, но мы не блокированы).
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = handle.join().unwrap_or_default();
        let _ = tx.send(result);
    });
    rx.recv_timeout(timeout).unwrap_or_default()
}

/// Plan 209 Ф.2: synthesize a plain success/failure `ExitStatus`. The
/// multi-TU compile+link path (`compile_multi_tu_to_exe`) runs SEVERAL
/// subprocesses (N parallel part compiles + a link) folded into one
/// `anyhow::Result` — this lets `run_one` feed that single verdict into the
/// SAME `(CapturedOutput, ExitStatus)`-shaped post-cc branching
/// (`EXPECT_CC_ERROR` matching, run-the-exe, …) that a real single child
/// process's exit status already drives for the single-TU path, without
/// duplicating that (large) downstream logic.
#[cfg(target_os = "windows")]
pub fn synth_exit_status(success: bool) -> ExitStatus {
    use std::os::windows::process::ExitStatusExt;
    ExitStatus::from_raw(if success { 0 } else { 1 })
}
#[cfg(not(target_os = "windows"))]
pub fn synth_exit_status(success: bool) -> ExitStatus {
    use std::os::unix::process::ExitStatusExt;
    ExitStatus::from_raw(if success { 0 } else { 1 })
}

/// Имя сигнала, которым убит процесс, — или пустая строка, если он завершился
/// сам (реестр 221.1 №975).
///
/// ЗАЧЕМ. `ExitStatus::code()` на Unix возвращает `None` РОВНО тогда, когда
/// процесс завершён сигналом, а вызывающий сводит это к `-1`. В отчёте все
/// сигналы становятся неотличимы: OOM-killer (`SIGKILL`), переполнение стека
/// (`SIGSEGV`) и паника рантайма (`SIGABRT`) печатаются ОДНИМ И ТЕМ ЖЕ
/// `exit=-1 (0xFFFFFFFF)`. Соседний комментарий ниже верно говорит, что код
/// возврата диагностичен, и перечисляет NT-статусы — но это ВИНДОВЫЕ коды, а
/// юниксовый эквивалент терялся. Из-за этого причина падения
/// `presume_446_stress` на CI (реестр №968, №862, №855) не устанавливалась
/// месяцами: три подозреваемых ведут к разной работе, а различает их ровно
/// номер сигнала.
///
/// На Windows `code()` всегда `Some`, поэтому там возвращается пустая строка и
/// вид сообщения не меняется.
#[cfg(target_os = "windows")]
pub fn signal_note(_status: &ExitStatus) -> String {
    String::new()
}
#[cfg(not(target_os = "windows"))]
pub fn signal_note(status: &ExitStatus) -> String {
    use std::os::unix::process::ExitStatusExt;
    match status.signal() {
        None => String::new(),
        Some(n) => {
            // Только те, что реально встречаются у нас; остальные — числом.
            let name = match n {
                2 => "SIGINT",
                4 => "SIGILL",
                6 => "SIGABRT",
                8 => "SIGFPE",
                9 => "SIGKILL",
                11 => "SIGSEGV",
                13 => "SIGPIPE",
                15 => "SIGTERM",
                24 => "SIGXCPU",
                _ => "",
            };
            if name.is_empty() {
                format!(", убит сигналом {}", n)
            } else {
                format!(", убит сигналом {}({})", name, n)
            }
        }
    }
}

/// Капчуренный output после run с timeout. Заменяет `Output` из
/// `Command::output()` — там нет варианта «убит по таймауту».
pub struct CapturedOutput {
    pub status: Option<ExitStatus>, // None = timeout
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub elapsed: Duration,
}

pub fn run_with_timeout(mut cmd: Command, timeout: Duration) -> std::io::Result<CapturedOutput> {
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    let start = Instant::now();
    let mut child = cmd.spawn()?;

    // Drain stdout/stderr в фоновых потоках, чтобы не deadlock'нуть
    // на полном pipe-buffer'е (Windows ~4 KB, Linux ~64 KB).
    // Plan 26 Ф.15: explicit error если pipe internal-invariant нарушен
    // вместо panic. `Stdio::piped()` гарантирует Some(...), но defensive.
    let stdout = match child.stdout.take() {
        Some(s) => s,
        None => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "child stdout pipe missing",
            ))
        }
    };
    let stderr = match child.stderr.take() {
        Some(s) => s,
        None => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "child stderr pipe missing",
            ))
        }
    };
    // Plan 26 Ф.15: read buffer cap. Тест, печатающий 100 MB stdout
    // (бесконечный print-loop), не должен OOM'нуть runner. Cap = 4 MB —
    // больше чем хватит для real test output, меньше чем разумный stress.
    // Plan 26 Ф.16 #9: при переполнении добавляем truncation marker —
    // silent truncate скрывал бы важные ошибки в конце stdout.
    const READ_CAP: u64 = 4 * 1024 * 1024;
    const TRUNC_MARKER: &[u8] = b"\n... (output truncated at 4 MB)\n";
    let stdout_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let mut s = std::io::Read::take(stdout, READ_CAP);
        let _ = std::io::Read::read_to_end(&mut s, &mut buf);
        if buf.len() as u64 == READ_CAP {
            buf.extend_from_slice(TRUNC_MARKER);
        }
        buf
    });
    let stderr_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let mut s = std::io::Read::take(stderr, READ_CAP);
        let _ = std::io::Read::read_to_end(&mut s, &mut buf);
        if buf.len() as u64 == READ_CAP {
            buf.extend_from_slice(TRUNC_MARKER);
        }
        buf
    });

    let status = wait_with_timeout(&mut child, timeout)?;
    // Plan 26 Ф.16 #2: thread join с safety-timeout. После kill child'а
    // pipe должен закрыться → read_to_end вернётся. На Windows
    // TerminateProcess не всегда закрывает pipe handles немедленно;
    // если drain thread висит — лучше потерять часть output'а чем
    // hang'нуть runner. 500ms — generous для real-world Windows close.
    let stdout_bytes = join_with_timeout(stdout_handle, Duration::from_millis(500));
    let stderr_bytes = join_with_timeout(stderr_handle, Duration::from_millis(500));
    Ok(CapturedOutput {
        status,
        stdout: stdout_bytes,
        stderr: stderr_bytes,
        elapsed: start.elapsed(),
    })
}
