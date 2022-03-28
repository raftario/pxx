use std::{
    fmt::{self, Display},
    io,
    net::SocketAddr,
    pin::Pin,
    task::{Context, Poll},
};

use pin_project::pin_project;
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::{TcpListener, TcpStream},
};

#[derive(Debug, Clone)]
pub enum Endpoint {
    Tcp(SocketAddr),
    #[cfg(unix)]
    Unix(std::path::PathBuf),
    #[cfg(windows)]
    Pipe(std::ffi::OsString),
}

pub async fn proxy(source: Endpoint, destination: Endpoint) -> io::Result<()> {
    let mut listener = Listener::bind(source).await?;

    loop {
        let mut source = listener.accept().await?;
        let destination = destination.clone();

        tokio::spawn(async move {
            let mut destination = Stream::connect(destination).await.unwrap();
            tokio::io::copy_bidirectional(&mut source, &mut destination)
                .await
                .unwrap()
        });
    }
}

#[pin_project(project = StreamProjection)]
enum Stream {
    Tcp(#[pin] TcpStream),
    #[cfg(unix)]
    Unix(#[pin] tokio::net::UnixStream),
    #[cfg(windows)]
    PipeClient(#[pin] tokio::net::windows::named_pipe::NamedPipeClient),
    #[cfg(windows)]
    PipeServer(#[pin] tokio::net::windows::named_pipe::NamedPipeServer),
}

enum Listener {
    Tcp(TcpListener),
    #[cfg(unix)]
    Unix(tokio::net::UnixListener),
    #[cfg(windows)]
    Pipe {
        server: tokio::net::windows::named_pipe::NamedPipeServer,
        name: std::ffi::OsString,
    },
}

impl Stream {
    async fn connect(endpoint: Endpoint) -> io::Result<Self> {
        Ok(match endpoint {
            Endpoint::Tcp(addr) => Self::Tcp(TcpStream::connect(addr).await?),
            #[cfg(unix)]
            Endpoint::Unix(path) => Self::Unix(tokio::net::UnixStream::connect(path).await?),
            #[cfg(windows)]
            Endpoint::Pipe(name) => {
                let mut wait = 0;
                loop {
                    match tokio::net::windows::named_pipe::ClientOptions::new().open(&name) {
                        Ok(client) => break Self::PipeClient(client),
                        Err(err) if err.raw_os_error() == Some(231) => {
                            if wait == 0 {
                                tokio::task::yield_now().await;
                                wait = 1;
                            } else {
                                tokio::time::sleep(tokio::time::Duration::from_millis(wait)).await;
                                wait *= 2;
                            }
                        }
                        Err(err) => return Err(err),
                    }
                }
            }
        })
    }
}

impl Listener {
    async fn bind(endpoint: Endpoint) -> io::Result<Self> {
        Ok(match endpoint {
            Endpoint::Tcp(address) => Self::Tcp(TcpListener::bind(address).await?),
            #[cfg(unix)]
            Endpoint::Unix(path) => Self::Unix(tokio::net::UnixListener::bind(path)?),
            #[cfg(windows)]
            Endpoint::Pipe(path) => Self::Pipe {
                server: tokio::net::windows::named_pipe::ServerOptions::new()
                    .first_pipe_instance(true)
                    .create(&path)?,
                name: path,
            },
        })
    }

    async fn accept(&mut self) -> io::Result<Stream> {
        Ok(match self {
            Self::Tcp(listener) => Stream::Tcp(listener.accept().await?.0),
            #[cfg(unix)]
            Self::Unix(listener) => Stream::Unix(listener.accept().await?.0),
            #[cfg(windows)]
            Self::Pipe { server, name: path } => {
                server.connect().await?;
                let stream = std::mem::replace(
                    server,
                    tokio::net::windows::named_pipe::ServerOptions::new().create(path)?,
                );
                Stream::PipeServer(stream)
            }
        })
    }
}

impl Display for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Endpoint::Tcp(addr) => write!(f, "tcp:{addr}"),
            #[cfg(unix)]
            Endpoint::Unix(path) => write!(f, "unix:{}", path.display()),
            #[cfg(windows)]
            Endpoint::Pipe(name) => write!(f, "pipe:{}", name.to_string_lossy()),
        }
    }
}

macro_rules! project_stream {
    ($self:expr; $inner:ident => $e:expr) => {
        match $self.project() {
            StreamProjection::Tcp($inner) => $e,
            #[cfg(unix)]
            StreamProjection::Unix($inner) => $e,
            #[cfg(windows)]
            StreamProjection::PipeClient($inner) => $e,
            #[cfg(windows)]
            StreamProjection::PipeServer($inner) => $e,
        }
    };
}

impl AsyncRead for Stream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        project_stream!(self; stream => stream.poll_read(cx, buf))
    }
}

impl AsyncWrite for Stream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        project_stream!(self; stream => stream.poll_write(cx, buf))
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        project_stream!(self; stream => stream.poll_flush(cx))
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        project_stream!(self; stream => stream.poll_shutdown(cx))
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<Result<usize, io::Error>> {
        project_stream!(self; stream => stream.poll_write_vectored(cx, bufs))
    }

    fn is_write_vectored(&self) -> bool {
        match self {
            Stream::Tcp(stream) => stream.is_write_vectored(),
            #[cfg(unix)]
            Stream::Unix(stream) => stream.is_write_vectored(),
            #[cfg(windows)]
            Stream::PipeClient(client) => client.is_write_vectored(),
            #[cfg(windows)]
            Stream::PipeServer(server) => server.is_write_vectored(),
        }
    }
}
