# [tokio](https://docs.rs/tokio/latest/tokio/)

Audited by: Caleb Stanford

Date: 2022-10-09

Tokio is a popular asynchronous library for network applications.
It is a significant transitive risk for many other libraries which directly
use it for network-related functionality and for its async programming
runtime.

Tokio relies on `mio` for some of its implementation details (4 imports).
The dependency chain is:
```
tokio -> mio -> libc -> std
```
but Tokio also uses `libc` directly (3 imports).

Tokio has 130 dangerous imports (123 in the standard library),
which is the second most flagged by the script in the top 100
(in fact, even the top 1000) crates, second only to `mio`.

## List of imports (130) and detailed audit

### `tokio::fs` (41)

```
src/fs, canonicalize.rs, std::path::Path
src/fs, canonicalize.rs, std::path::PathBuf
src/fs, copy.rs, std::path::Path
src/fs, create_dir.rs, std::path::Path
src/fs, create_dir_all.rs, std::path::Path
src/fs, dir_builder.rs, std::path::Path
src/fs, file.rs, std::fs::Metadata
src/fs, file.rs, std::fs::Permissions
src/fs, file.rs, std::path::Path
src/fs, file.rs, std::fs::File as StdFile
src/fs, hard_link.rs, std::path::Path
src/fs, metadata.rs, std::fs::Metadata
src/fs, metadata.rs, std::path::Path
src/fs, mocks.rs, std::fs::Metadata
src/fs, mocks.rs, std::fs::Permissions
src/fs, mocks.rs, std::path::PathBuf
src/fs, open_options.rs, std::path::Path
src/fs, open_options.rs, std::fs::OpenOptions as StdOpenOptions
src/fs, read.rs, std::path::Path
src/fs, read_dir.rs, std::fs::FileType
src/fs, read_dir.rs, std::fs::Metadata
src/fs, read_dir.rs, std::path::Path
src/fs, read_dir.rs, std::path::PathBuf
src/fs, read_link.rs, std::path::Path
src/fs, read_link.rs, std::path::PathBuf
src/fs, read_to_string.rs, std::path::Path
src/fs, remove_dir.rs, std::path::Path
src/fs, remove_dir_all.rs, std::path::Path
src/fs, remove_file.rs, std::path::Path
src/fs, rename.rs, std::path::Path
src/fs, set_permissions.rs, std::fs::Permissions
src/fs, set_permissions.rs, std::path::Path
src/fs, symlink.rs, std::path::Path
src/fs, symlink_dir.rs, std::path::Path
src/fs, symlink_file.rs, std::path::Path
src/fs, symlink_metadata.rs, std::fs::Metadata
src/fs, symlink_metadata.rs, std::path::Path
src/fs, write.rs, std::path::Path
src/fs/open_options, mock_open_options.rs, std::os::unix::fs::OpenOptionsExt
src/fs/open_options, mock_open_options.rs, std::os::windows::fs::OpenOptionsExt
src/fs/open_options, mock_open_options.rs, std::path::Path
```

These modules port all of the standard library's file system capabilities to the
async context.
The entire module is optional (crate feature `fs`), but contains all of the
relevant file system operations.
In fact, it would appear that most of the operations are thin wrappers around
std using a function called `asyncify` which spawns a blocking thread for the
file system operation.
Of particular security interest is the `set_permissions` submodule which
calls `std::fs::set_permissions`.

### `tokio::io` (5)

```
src/io, async_fd.rs, std::os::unix::io::AsRawFd
src/io, async_fd.rs, std::os::unix::io::RawFd
src/io/bsd, poll_aio.rs, std::os::unix::io::AsRawFd
src/io/bsd, poll_aio.rs, std::os::unix::prelude::RawFd
```

Mio import:
```
src/io, async_fd.rs, mio::unix::SourceFd
```

This module implements asynchronous IO, relying on `mio` (Unix-only)
and on file descriptors in the standard library.
The examples suggest that it can only be used with `tokio::fs` to actually
access the file system. This is also supported by the fact that all of
the above imports are Unix-only.

