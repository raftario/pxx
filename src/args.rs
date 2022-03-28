use crate::proxy::Endpoint as ProxyEndpoint;

use std::{borrow::Cow, net::ToSocketAddrs, str::FromStr};

use clap::{AppSettings::DeriveDisplayOrder, Parser};

#[cfg(unix)]
fn shell() -> &'static str {
    use once_cell::sync::Lazy;
    static SHELL: Lazy<String> = Lazy::new(|| std::env::var("SHELL").unwrap());
    SHELL.as_str()
}
#[cfg(windows)]
fn shell() -> &'static str {
    "powershell.exe"
}

#[derive(Parser, Debug)]
#[clap(author, version, setting(DeriveDisplayOrder))]
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
/// Example: `pxx -p "[::]:8080<>localhost:3000" -p "unix:./pg.sock<>localhost:5432" "npm start" "docker compose up"`
pub struct Args {
    /// Proxy directives
    ///
    /// Directives are specified `<SOURCE><><DESTINATION>`,
    /// where connections to `<SOURCE>` will be proxied to `<DESTINATION>`.
    /// Both sides are specified as `[<TYPE>:]<ADDRESS>`,
    /// where type can be `tcp`, `unix` or `pipe`.
    /// If `<TYPE>` is omitted, `tcp` is assumed.
    /// `<ADDRESS>` should be specified as `<HOST>:<PORT>` for `tcp`,
    /// as a valid file path for `unix` and as a valid pipe name for `pipe`.
    ///
    /// Example: `[::]:80<>localhost:8080`
    /// {n}Example: `tcp:localhost:5432<>unix:/var/run/postgresql/.s.PGSQL.5432`
    /// {n}Example: `tcp:localhost:2375<>pipe:\\.\pipe\docker_engine`
    #[clap(short, long = "proxy")]
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
    /// These arguments will be passed to the shell before the commands themselves.
    /// They do not affect raw commands.
    #[clap(short = 'a', long = "shell-arg", allow_hyphen_values = true)]
    #[cfg_attr(unix, clap(default_value = "-c"))]
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
    /// These commands will be run directly by splitting at whitespace.
    /// The first word will be interpreted as the program and the rest as arguments.
    #[clap(short = 'r', long = "raw")]
    pub raw_commands: Vec<String>,

    /// Commands to run
    ///
    /// These commands will be run in a shell.
    pub commands: Vec<String>,
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
            .split_once("<>")
            .ok_or("Proxy directives must be of the form `<SOURCE><><DESTINATION>`")?;

        let source = source.parse()?;
        let destination = destination.parse()?;

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

        match s.split_once(':') {
            Some(("tcp", endpoint)) => tcp(endpoint),
            #[cfg(unix)]
            Some(("unix", path)) => Ok(Self::Unix(std::path::PathBuf::from(path))),
            #[cfg(windows)]
            Some(("unix", _)) => Err("Unix sockets are not supported on Windows".into()),
            #[cfg(windows)]
            Some(("pipe", path)) => Ok(Self::Pipe(std::ffi::OsString::from(path))),
            #[cfg(unix)]
            Some(("pipe", _)) => Err("Named pipes are not supported on Unix".into()),
            _ => tcp(s),
        }
    }
}
