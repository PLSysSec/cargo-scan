# [tokio-util](https://docs.rs/tokio-util/0.7.4/tokio_util/)

Audited by: Caleb Stanford

Date: 2022-10-05

Update: 2022-10-12

Top 100 most downloaded crates.

## List of std imports (3)

```
src/udp, frame.rs, std::net::Ipv4Addr
src/udp, frame.rs, std::net::SocketAddr
src/udp, frame.rs, std::net::SocketAddrV4
```

## List of other imports

```
src, either.rs, tokio::io::AsyncBufRead
src, either.rs, tokio::io::AsyncRead
src, either.rs, tokio::io::AsyncSeek
src, either.rs, tokio::io::AsyncWrite
src, either.rs, tokio::io::ReadBuf
src, either.rs, tokio::io::Result
src/codec, decoder.rs, tokio::io::AsyncRead
src/codec, decoder.rs, tokio::io::AsyncWrite
src/codec, framed.rs, tokio::io::AsyncRead
src/codec, framed.rs, tokio::io::AsyncWrite
src/codec, framed_impl.rs, tokio::io::AsyncRead
src/codec, framed_impl.rs, tokio::io::AsyncWrite
src/codec, framed_read.rs, tokio::io::AsyncRead
src/codec, framed_write.rs, tokio::io::AsyncWrite
src/codec, length_delimited.rs, tokio::io::AsyncRead
src/codec, length_delimited.rs, tokio::io::AsyncWrite
src/io, read_buf.rs, tokio::io::AsyncRead
src/io, reader_stream.rs, tokio::io::AsyncRead
src/io, stream_reader.rs, tokio::io::AsyncBufRead
src/io, stream_reader.rs, tokio::io::AsyncRead
src/io, stream_reader.rs, tokio::io::ReadBuf
src/io, sync_bridge.rs, tokio::io::AsyncRead
src/io, sync_bridge.rs, tokio::io::AsyncReadExt
src/io, sync_bridge.rs, tokio::io::AsyncWrite
src/io, sync_bridge.rs, tokio::io::AsyncWriteExt
src/udp, frame.rs, tokio::io::ReadBuf
src/udp, frame.rs, tokio::net::UdpSocket
src/udp, frame.rs, std::net::Ipv4Addr
src/udp, frame.rs, std::net::SocketAddr
src/udp, frame.rs, std::net::SocketAddrV4
```

Most of these look benign: the one interesting import is `UdpSocket`.

## Analysis

Hard to assess in detail. The std::net uses are benign, but there are a huge
number of tokio imports throughout. I looked at a few of the
tokio uses and didn't notice anything fishy.

## Security summary

1. Security risks

Network access and any risks associated with tokio

2. Permissions

Any permissions transitively needed by tokio

3. Transitive risk

I think `tokio_util::udp` is a transitive risk.
`tokio_util::net` may also be a transitive risk.

4. Automation feasibility

- Spec: unsure
- Static analysis: difficult
- Dynamic enforcement overhead: likely not acceptable
