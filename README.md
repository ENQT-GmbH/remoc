# Remoc-obs — remotely observable collections for use with [Remoc]

This crate provides collections that emit an event for each change.
This event stream can be sent to a remote endpoint via a [Remoc] channel,
where a mirrored collection can be built from it.

At the moment the following is implemented:
  * a remotely observable hash map
  * a channel buffer that stores and replays all values sent through it

[Remoc]: https://crates.io/crates/remoc
[![crates.io page](https://img.shields.io/crates/v/remoc-obs)](https://crates.io/crates/remoc-obs)
[![docs.rs page](https://docs.rs/remoc-obs/badge.svg)](https://docs.rs/remoc-obs)
[![Apache 2 license](https://img.shields.io/crates/l/remoc-obs)](https://raw.githubusercontent.com/surban/remoc-obs/master/LICENSE)

## Supported Rust versions

Remoc-obs is built against the latest stable release.
The minimum supported Rust version (MSRV) is 1.57.

## License

Remoc-obs is licensed under the [Apache 2.0 license].

[Apache 2.0 license]: https://github.com/surban/remoc-obc/blob/master/LICENSE

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in Remoc-obs by you, shall be licensed as Apache 2.0, without any
additional terms or conditions.
