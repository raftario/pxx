use std::{
    io,
    process::{self, ExitStatus},
    sync::Arc,
};

use args::Args;
use tokio::{select, signal, sync::Mutex, task::JoinHandle};

mod args;
mod command;
mod proxy;

fn main() -> io::Result<()> {
    let mut code = 0;

    {
        let Args {
            commands,
            raw_commands,
            proxies,
            shell,
            shell_args,
            buffered,
        } = args::parse();

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;

        rt.block_on(async move {
            for proxy in proxies {
                tokio::spawn(proxy::proxy(proxy.source, proxy.destination));
            }

            let buffers = if buffered {
                Arc::new(Some((
                    Mutex::new(tokio::io::stdout()),
                    Mutex::new(tokio::io::stderr()),
                )))
            } else {
                Arc::new(None)
            };

            let mut tasks = Vec::with_capacity(commands.len() + raw_commands.len());
            let mut stdins = Vec::with_capacity(tasks.len() + raw_commands.len());

            let execs = Iterator::chain(
                commands
                    .into_iter()
                    .map(|cmd| command::shell(&shell, &shell_args, cmd)),
                raw_commands.into_iter().map(command::raw),
            )
            .map(|cmd| command::exec(cmd, buffers.clone()));

            for exec in execs {
                let (task, stdin) = exec?;
                tasks.push(task);
                stdins.push(stdin);
            }
            tokio::spawn(command::broadcast(stdins));

            select! {
                biased;

                res = wait(&mut tasks, &mut code) => res,
                res = signal::ctrl_c() => {
                    res?;
                    wait(&mut tasks, &mut code).await
                },
            }
        })?;

        // required because of stdin broadcasting which cannot be cancelled
        rt.shutdown_background();
    }

    process::exit(code);
}

async fn wait(
    tasks: &mut Vec<JoinHandle<io::Result<ExitStatus>>>,
    code: &mut i32,
) -> io::Result<()> {
    while let Some(task) = tasks.last_mut() {
        let exit = task.await??;
        if !exit.success() && *code == 0 {
            *code = exit.code().unwrap_or(1);
        }
        tasks.pop();
    }
    Ok(())
}
