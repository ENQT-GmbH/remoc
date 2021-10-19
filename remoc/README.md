# ReMOC 🦑 — Remote multiplexed objects and channels

[![crates.io page](https://img.shields.io/crates/v/remoc)](https://crates.io/crates/remoc)
[![docs.rs page](https://docs.rs/remoc/badge.svg)](https://docs.rs/remoc)
[![Apache 2 license](https://img.shields.io/crates/l/remoc)](https://raw.githubusercontent.com/ENQT-GmbH/remoc/master/LICENSE)

ReMOC is a [Rust] crate that provides async channels similar to [Tokio]'s, remote function calls and synchronization primitives over a single, arbitrary physical connection.
Any struct that can be serialized using [Serde] as well as large binary data can be exchanged over a ReMOC channel.

[Rust]: https://www.rust-lang.org/
[Tokio]: https://tokio.rs
[Serde]: https://serde.rs

## Pre-release

ReMOC is currently undergoing pre-release testing and will be released soon.

## Sponsors

Development of ReMOC is sponsored in part by [ENQT GmbH](https://enqt.de/).

[![ENQT Logo](https://enqt.de/wp-content/uploads/2017/02/ENQT-Logo_Tagline_transparent.png)](https://enqt.de/)