### `tokio::net` (55)

```
src/net, addr.rs, std::net::IpAddr
src/net, addr.rs, std::net::Ipv4Addr
src/net, addr.rs, std::net::Ipv6Addr
src/net, addr.rs, std::net::SocketAddr
src/net, addr.rs, std::net::SocketAddrV4
src/net, addr.rs, std::net::SocketAddrV6
src/net, udp.rs, std::net::Ipv4Addr
src/net, udp.rs, std::net::Ipv6Addr
src/net, udp.rs, std::net::SocketAddr
src/net, udp.rs, std::net::self
src/net/tcp, listener.rs, std::net::SocketAddr
src/net/tcp, listener.rs, std::net::self
src/net/tcp, socket.rs, std::net::SocketAddr
src/net/tcp, socket.rs, std::os::unix::io::AsRawFd
src/net/tcp, socket.rs, std::os::unix::io::FromRawFd
src/net/tcp, socket.rs, std::os::unix::io::IntoRawFd
src/net/tcp, socket.rs, std::os::unix::io::RawFd
src/net/tcp, socket.rs, std::os::windows::io::AsRawSocket
src/net/tcp, socket.rs, std::os::windows::io::FromRawSocket
src/net/tcp, socket.rs, std::os::windows::io::IntoRawSocket
src/net/tcp, socket.rs, std::os::windows::io::RawSocket
src/net/tcp, split.rs, std::net::Shutdown
src/net/tcp, split.rs, std::net::SocketAddr
src/net/tcp, split_owned.rs, std::net::Shutdown
src/net/tcp, split_owned.rs, std::net::SocketAddr
src/net/tcp, stream.rs, std::net::Shutdown
src/net/tcp, stream.rs, std::net::SocketAddr
src/net/unix, listener.rs, std::os::unix::io::AsRawFd
src/net/unix, listener.rs, std::os::unix::io::FromRawFd
src/net/unix, listener.rs, std::os::unix::io::IntoRawFd
src/net/unix, listener.rs, std::os::unix::io::RawFd
src/net/unix, listener.rs, std::os::unix::net
src/net/unix, listener.rs, std::path::Path
src/net/unix, socketaddr.rs, std::path::Path
src/net/unix, split.rs, std::net::Shutdown
src/net/unix, split_owned.rs, std::net::Shutdown
src/net/unix, stream.rs, std::net::Shutdown
src/net/unix, stream.rs, std::os::unix::io::AsRawFd
src/net/unix, stream.rs, std::os::unix::io::FromRawFd
src/net/unix, stream.rs, std::os::unix::io::IntoRawFd
src/net/unix, stream.rs, std::os::unix::io::RawFd
src/net/unix, stream.rs, std::os::unix::net
src/net/unix, stream.rs, std::path::Path
src/net/unix/datagram, socket.rs, std::net::Shutdown
src/net/unix/datagram, socket.rs, std::os::unix::io::AsRawFd
src/net/unix/datagram, socket.rs, std::os::unix::io::FromRawFd
src/net/unix/datagram, socket.rs, std::os::unix::io::IntoRawFd
src/net/unix/datagram, socket.rs, std::os::unix::io::RawFd
src/net/unix/datagram, socket.rs, std::os::unix::net
src/net/unix/datagram, socket.rs, std::path::Path
```

Mio imports:
```
src/signal, unix.rs, mio::net::UnixStream
src/signal/unix, driver.rs, mio::net::UnixStream
```

Libc imports:
```
src/net/unix, ucred.rs, libc::gid_t
src/net/unix, ucred.rs, libc::pid_t
src/net/unix, ucred.rs, libc::uid_t
```

I took a look at `udp`: calls into `mio::net` for the underlying network socket bindings. It also interfaces with `net::UdpSocket`.
It's a bit surprising that `tcp` doesn't also use `mio`.
Why does `mio::net::TcpStream` never get imported by Tokio anywhere?

There is also `unix` again with some Unix-specific stuff;
note the `libc` imports are just types.

