# Remoc 🦑 — Remote multiplexed objects and channels

[![crates.io page](https://img.shields.io/crates/v/remoc)](https://crates.io/crates/remoc)
[![docs.rs page](https://docs.rs/remoc/badge.svg)](https://docs.rs/remoc)
[![Apache 2 license](https://img.shields.io/crates/l/remoc)](https://raw.githubusercontent.com/ENQT-GmbH/remoc/master/LICENSE)
[![codecov](https://codecov.io/gh/ENQT-GmbH/remoc/branch/master/graph/badge.svg?token=UDMOOK0QT8)](https://codecov.io/gh/ENQT-GmbH/remoc)





Remoc is a [Rust] crate that provides async channels similar to [Tokio]'s, remote function calls and synchronization primitives over a single, arbitrary physical connection.
Any struct that can be serialized using [Serde] as well as large binary data can be exchanged over a Remoc channel.

[Rust]: https://www.rust-lang.org/
[Tokio]: https://tokio.rs
[Serde]: https://serde.rs


## Sponsors

Development of Remoc is partially sponsored by [ENQT GmbH](https://enqt.de/).

[![ENQT Logo](https://github.com/ENQT-GmbH/remoc/blob/master/.misc/ENQT.png)](https://enqt.de/)


## License

This project is licensed under the [Apache 2.0 license].

[Apache 2.0 license]: https://github.com/ENQT-GmbH/remoc/blob/master/LICENSE

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in Remoc by you, shall be licensed as Apache 2.0, without any additional
terms or conditions.
