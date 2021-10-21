# Remoc 🦑 — remote multiplexed objects and channels

Remoc makes remote interaction between Rust programs seamless and smooth.
Over a single underlying transport, it provides: 

  * multiple channels of different types like MPSC, oneshot, watch, etc.,
  * remote synchronization primitives,
  * calling of remote functions and trait methods on a remote object (RPC).

Remoc is written in [100% safe Rust], builds upon [Tokio], works with any type and data format supported by [Serde] and does not depend on any particular transport type.

[100% safe Rust]: https://www.rust-lang.org/
[Tokio]: https://tokio.rs
[Serde]: https://serde.rs

[![crates.io page](https://img.shields.io/crates/v/remoc)](https://crates.io/crates/remoc)
[![docs.rs page](https://docs.rs/remoc/badge.svg)](https://docs.rs/remoc)
[![Apache 2 license](https://img.shields.io/crates/l/remoc)](https://raw.githubusercontent.com/ENQT-GmbH/remoc/master/LICENSE)
[![codecov](https://codecov.io/gh/ENQT-GmbH/remoc/branch/master/graph/badge.svg?token=UDMOOK0QT8)](https://codecov.io/gh/ENQT-GmbH/remoc)


## Introduction

A common pattern in Rust programs is to use channels to communicate between threads and async tasks.
Setting up a channel is done in a single line and it largely avoids the need for shared state and the associated complexity.
Remoc extends this programming model to distributed systems by providing channels that work seamlessly over remote connections.

For that it uses Serde to serialize and deserialize data as it gets transmitted over an underlying transport,
which might be a TCP network connection, a WebSocket, UNIX pipe, or even a serial link.
Opening a new channel is straightforward, just send the sender or receiver half of the new channel over an existing channel, 
like you would do between local threads and tasks.
All channels are multiplexed over the same remote connection, with data being transmitted in chunks to avoid one channel
blocking another if a large message is transmitted.

Building upon its remote channels, Remoc allows calling of remote functions and closures.
Furthermore, a trait can be made remote callable with automatically generated client and server implementations, resembling
a classical remote procedure calling (RPC) model.


## Forward and backward compatibility

Distributed systems often require that endpoints running different software versions interact.
By utilizing a self-describing data format like JSON for encoding of your data for transport, you can ensure a high level of
backward and forward compatibility.
It is always possible to add new fields to enums and struct and utilize the `#[serde(default)]` attribute to provide
default values for that field if it was sent by an older client that has no knowledge of it.
Likewise additional, unknown fields are silently ignored when received, allowing you to extend the data format
without disturbing legacy endpoints.


## Installation

Add the following line to your Cargo.toml:

    [dependencies]
    remoc = "0.8"


## Crate features

Most functionality of Remoc is gated by crate features.
The following features are available:

  * `serde` enables the `codec` module and implements serialize and deserialize for all configuration and error types.
  * `rch` enables remote channels provided by the `rch` module.
  * `rfn` enables remote function calling provided by the `rfn` module.
  * `robj` enables remote object utilities provided by the `robj` module.
  * `rtc` enables remote trait calling provided by the `rtc` module.

The following features enable data formats for transmission:

  * `codec-bincode` provides the Bincode format.
  * `codec-cbor` provides the CBOR format.
  * `codec-json` provides the JSON format.
  * `codec-message-pack` provides the MessagePack format.

The feature `default-codec-*` selects the respective codec as default. 
At most one of this must be selected and this should only be used by applications, not libraries.

The meta-feature `full` enables all features and codecs from above, but selects no default codec.

By default all features are enabled and the JSON codec is used as default.


## Supported Rust versions

Remoc is built against the latest stable release.
The minimum supported Rust version (MSRV) is 1.51.

## Example

TODO

## Sponsors

Development of Remoc is partially sponsored by [ENQT GmbH](https://enqt.de/).

[![ENQT Logo](https://raw.githubusercontent.com/ENQT-GmbH/remoc/master/.misc/ENQT.png)](https://enqt.de/)

## License

Remoc is licensed under the [Apache 2.0 license].

[Apache 2.0 license]: https://github.com/ENQT-GmbH/remoc/blob/master/LICENSE

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in Remoc by you, shall be licensed as Apache 2.0, without any additional
terms or conditions.
