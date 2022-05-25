# Changelog
All notable changes to Remoc will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.10.0 - 2022-05-25
## Added
- move remotely observable collections from remoc-obs crate into `robs` module
- `rch::watch::Receiver::send_modify` method
- `chmux` errors can now be converted into `std::io::Error`
## Changed
- minimum supported Rust version (MSRV) is 1.59
- remove `rch::buffer` types and use const generics directly to specify
  buffer sizes of received channel halves
- update `uuid` to 1.0
## Fixed
- fix infinite recursion in `std::fmt::Debug` implementation on some types

## 0.9.16 - 2022-02-24
### Added
- reference to remoc-obs crate for remotely observable collections

## 0.9.15 - 2022-02-08
### Changed
- optimize default configuration for higher throughput
### Added
- configuration defaults optimized for low memory usage or high throughput
- enhanced configuration documentation

## 0.9.14 - 2022-02-02
### Fixed
- fix build when no default codec was selected

## 0.9.13 - 2022-01-26
### Added
- ConnectExt trait that allows for replacement of the base channel by
  another object, such as an RTC client or remote broadcast channel
- RTC example in examples/rtc
### Changed
- optimized CI by baptiste0928
- updated rmp-serde to 1.0

## 0.9.12 - 2022-01-24
### Fixed
- export rch::watch::ChangedError

## 0.9.11 - 2022-01-17
### Added
- conversions between remote channel receive errors
- error message when trying to use lifetimes or function generics in a remote trait

## 0.9.10 - 2022-01-03
### Added
- Cbor codec using ciborium, contributed by baptiste0928
### Deprecated
- legacy Cbor codec using serde_cbor

## 0.9.9 - 2021-12-10
### Added
- rch::mpsc::Receiver implements the Stream trait
- ReceiverStream for rch::broadcast::Receiver and rch::watch::Receiver
- rch::watch::Sender::send_replace

## 0.9.8 - 2021-12-07
### Added
- rch::SendErrorExt and rch::SendResultExt for quick querying if a send error
  was due to disconnection

## 0.9.7 - 2021-11-26
### Added
- rch::mpsc::Receiver::try_recv, error and take_error
- rch::mpsc::Sender::closed_reason
- `full-codecs` crate feature to activate all codecs
### Changed
- An mpsc channel receiver will hold back a receive error due to connection failure
  if other senders are still active. The error will be returned after all other
  senders have been disconnected.
- Fixes premature drop of RwLock owners.

## 0.9.6 - 2021-11-18
### Added
- add rtc::Client to prelude

## 0.9.5 - 2021-11-17
### Added
- rtc::Client trait implemented by all generated clients. This allows to
  receive notifications when the server has been dropped or disconnected.
- configuration options for transport queue lengths
### Changed
- fix mpsc channel close notifications not being delivered sometimes

## 0.9.4 - 2021-11-17
### Changed
- fix build when no default codec is set

## 0.9.3 - 2021-11-11
### Changed
- fix premature chmux termination with outstanding remote port requests
- fix build with Rust 1.51

## 0.9.2 - 2021-11-11
### Changed
- fix send error being missed during threaded serialization

## 0.9.1 - 2021-11-02
### Changed
- fix panic during threaded deserialization
- propagate panics from serializers and deserializers spawned into threads

## 0.9.0 - 2021-11-01
### Added
- `is_final()` on channel error types
### Changed
- terminate providers and RTC servers when a final receive error occurs
### Removed
- `chmux::Cfg::trace_id` because using tracing spans makes it redundant

## 0.8.2 - 2021-10-29
### Added
- blocking send and receive functions for rch::mpsc
### Changed
- switch to `tracing` crate for logging

## 0.8.1 - 2021-10-26
### Changed
- remove `default-codec-json` from `full` feature

## 0.8.0 - 2021-10-21
- initial release
