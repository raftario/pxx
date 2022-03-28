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
    process::exit({
        let args = args::parse();
        if !args.verbose {
            print(&args);
        }

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let code = rt.block_on(future(args))?;

        // required because of stdin broadcasting which cannot be cancelled
        rt.shutdown_background();
        code
    })
}

fn print(args: &Args) {
    for proxy in &args.proxies {
        eprintln!(
            "Proxying connections from `{}` to `{}`",
            proxy.source, proxy.destination
        );
    }
    if !args.proxies.is_empty() {
        eprintln!();
    }

    let shell = match args.shell_args.join(" ") {
        a if !a.is_empty() => format!("{} {a}", args.shell),
        _ => args.shell.clone(),
    };
    for command in &args.commands {
        eprintln!("Spawning `{shell} {command}`",);
    }
    for raw_command in &args.raw_commands {
        eprintln!("Spawning `{raw_command}`");
    }
    if args.commands.len() + args.raw_commands.len() > 0 {
        eprintln!();
    }
}

async fn future(args: Args) -> io::Result<i32> {
    let Args {
        commands,
        raw_commands,
        proxies,
        shell,
        shell_args,
        buffered,
        ..
    } = args;

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
    .map(|cmd| command::spawn(cmd, buffers.clone()));

    for exec in execs {
        let (future, stdin) = exec?;
        tasks.push(tokio::spawn(future));
        stdins.push(stdin);
    }
    tokio::spawn(command::broadcast(stdins));

    let mut code = 0;
    select! {
        biased;

        res = wait(&mut tasks, &mut code) => res,
        res = signal::ctrl_c() => {
            res?;
            wait(&mut tasks, &mut code).await
        },
    }
}

async fn wait(
    tasks: &mut Vec<JoinHandle<io::Result<ExitStatus>>>,
    code: &mut i32,
) -> io::Result<i32> {
    while let Some(task) = tasks.last_mut() {
        let exit = task.await??;
        if !exit.success() && *code == 0 {
            *code = exit.code().unwrap_or(1);
        }
        tasks.pop();
    }
    Ok(*code)
}
