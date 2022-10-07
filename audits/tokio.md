# [tokio](https://docs.rs/tokio/latest/tokio/)

Audited by: Caleb Stanford
(IN PROGRESS)

Date: 2022-10-07

Tokio is a popular asynchronous library for network applications, of huge
significance as a transitive risk for many other libraries which directly use
it for network-related functionality.
It has 123 dangerous imports, which is the second most flagged by the script
in the top 100 crates (second only to `mio`).

## List of imports (123)

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

### `tokio::io` (4)

```
src/io, async_fd.rs, std::os::unix::io::AsRawFd
src/io, async_fd.rs, std::os::unix::io::RawFd
src/io/bsd, poll_aio.rs, std::os::unix::io::AsRawFd
src/io/bsd, poll_aio.rs, std::os::unix::prelude::RawFd
```

### `tokio::net` (50)

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

### `tokio::process` (27)

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

### `tokio::sync` (1)

```
src/sync/mpsc, chan.rs, std::process
```

## Analysis

<!-- Detailed audit -->

## Security summary

1. Security risks

<!-- Short answer -->

2. Permissions

<!-- Short answer -->

3. Transitive risk

<!-- Short answer -->

4. Automation feasibility

<!-- Feasible/infeasible -->

- Spec:
- Static analysis:
- Dynamic enforcement overhead:
