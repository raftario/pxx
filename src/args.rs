use crate::proxy::Endpoint as ProxyEndpoint;

use std::{borrow::Cow, net::ToSocketAddrs, str::FromStr};

use clap::{AppSettings::DeriveDisplayOrder, Parser};

#[cfg(unix)]
fn shell() -> &'static str {
    use once_cell::sync::Lazy;
    static SHELL: Lazy<Option<String>> = Lazy::new(|| std::env::var("SHELL").ok());
    SHELL.as_ref().map(String::as_str).unwrap_or("/bin/sh")
}
#[cfg(windows)]
fn shell() -> &'static str {
    "PowerShell.exe"
}

#[cfg(unix)]
const SHELL_ARG: &str = "-c";
#[cfg(windows)]
const SHELL_ARG: &str = "-Command";

/// Proxy connections while executing commands
///
/// pxx lets you proxy TCP, Unix, and named pipe connections while executing commands in parallel.
/// It is useful in scenarios where a program listens on a hardcoded address
/// but needs to be accessed from a different one, or to communicate with a Unix socket or named pipe
/// program using TCP and vice versa.
///
/// pxx is also useful for simply executing commands in parallel. It broadcast standard input to
/// all executed commands and can buffer standard output and error if desired.
///
/// For more information, examples, and the source code, see https://github.com/raftario/pxx
#[derive(Parser, Debug)]
#[clap(
    author,
    version,
    arg_required_else_help = true,
    setting(DeriveDisplayOrder)
)]
pub struct Args {
    /// Proxy directives
    ///
    /// Can be specified multiple times
    ///
    /// Directives are specified `<SOURCE>-><DESTINATION>`,
    /// where connections to `<SOURCE>` will be proxied to `<DESTINATION>`.
    /// Both sides are specified as `[<SCHEME>://]<ADDRESS>`,
    /// where type can be `tcp`, `unix` or `pipe`.
    /// If `<SCHEME>` is omitted, `tcp` is assumed.
    /// `<ADDRESS>` should be specified as `<HOST>:<PORT>` for `tcp`,
    /// as a valid file path for `unix` and as a valid pipe name for `pipe`.
    ///
    /// Example: `[::]:80->localhost:8080`
    /// {n}Example: `tcp://localhost:5432->unix:///var/run/postgresql/.s.PGSQL.5432`
    /// {n}Example: `tcp://192.168.0.1:2375->pipe://\\.\pipe\docker_engine`
    #[clap(short = 'p', long = "proxy", name = "PROXY")]
    pub proxies: Vec<ProxyDirective>,

    /// Shell to use
    ///
    /// Commands will be passed to this shell as a single argument,
    /// after any other specified shell arguments.
    /// This does not affect raw commands.
    #[clap(short, long, default_value = shell())]
    pub shell: String,

    /// Arguments passed to the shell
    ///
    /// Can be specified multiple times
    ///
    /// These arguments will be passed to the shell before the commands themselves.
    /// They do not affect raw commands.
    #[clap(
        short = 'a',
        long = "shell-arg",
        name = "ARG",
        default_value = SHELL_ARG,
        allow_hyphen_values = true
    )]
    pub shell_args: Vec<String>,

    /// Buffer output at newlines
    ///
    /// By default, commands inherit standard output and error streams.
    /// When running multiple commands in parallel, this can cause output to be interleaved.
    /// However, this will disable coloured output for most programs.
    #[clap(short, long)]
    pub buffered: bool,

    /// Commands to run without wrapping in a shell
    ///
    /// Can be specified multiple times
    ///
    /// These commands will be run directly by splitting at whitespace.
    /// The first word will be interpreted as the program and the rest as arguments.
    #[clap(short = 'r', long = "raw", name = "COMMAND")]
    pub raw_commands: Vec<String>,

    /// Commands to run
    ///
    /// These commands will be run in a shell.
    pub commands: Vec<String>,

    /// Print verbose information at startup
    ///
    /// This will print the proxies with hostnames resolved
    /// and commands including their shell if they are not raw.
    #[clap(short, long)]
    pub verbose: bool,
}

pub fn parse() -> Args {
    Args::parse()
}

#[derive(Debug)]
pub struct ProxyDirective {
    pub source: ProxyEndpoint,
    pub destination: ProxyEndpoint,
}

impl FromStr for ProxyDirective {
    type Err = Cow<'static, str>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (source, destination) = s
            .split_once("->")
            .ok_or("Proxy directives must be of the form `<SOURCE>-><DESTINATION>`")?;

        let source = source.trim().parse()?;
        let destination = destination.trim().parse()?;

        Ok(Self {
            source,
            destination,
        })
    }
}

impl FromStr for ProxyEndpoint {
    type Err = Cow<'static, str>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let tcp = |endpoint: &str| -> Result<Self, Self::Err> {
            Ok(Self::Tcp(
                endpoint
                    .to_socket_addrs()
                    .map_err(|err| format!("Invalid address `{endpoint}`: {err}"))?
                    .next()
                    .ok_or(format!("Invalid hostname `{endpoint}`: no addresses found"))?,
            ))
        };

        match s.split_once("://") {
            Some(("tcp", endpoint)) => tcp(endpoint),
            #[cfg(unix)]
            Some(("unix", path)) => Ok(Self::Unix(std::path::PathBuf::from(path))),
            #[cfg(windows)]
            Some(("unix", _)) => Err("Unix sockets are not supported on Windows".into()),
            #[cfg(windows)]
            Some(("pipe", path)) => Ok(Self::Pipe(std::ffi::OsString::from(path))),
            #[cfg(unix)]
            Some(("pipe", _)) => Err("Named pipes are not supported on Unix".into()),
            Some((scheme, _)) => Err(format!("Unrecognised scheme `{scheme}`").into()),
            None => tcp(s),
        }
    }
}
