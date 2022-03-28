use std::{
    ffi::OsStr,
    future::Future,
    io::{self},
    process::{ExitStatus, Stdio},
    sync::Arc,
};

use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, Stderr, Stdout},
    process::{Child, ChildStdin, Command},
    select,
    sync::Mutex,
};

pub fn shell(
    shell: impl AsRef<OsStr>,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    cmd: impl AsRef<OsStr>,
) -> Command {
    let mut command = Command::new(shell);
    command.args(args).arg(cmd).stdin(Stdio::piped());
    command
}

pub fn raw(cmd: impl AsRef<str>) -> Command {
    let mut iter = cmd.as_ref().split_whitespace();
    let cmd = iter.next().unwrap();

    let mut command = Command::new(cmd);
    command.args(iter).stdin(Stdio::null());

    command
}

pub fn spawn(
    mut command: Command,
    buffers: Arc<Option<(Mutex<Stdout>, Mutex<Stderr>)>>,
) -> io::Result<(impl Future<Output = io::Result<ExitStatus>>, ChildStdin)> {
    let out_err = if buffers.is_some() {
        Stdio::piped
    } else {
        Stdio::inherit
    };
    command.stdout(out_err()).stderr(out_err());

    let mut child = command.spawn()?;
    let stdin = child.stdin.take().unwrap();
    let future = exec(child, buffers);

    Ok((future, stdin))
}

pub async fn broadcast(mut stdins: Vec<ChildStdin>) -> io::Result<()> {
    let mut buf = vec![0; 8 * 1024];
    let mut stdin = tokio::io::stdin();

    loop {
        let n = stdin.read(&mut buf).await?;
        if n == 0 {
            break Ok(());
        }

        for stdin in &mut stdins {
            stdin.write_all(&buf[..n]).await?;
        }
    }
}

async fn exec(
    mut child: Child,
    buffers: Arc<Option<(Mutex<Stdout>, Mutex<Stderr>)>>,
) -> io::Result<ExitStatus> {
    match &*buffers {
        None => child.wait().await,

        Some((out, err)) => {
            let mut child_out = BufReader::new(child.stdout.take().unwrap());
            let mut child_err = BufReader::new(child.stderr.take().unwrap());

            let mut out_buf = Vec::new();
            let mut err_buf = Vec::new();

            let mut out_done = false;
            let mut err_done = false;

            loop {
                select! {
                    biased;

                    res = child_out.read_until(b'\n', &mut out_buf), if !out_done => {
                        let n = res?;
                        if n == 0 {
                            out_done = true;
                            continue;
                        }

                        let mut lock = out.lock().await;
                        lock.write_all(&out_buf).await?;
                        out_buf.clear();
                    },
                    res = child_err.read_until(b'\n', &mut err_buf), if !err_done => {
                        let n = res?;
                        if n == 0 {
                            err_done = true;
                            continue;
                        }

                        let mut lock = err.lock().await;
                        lock.write_all(&err_buf).await?;
                        err_buf.clear();
                    },

                    res = child.wait() => return res,
                }
            }
        }
    }
}
