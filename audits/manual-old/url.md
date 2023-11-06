# [url](https://docs.rs/url/latest/url/)

Audited by: Caleb Stanford

Date: 2022-10-12

50th most downloaded crate

## List of imports (7)

```
host.rs, std::net::Ipv4Addr
host.rs, std::net::Ipv6Addr
lib.rs, std::net::IpAddr
lib.rs, std::net::SocketAddr
lib.rs, std::net::ToSocketAddrs
lib.rs, std::path::Path
lib.rs, std::path::PathBuf
```

## Analysis

All of these are parsing/URL management, probably without
giving access to the network.
Path and PathBuf do have filesystem-acting methods.

## Security summary

1. Security risks

Invalidly or incorrectly parsed URLs could lead to dangerous web endpoints

2. Permissions

None

3. Transitive risk

No

4. Automation feasibility

- Spec: project-independent
- Static analysis: feasible
- Dynamic enforcement overhead: probably acceptable
