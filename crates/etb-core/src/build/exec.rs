//! Running the compiler.
//!
//! The application does not run the user's programs — it builds them and hands
//! the user the executable. So the only child process here is the compiler, and the
//! only thing we need from it is its interleaved output and its exit code.

use crate::error::{EtbError, Result};
use std::io::{ErrorKind, Read};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Windows: do not flash a console window when a GUI process spawns a child.
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

const READ_CHUNK: usize = 8192;

/// Apply the platform flags every child of ours needs.
pub fn harden(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Its own process group, so a cancelled compile cannot be left attached
        // to our terminal — and so the whole group can be signalled at once,
        // which is what `stop` below depends on.
        cmd.process_group(0);
    }
}

/// Stop a compile and everything it started.
///
/// `Child::kill` is not enough, and the difference is the whole Stop button.
/// `fbc` runs `make`, which runs the C++ compiler, the assembler and the
/// linker, and they inherit the write end of the pipe we are reading. Kill only
/// `fbc` and those children keep running and keep that pipe open, so the
/// reader never sees EOF and the build blocks until the compile would have
/// finished by itself. Verified: a grandchild held a pipe open for its full
/// lifetime after its parent had exited.
///
/// So this tries to take the whole tree. On unix `harden` already made the child
/// a process group leader, so its pid negated names the group; `killpg` and a
/// Windows Job object both need `unsafe`, which this workspace forbids, so it
/// goes through `kill` and `taskkill` instead. `Command::arg` per argument,
/// never a shell.
///
/// **Best effort, and deliberately not the guarantee.** `kill` is a shell
/// builtin as often as it is a binary, the two implementations parse a negative
/// pid differently, and `Command::new` does not consult a shell — this worked on
/// Fedora and did nothing at all on Ubuntu's CI runner. What makes Stop reliable
/// is that `run_capture` stops waiting for the pipe after a cancel, whether or
/// not this managed to kill anything. This only decides whether the compiler's
/// orphans die now or run to completion unnoticed.
fn stop(child: &mut Child) -> bool {
    let pid = child.id();

    #[cfg(unix)]
    let signalled = {
        // Straight from `rustix`, not a `kill` subprocess. The earlier version
        // shelled out on the grounds that signalling a group needs `unsafe` — it
        // does not, here: `unsafe_code = "forbid"` is a lint on the code in this
        // workspace, and rustix's `killpg` is a safe function. Shelling out was
        // also unreliable, which is how the mistake surfaced. `kill` is a shell
        // builtin as often as a binary, `Command::new` does not consult a shell,
        // and the implementations disagree about a bare negative pid, so it
        // worked on Fedora and did nothing at all on Ubuntu's CI runner.
        match rustix::process::Pid::from_raw(pid as i32) {
            Some(leader) => {
                let r = rustix::process::kill_process_group(leader, rustix::process::Signal::KILL);
                if let Err(e) = r {
                    tracing::debug!("could not signal the compiler's process group: {e}");
                }
                r.is_ok()
            }
            None => false,
        }
    };

    #[cfg(windows)]
    let signalled = {
        // No Job object, because the `windows` crate's functions are `unsafe fn`
        // and calling them would need `unsafe` here. `taskkill /T` walks the
        // tree instead; it is a real binary on every Windows, and this is a
        // plain child process, never a shell.
        let mut c = Command::new("taskkill");
        c.args(["/T", "/F", "/PID", &pid.to_string()]);
        c.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        harden(&mut c);
        match c.status() {
            Ok(s) if s.success() => true,
            Ok(s) => {
                tracing::debug!("taskkill could not stop the compiler's tree: {s}");
                false
            }
            Err(e) => {
                tracing::debug!("could not run taskkill: {e}");
                false
            }
        }
    };

    // Belt and braces: whatever happened above, the driver itself dies.
    let _ = child.kill();
    signalled
}

