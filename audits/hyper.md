# [hyper](https://docs.rs/hyper/latest/hyper/index.html)

Audited by: Caleb Stanford

Date: 2022-10-11

75th most downloaded crate.
Heavily uses `tokio` -- prior to including non-std imports,
didn't have that many dangerous imports.

## List of imports (56)

### upgrade, common, and service

```
src, upgrade.rs, tokio::io::AsyncRead
src, upgrade.rs, tokio::io::AsyncWrite
src, upgrade.rs, tokio::io::ReadBuf
src/common/io, rewind.rs, tokio::io::AsyncRead
src/common/io, rewind.rs, tokio::io::AsyncWrite
src/common/io, rewind.rs, tokio::io::ReadBuf
src/service, make.rs, tokio::io::AsyncRead
src/service, make.rs, tokio::io::AsyncWrite
```

These are all relatively safe imports. `common` is private.

### client

```
src/client, conn.rs, tokio::io::AsyncRead
src/client, conn.rs, tokio::io::AsyncWrite
src/client, tests.rs, tokio::net::TcpStream
src/client/connect, dns.rs, std::net::Ipv4Addr
src/client/connect, dns.rs, std::net::Ipv6Addr
src/client/connect, dns.rs, std::net::SocketAddr
src/client/connect, dns.rs, std::net::SocketAddrV4
src/client/connect, dns.rs, std::net::SocketAddrV6
src/client/connect, dns.rs, std::net::ToSocketAddrs
src/client/connect, http.rs, std::net::IpAddr
src/client/connect, http.rs, std::net::Ipv4Addr
src/client/connect, http.rs, std::net::Ipv6Addr
src/client/connect, http.rs, std::net::SocketAddr
src/client/connect, http.rs, tokio::net::TcpSocket
src/client/connect, http.rs, tokio::net::TcpStream
```

Network access.
The important module is `connect` -- particularly `http.rs`.
(I think `dns.rs` is not doing anything unsafe.)
There are two unsafe blocks for working with raw file descriptors,
and an interesting comment about how these might be ideally
avoided using Tokio in the future.
The implementation does use Tokio for a couple of other things.

Note that `client` re-exports `connect::HttpConnector`
so itself has potential network access.

## ffi

```
src/ffi, body.rs, libc::c_int
src/ffi, body.rs, libc::size_t
src/ffi, client.rs, libc::c_int
src/ffi, error.rs, libc::size_t
src/ffi, http_types.rs, libc::c_int
src/ffi, http_types.rs, libc::size_t
src/ffi, io.rs, libc::size_t
src/ffi, io.rs, tokio::io::AsyncRead
src/ffi, io.rs, tokio::io::AsyncWrite
src/ffi, task.rs, libc::c_int
```

According to the documentation this is actually an API for
using C from Rust, and unstable when used in Rust directly --
so this is safe.

## proto

```
src/proto/h1, conn.rs, tokio::io::AsyncRead
src/proto/h1, conn.rs, tokio::io::AsyncWrite
src/proto/h1, dispatch.rs, tokio::io::AsyncRead
src/proto/h1, dispatch.rs, tokio::io::AsyncWrite
src/proto/h1, io.rs, tokio::io::AsyncRead
src/proto/h1, io.rs, tokio::io::AsyncWrite
src/proto/h1, io.rs, tokio::io::ReadBuf
src/proto/h2, client.rs, tokio::io::AsyncRead
src/proto/h2, client.rs, tokio::io::AsyncWrite
src/proto/h2, mod.rs, tokio::io::AsyncRead
src/proto/h2, mod.rs, tokio::io::AsyncWrite
src/proto/h2, mod.rs, tokio::io::ReadBuf
src/proto/h2, server.rs, tokio::io::AsyncRead
src/proto/h2, server.rs, tokio::io::AsyncWrite
```

Internal module.
Didn't check what else uses it.

### server

```
src/server, server.rs, std::net::SocketAddr
src/server, server.rs, std::net::TcpListener as StdTcpListener
src/server, server.rs, tokio::io::AsyncRead
src/server, server.rs, tokio::io::AsyncWrite
src/server, shutdown.rs, tokio::io::AsyncRead
src/server, shutdown.rs, tokio::io::AsyncWrite
src/server, tcp.rs, std::net::SocketAddr
src/server, tcp.rs, std::net::TcpListener as StdTcpListener
src/server, tcp.rs, tokio::net::TcpListener
```

Network access.

## Analysis

## Security summary

1. Security risks

The unsafe file descriptor code

2. Permissions

Network access

3. Transitive risk

Yes, as with tokio etc:
`hyper::client` and `hyper::server`

4. Automation feasibility

- Spec: project-dependent
- Static analysis: difficult
- Dynamic enforcement overhead: likely unacceptable
