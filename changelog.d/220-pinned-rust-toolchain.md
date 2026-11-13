### Internal: Rust toolchain

- Changed: `rust-toolchain.toml` pins Rust 1.100.0 for local builds and every CI and release job,
  which install it from there; Renovate proposes each new release a week after it ships (#220).