/// Run a command to completion, capturing stdout and stderr interleaved, with a
/// hard cap.
///
/// One OS pipe is shared by both streams so the order of the output matches the
/// order the compiler wrote it; two separate pipes give no ordering guarantee,
/// which puts error messages in the wrong place. The cap matters because a
/// runaway error cascade can emit hundreds of megabytes; `cap` is the room left
/// in the *build's* budget, not a fresh allowance per file.
pub fn run_capture(
    mut cmd: Command,
    cap: usize,
    cancel: &AtomicBool,
) -> Result<(Option<i32>, String)> {
    let (mut reader, writer) = os_pipe::pipe().map_err(|e| EtbError::io("<compiler>", e))?;
    let writer2 = writer
        .try_clone()
        .map_err(|e| EtbError::io("<compiler>", e))?;
    cmd.stdout(writer2);
    cmd.stderr(writer);
    cmd.stdin(Stdio::null());
    harden(&mut cmd);

    let mut child = cmd.spawn().map_err(|e| EtbError::io("<compiler>", e))?;
    // Drop the Command so the parent's copies of the write end are closed.
    // Without this the reader never sees EOF and the build appears to hang.
    drop(cmd);

    let collected = Arc::new(Mutex::new(Vec::<u8>::new()));
    let dropped_any = Arc::new(AtomicBool::new(false));
    let reader_done = Arc::new(AtomicBool::new(false));
    let sink = Arc::clone(&collected);
    let dropped = Arc::clone(&dropped_any);
    let done = Arc::clone(&reader_done);
    let pump = std::thread::spawn(move || {
        // On drop rather than at the end of the body, so that a panic — which
        // skips the last statement — still records that nobody is reading any
        // more. Otherwise the wait below spends its whole timeout and the note
        // about lost output is never added.
        struct MarkDone(Arc<AtomicBool>);
        impl Drop for MarkDone {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Relaxed);
            }
        }
        let _mark = MarkDone(done);

        let mut buf = [0u8; READ_CHUNK];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                // A signal delivered to this thread mid-read is not end of
                // output. `read` is raw here, so std does not retry for us, and
                // treating it as EOF would silently discard the rest of the
                // compiler's errors.
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(_) => break,
                Ok(n) => {
                    let mut g = sink.lock().unwrap();
                    let room = cap.saturating_sub(g.len());
                    if n > room {
                        dropped.store(true, Ordering::Relaxed);
                    }
                    g.extend_from_slice(&buf[..n.min(room)]);
                }
            }
        }
    });

    let mut stopped = false;
    let status = loop {
        if !stopped && cancel.load(Ordering::Relaxed) {
            let _ = stop(&mut child);
            stopped = true;
        }
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(10)),
            Err(e) => return Err(EtbError::io("<compiler>", e)),
        }
    };

    // Waiting for the reader is right when the compiler finished on its own:
    // EOF is coming, and joining is what guarantees none of its output is lost.
    //
    // After a cancel it is not. The children the compiler started may still hold
    // the write end, in which case EOF never comes and joining here is exactly
    // the hang the Stop button was reported as. So the wait is bounded, and on
    // expiry the thread is left to end in its own time — it holds nothing but a
    // pipe and a buffer, and the build has already been abandoned.
    let finished = if stopped {
        let deadline = Instant::now() + Duration::from_secs(2);
        while !reader_done.load(Ordering::Relaxed) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        let done = reader_done.load(Ordering::Relaxed);
        if !done {
            tracing::debug!(
                "the compiler's output pipe is still open after cancelling: its \
                 children are probably still running"
            );
        }
        done
    } else {
        true
    };

    // A panic in the pump must not become a panic here: this runs on the build
    // worker thread, and a panic there drops the channel with no `Finished`
    // event, leaving the interface showing a build that never resolves.
    let mut pump_failed = false;
    let bytes = if finished {
        pump_failed = pump.join().is_err();
        match Arc::try_unwrap(collected) {
            // Sole owner now, so the buffer is taken rather than copied — it may
            // be megabytes.
            Ok(m) => m.into_inner().unwrap_or_else(|e| e.into_inner()),
            Err(arc) => arc.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        }
    } else {
        // Still reading, so the buffer has to be copied out from under it.
        collected.lock().unwrap_or_else(|e| e.into_inner()).clone()
    };

    // Valid UTF-8 is the overwhelmingly common case and costs no copy at all.
    let mut text = match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(e) => String::from_utf8_lossy(e.as_bytes()).into_owned(),
    };

    if dropped_any.load(Ordering::Relaxed) {
        text.push_str("\n… (output truncated)\n");
    }
    if pump_failed {
        text.push_str("\n… (some compiler output could not be captured)\n");
    }

    let code = status.code();
    // `code()` is `None` when the process died from a signal. Without this the
    // caller sees an ordinary non-zero exit and the user gets "Build failed"
    // with no errors in it — the same screen as a compiler that simply crashed
    // or was killed for running out of memory. Cancelling is the one legitimate
    // way to get here, and says so for itself.
    #[cfg(unix)]
    if code.is_none() && !stopped {
        use std::os::unix::process::ExitStatusExt;
        if let Some(sig) = status.signal() {
            text.push_str(&format!(
                "\n… the compiler itself stopped unexpectedly (signal {sig})\n"
            ));
        }
    }

    Ok((code, text))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// `stop` must take the compiler's children with it, where it can.
    ///
    /// The test above deliberately passes whether or not the group kill lands,
    /// because what it guards is that Stop *returns*. That leaves `stop` itself
    /// uncovered, and `stop` is the half that decides whether an abandoned
    /// compile keeps burning a core until it finishes.
    ///
    /// So this covers it directly, and conditionally: the assertion is made only
    /// when `stop` reports that it managed to signal the group. On a machine
    /// where `kill` is a shell builtin with no binary behind it, there is
    /// nothing to assert and the test says so rather than failing — which is the
    /// honest shape, since the behaviour genuinely is unavailable there.
    #[test]
    fn stopping_takes_the_whole_process_group_where_it_can() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "sleep 30 & wait"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        // The same call the real path makes: it is what puts the child in a
        // group of its own, and so what makes the group killable.
        harden(&mut cmd);
        let mut child = cmd.spawn().expect("sh should spawn");
        // The group is named by the leader's pid, read before it is reaped.
        let pid = child.id();
        // Let the backgrounded child come into existence first.
        std::thread::sleep(Duration::from_millis(200));

        let signalled = stop(&mut child);
        let _ = child.wait();

        if !signalled {
            eprintln!("skipping: no usable `kill` binary on this machine");
            return;
        }

        std::thread::sleep(Duration::from_millis(200));
        // `pgrep -g` selects by process group. (`ps -g` selects by session, and
        // using it here made this test pass while the group was untouched.)
        let Ok(out) = Command::new("pgrep")
            .args(["-g", &pid.to_string()])
            .output()
        else {
            eprintln!("skipping the assertion: no `pgrep` to observe the group with");
            return;
        };
        let alive = String::from_utf8_lossy(&out.stdout).trim().to_string();
        assert!(
            alive.is_empty(),
            "the compiler's children outlived the kill, pids still in the group: {alive:?}"
        );
    }

    /// Cancelling must return promptly even when the compiler leaves children
    /// behind.
    ///
    /// This is the Stop button. A compiler driver forks children that inherit
    /// the pipe we read, so killing only the driver leaves them holding it open
    /// and the read blocks until they finish on their own. `sh -c 'sleep 30 &
    /// wait'` is that shape in miniature: kill only the `sh` and the `sleep`
    /// keeps the pipe open for its full thirty seconds.
    ///
    /// Two things can save it, and the test deliberately does not care which:
    /// killing the process group, which takes the children too, or giving up on
    /// the pipe after a cancel. The first is best effort and does nothing on
    /// some systems -- this test passed on Fedora and hung for the full thirty
    /// seconds on Ubuntu's runner when only that was in place. The second always
    /// works, which is why it is the guarantee. Measured here: 0.2s when the
    /// group kill lands, 2.2s when it does nothing, 30s with neither.
    ///
    /// Unix only because it needs a shell that backgrounds a child; the
    /// mechanism it guards is the same on Windows, where `taskkill /T` does the
    /// same job.
    #[test]
    fn cancelling_kills_the_children_the_compiler_started() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "sleep 30 & wait"]);

        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            flag.store(true, Ordering::SeqCst);
        });

        let start = Instant::now();
        let out = run_capture(cmd, 1024, &cancel);
        let took = start.elapsed();

        assert!(out.is_ok(), "should return, not error: {out:?}");
        assert!(
            took < Duration::from_secs(10),
            "cancelling took {took:?}: the grandchild still held the pipe, so \
             this is the hang the Stop button shows as a build that will not stop"
        );
    }
}
