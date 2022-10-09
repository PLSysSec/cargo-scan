# [mio](https://docs.rs/mio/latest/mio/)

Audited by: Caleb Stanford

Date: 2022-10-09

`mio` is a low-level high-performance I/O library -- developed by the `tokio`
team and predominantly used (as far as I know) by the `tokio` crate.

Both `tokio` and `mio` use `libc` under the hood. So the dependency chain
around all of this system-related code infrastructure (including
network, process, and filesystem accesses) is
```
tokio -> mio -> libc -> std
```

`mio` is the crate with the largest number of unsafe stdlib imports among
the top 1000 crates.
(So not including `libc` and `mio` imports.)
Despite these, it has minimal dependencies: besides `libc`, basically
just `log`, which uses `cfg-if`.
And it is a much smaller crate than `tokio`.

## Analysis of imports (133)

### `io_source` and `poll` (5)

```
src, io_source.rs, std::os::unix::io::AsRawFd
src, io_source.rs, std::os::wasi::io::AsRawFd
src, io_source.rs, std::os::windows::io::AsRawSocket
src, poll.rs, std::os::unix::io::AsRawFd
src, poll.rs, std::os::unix::io::RawFd
```

Both of these are private modules. The imports in `io_source` are traits,
and all of these are for converting things to raw file descriptors.
These imports on their own are probably not dangerous.

### `net` (61)

```
src/net, udp.rs, std::net
src/net, udp.rs, std::net::Ipv4Addr
src/net, udp.rs, std::net::Ipv6Addr
src/net, udp.rs, std::net::SocketAddr
src/net, udp.rs, std::os::unix::io::AsRawFd
src/net, udp.rs, std::os::unix::io::FromRawFd
src/net, udp.rs, std::os::unix::io::IntoRawFd
src/net, udp.rs, std::os::unix::io::RawFd
src/net, udp.rs, std::os::windows::io::AsRawSocket
src/net, udp.rs, std::os::windows::io::FromRawSocket
src/net, udp.rs, std::os::windows::io::IntoRawSocket
src/net, udp.rs, std::os::windows::io::RawSocket
src/net/tcp, listener.rs, std::net::SocketAddr
src/net/tcp, listener.rs, std::net::self
src/net/tcp, listener.rs, std::os::unix::io::AsRawFd
src/net/tcp, listener.rs, std::os::unix::io::FromRawFd
src/net/tcp, listener.rs, std::os::unix::io::IntoRawFd
src/net/tcp, listener.rs, std::os::unix::io::RawFd
src/net/tcp, listener.rs, std::os::wasi::io::AsRawFd
src/net/tcp, listener.rs, std::os::wasi::io::FromRawFd
src/net/tcp, listener.rs, std::os::wasi::io::IntoRawFd
src/net/tcp, listener.rs, std::os::wasi::io::RawFd
src/net/tcp, listener.rs, std::os::windows::io::AsRawSocket
src/net/tcp, listener.rs, std::os::windows::io::FromRawSocket
src/net/tcp, listener.rs, std::os::windows::io::IntoRawSocket
src/net/tcp, listener.rs, std::os::windows::io::RawSocket
src/net/tcp, stream.rs, std::net::Shutdown
src/net/tcp, stream.rs, std::net::SocketAddr
src/net/tcp, stream.rs, std::net::self
src/net/tcp, stream.rs, std::os::unix::io::AsRawFd
src/net/tcp, stream.rs, std::os::unix::io::FromRawFd
src/net/tcp, stream.rs, std::os::unix::io::IntoRawFd
src/net/tcp, stream.rs, std::os::unix::io::RawFd
src/net/tcp, stream.rs, std::os::wasi::io::AsRawFd
src/net/tcp, stream.rs, std::os::wasi::io::FromRawFd
src/net/tcp, stream.rs, std::os::wasi::io::IntoRawFd
src/net/tcp, stream.rs, std::os::wasi::io::RawFd
src/net/tcp, stream.rs, std::os::windows::io::AsRawSocket
src/net/tcp, stream.rs, std::os::windows::io::FromRawSocket
src/net/tcp, stream.rs, std::os::windows::io::IntoRawSocket
src/net/tcp, stream.rs, std::os::windows::io::RawSocket
src/net/uds, datagram.rs, std::net::Shutdown
src/net/uds, datagram.rs, std::os::unix::io::AsRawFd
src/net/uds, datagram.rs, std::os::unix::io::FromRawFd
src/net/uds, datagram.rs, std::os::unix::io::IntoRawFd
src/net/uds, datagram.rs, std::os::unix::io::RawFd
src/net/uds, datagram.rs, std::os::unix::net
src/net/uds, datagram.rs, std::path::Path
src/net/uds, listener.rs, std::os::unix::io::AsRawFd
src/net/uds, listener.rs, std::os::unix::io::FromRawFd
src/net/uds, listener.rs, std::os::unix::io::IntoRawFd
src/net/uds, listener.rs, std::os::unix::io::RawFd
src/net/uds, listener.rs, std::os::unix::net
src/net/uds, listener.rs, std::path::Path
src/net/uds, stream.rs, std::net::Shutdown
src/net/uds, stream.rs, std::os::unix::io::AsRawFd
src/net/uds, stream.rs, std::os::unix::io::FromRawFd
src/net/uds, stream.rs, std::os::unix::io::IntoRawFd
src/net/uds, stream.rs, std::os::unix::io::RawFd
src/net/uds, stream.rs, std::os::unix::net
src/net/uds, stream.rs, std::path::Path
```

