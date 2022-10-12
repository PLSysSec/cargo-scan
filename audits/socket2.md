# [socket2](https://docs.rs/socket2/latest/socket2/)

Audited by: Caleb Stanford

Date: 2022-10-12

The 87th most downloaded crate.
A networking library (that doesn't use tokio!)
Its dependencies are only `libc` and `winapi`.

## List of imports (39)

### lib and sockaddr

```
src, lib.rs, std::net::SocketAddr
src, sockaddr.rs, std::net::SocketAddr
src, sockaddr.rs, std::net::SocketAddrV4
src, sockaddr.rs, std::net::SocketAddrV6
```

### socket and sockref

```
src, socket.rs, std::net::Ipv4Addr
src, socket.rs, std::net::Ipv6Addr
src, socket.rs, std::net::Shutdown
src, socket.rs, std::net::self
src, socket.rs, std::os::unix::io::FromRawFd
src, socket.rs, std::os::unix::io::IntoRawFd
src, socket.rs, std::os::windows::io::FromRawSocket
src, socket.rs, std::os::windows::io::IntoRawSocket
src, sockref.rs, std::os::unix::io::AsRawFd
src, sockref.rs, std::os::unix::io::FromRawFd
src, sockref.rs, std::os::windows::io::AsRawSocket
src, sockref.rs, std::os::windows::io::FromRawSocket
```

Potentially unsafe.

### sys

```
src/sys, unix.rs, std::net::Shutdown
src/sys, unix.rs, std::net::Ipv4Addr
src/sys, unix.rs, std::net::Ipv6Addr
src/sys, unix.rs, std::os::unix::ffi::OsStrExt
src/sys, unix.rs, std::os::unix::io::RawFd
src/sys, unix.rs, std::os::unix::io::AsRawFd
src/sys, unix.rs, std::os::unix::io::FromRawFd
src/sys, unix.rs, std::os::unix::io::IntoRawFd
src/sys, unix.rs, std::os::unix::net::UnixDatagram
src/sys, unix.rs, std::os::unix::net::UnixListener
src/sys, unix.rs, std::os::unix::net::UnixStream
src/sys, unix.rs, std::path::Path
src/sys, unix.rs, libc::ssize_t
src/sys, unix.rs, libc::c_void
src/sys, unix.rs, libc::in6_addr
src/sys, unix.rs, libc::in_addr
src/sys, unix.rs, libc::TCP_KEEPALIVE as KEEPALIVE_TIME
src/sys, unix.rs, libc::TCP_KEEPIDLE as KEEPALIVE_TIME
src/sys, windows.rs, std::net::Ipv4Addr
src/sys, windows.rs, std::net::Ipv6Addr
src/sys, windows.rs, std::net::Shutdown
src/sys, windows.rs, std::net::self
src/sys, windows.rs, std::os::windows::prelude::*
```

Also potentially dangerous.
The libc imports are just types and constants.

## Analysis

## Security summary

1. Security risks

Network access; a lot of unsafe code and unsafe fns

2. Permissions

Network access

3. Transitive risk

Yes (many of the structs public at top level)

4. Automation feasibility

- Spec: project-dependent
- Static analysis: difficult
- Dynamic enforcement overhead: likely not acceptable