Overall, this module imports dangerous wrappers around network functions.
The underlying `libc` and `mio` imports are likely not transitive risks,
beyond the risk that is already there from importing the `tokio` methods.

### `tokio::process` (28)

```
src/process, mod.rs, std::os::unix::process::CommandExt
src/process, mod.rs, std::os::windows::io::AsRawHandle
src/process, mod.rs, std::os::windows::io::RawHandle
src/process, mod.rs, std::os::windows::process::CommandExt
src/process, mod.rs, std::path::Path
src/process, mod.rs, std::process::Command as StdCommand
src/process, mod.rs, std::process::ExitStatus
src/process, mod.rs, std::process::Output
src/process, mod.rs, std::process::Stdio
src/process, windows.rs, std::fs::File as StdFile
src/process, windows.rs, std::os::windows::prelude::AsRawHandle
src/process, windows.rs, std::os::windows::prelude::IntoRawHandle
src/process, windows.rs, std::os::windows::prelude::RawHandle
src/process, windows.rs, std::process::Stdio
src/process, windows.rs, std::process::Child as StdChild
src/process, windows.rs, std::process::Command as StdCommand
src/process, windows.rs, std::process::ExitStatus
src/process/unix, mod.rs, std::fs::File
src/process/unix, mod.rs, std::os::unix::io::AsRawFd
src/process/unix, mod.rs, std::os::unix::io::FromRawFd
src/process/unix, mod.rs, std::os::unix::io::IntoRawFd
src/process/unix, mod.rs, std::os::unix::io::RawFd
src/process/unix, mod.rs, std::process::Child as StdChild
src/process/unix, mod.rs, std::process::ExitStatus
src/process/unix, mod.rs, std::process::Stdio
src/process/unix, orphan.rs, std::process::ExitStatus
src/process/unix, reap.rs, std::process::ExitStatus
```

Mio import:
```
src/process/unix, mod.rs, mio::unix::SourceFd
```

Infrastructure to handle child processes.
There's a lot going on here, but we don't care about structs
like ExitStatus and Stdio. The imports we care about are
- the raw file descriptor handling

  As far as I can tell, this is not actually used for files, but for pipes
  and for Stdio.
  So it should be safe.

- process handling:
  spawning children with Command::spawn()
  passing/capturing stdio for child processes
  signal sending (.kill() etc.) and handling

  The main interface, Command, is a shallow around std::Command.
  Thus any uses of tokio::process::Command should be flagged just like
  std::process::Command.

### `tokio::sync` (1)

```
src/sync/mpsc, chan.rs, std::process
```

Used only for `process::abort()`.
I'm not sure the exact conditions for the abort,
but probably the tokio authors would have used `panic!` or
`assert!` if this was unreachable.
The exact specifications on how/why code can panic or abort
need to be decided by the client.
It is likely this is not a risk.

### Other transitive dependencies

- `tokio::runtime::driver`

  The most dangerous module audited above is `crate::process` (`/src/process`)
  which is in turn used by `crate::runtime::driver`.
  Some tokio code is difficult to audit properly due to the use of macros
  annotating almost every single code block; I think these are just cfg related
  things though. The use of `crate::process` appears to be not direct, only
  triggered if an actual process is spawned on the runtime. The runtime API
  is mostly based on using `tokio::spawn` primitives and futures.

## Security summary

1. Security risks

Tokio is a security risk for:
(1) network access
(2) file system access (if `fs` is included)
(3) process management (if `process` is included), including running
arbitrary commands.

2. Permissions

Needs access for all of the above, depending on what the crate is used
for and the specific functions called.
Also calls things like `libc` and `num_cpus` internally which require
their own permissions.

3. Transitive risk

`tokio` is a transitive risk. The imports to `mio` and `libc` are wrapped
by `tokio` primitives and are likely not a transitive risk through `tokio`.

4. Automation feasibility

(Same as mio)

- Spec: project-dependent; difficult
- Static analysis: difficult
- Dynamic enforcement overhead: probably unacceptable