This module is available only with `cfg(net)` so can be
turned off. Many of the types and functions in `mio::net`
directly access the network and are wrappers around `std::net`.
The module specifically provides 7 structs: including `SocketAddr`
(just a type), `TcpListener` (used to listen over a network),
`TcpStream` (used for an open communication stream over a network),
`UdpSocket`, and three Unix-specific structs. All of these except
the Unix structs directly mirror types provided in `std::net`,
with technical differences. Everything in `mio::net` is a dangerous
import, with the exception of `SocketAddr`.

## `sys` (67)

```
src/sys/shell, selector.rs, std::os::unix::io::AsRawFd
src/sys/shell, selector.rs, std::os::unix::io::RawFd
src/sys/shell, tcp.rs, std::net::SocketAddr
src/sys/shell, tcp.rs, std::net::self
src/sys/shell, udp.rs, std::net::SocketAddr
src/sys/shell, udp.rs, std::net::self
src/sys/unix, net.rs, std::net::Ipv4Addr
src/sys/unix, net.rs, std::net::Ipv6Addr
src/sys/unix, net.rs, std::net::SocketAddr
src/sys/unix, net.rs, std::net::SocketAddrV4
src/sys/unix, net.rs, std::net::SocketAddrV6
src/sys/unix, pipe.rs, std::fs::File
src/sys/unix, pipe.rs, std::os::unix::io::AsRawFd
src/sys/unix, pipe.rs, std::os::unix::io::FromRawFd
src/sys/unix, pipe.rs, std::os::unix::io::IntoRawFd
src/sys/unix, pipe.rs, std::os::unix::io::RawFd
src/sys/unix, pipe.rs, std::process::ChildStderr
src/sys/unix, pipe.rs, std::process::ChildStdin
src/sys/unix, pipe.rs, std::process::ChildStdout
src/sys/unix, sourcefd.rs, std::os::unix::io::RawFd
src/sys/unix, tcp.rs, std::net::SocketAddr
src/sys/unix, tcp.rs, std::net::self
src/sys/unix, tcp.rs, std::os::unix::io::AsRawFd
src/sys/unix, tcp.rs, std::os::unix::io::FromRawFd
src/sys/unix, udp.rs, std::net::SocketAddr
src/sys/unix, udp.rs, std::net::self
src/sys/unix, udp.rs, std::os::unix::io::AsRawFd
src/sys/unix, udp.rs, std::os::unix::io::FromRawFd
sys/unix/selector, epoll.rs, libc::EPOLLET
sys/unix/selector, epoll.rs, libc::EPOLLIN
sys/unix/selector, epoll.rs, libc::EPOLLOUT
sys/unix/selector, epoll.rs, libc::EPOLLRDHUP
src/sys/unix/selector, epoll.rs, std::os::unix::io::AsRawFd
src/sys/unix/selector, epoll.rs, std::os::unix::io::RawFd
src/sys/unix/selector, kqueue.rs, std::os::unix::io::AsRawFd
src/sys/unix/selector, kqueue.rs, std::os::unix::io::RawFd
src/sys/unix/uds, datagram.rs, std::os::unix::io::AsRawFd
src/sys/unix/uds, datagram.rs, std::os::unix::io::FromRawFd
src/sys/unix/uds, datagram.rs, std::os::unix::net
src/sys/unix/uds, datagram.rs, std::path::Path
src/sys/unix/uds, listener.rs, std::os::unix::io::AsRawFd
src/sys/unix/uds, listener.rs, std::os::unix::io::FromRawFd
src/sys/unix/uds, listener.rs, std::os::unix::net
src/sys/unix/uds, listener.rs, std::path::Path
src/sys/unix/uds, socketaddr.rs, std::os::unix::ffi::OsStrExt
src/sys/unix/uds, socketaddr.rs, std::path::Path
src/sys/unix/uds, stream.rs, std::os::unix::io::AsRawFd
src/sys/unix/uds, stream.rs, std::os::unix::io::FromRawFd
src/sys/unix/uds, stream.rs, std::os::unix::net
src/sys/unix/uds, stream.rs, std::path::Path
src/sys/windows, afd.rs, std::fs::File
src/sys/windows, afd.rs, std::os::windows::io::AsRawHandle
src/sys/windows, handle.rs, std::os::windows::io::RawHandle
src/sys/windows, iocp.rs, std::os::windows::io::*
src/sys/windows, named_pipe.rs, std::os::windows::io::AsRawHandle
src/sys/windows, named_pipe.rs, std::os::windows::io::FromRawHandle
src/sys/windows, named_pipe.rs, std::os::windows::io::RawHandle
src/sys/windows, net.rs, std::net::SocketAddr
src/sys/windows, selector.rs, std::os::windows::io::RawSocket
src/sys/windows, tcp.rs, std::net::SocketAddr
src/sys/windows, tcp.rs, std::net::self
src/sys/windows, tcp.rs, std::os::windows::io::AsRawSocket
src/sys/windows, udp.rs, std::net::SocketAddr
src/sys/windows, udp.rs, std::net::self
src/sys/windows, udp.rs, std::os::windows::io::AsRawSocket
src/sys/windows, udp.rs, std::os::windows::io::FromRawSocket
src/sys/windows, udp.rs, std::os::windows::raw::SOCKET as StdSocket
```

### Others

The crate also provides four other modules.
`event` is used to create a queue of events and process them, but it must
be combined with some kind of net implementation to actually be used;
I don't think it's dangerous on its own.
`features` and `guide` are just for documentation purposes.

Lastly, `unix` contains `unix::pipe`, which wraps Unix's `pipe` system call,
and `unix::SourceFd` for creating sources from any file descriptor
(apparently, not just from a network file descriptor). I think these
could potentially be dangerous, so let's say `mio::unix` is a dangerous
import.

The structs `Interest`, `Poll`, `Registry`, `Token`, and `Waker` are exposed at
the top level.
As with events, at some point when using the registry you have to register
an actual connection into it (like a network listener) so I don't think
these can be used to access the network (or filesystem) on their own.

## Security summary

1. Security risks

Network access; some OS stuff like creating pipes and communicating
between threads and processes; potentially reading from files or other system
stream sources, though it's unclear if the crate offers this functionality
directly, indirectly, or neither.

2. Permissions

Needs network access and system access.

3. Transitive risk

Yes

4. Automation feasibility

- Spec: project-dependent; difficult
- Static analysis: difficult
- Dynamic enforcement overhead: probably unacceptable
