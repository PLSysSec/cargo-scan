# [tokio-util](https://docs.rs/tokio-util/0.7.4/tokio_util/)

Audited by: Caleb Stanford
Date: 2022-10-05

Top 100 most downloaded crates.

## List of imports (3)

```
src/udp, frame.rs, std::net::Ipv4Addr
src/udp, frame.rs, std::net::SocketAddr
src/udp, frame.rs, std::net::SocketAddrV4
```

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

Yes

4. Automation feasibility

- Spec: unsure
- Static analysis: difficult
- Dynamic enforcement overhead: likely not acceptable
