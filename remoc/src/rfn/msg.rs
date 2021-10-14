//! Messages exchanged between remote functions and their providers.

use serde::{Deserialize, Serialize};

use crate::{codec, rch::oneshot, RemoteSend};

/// Remote function call request.
#[derive(Serialize, Deserialize)]
#[serde(bound(serialize = "A: RemoteSend, R: RemoteSend, Codec: codec::Codec"))]
#[serde(bound(deserialize = "A: RemoteSend, R: RemoteSend, Codec: codec::Codec"))]
pub struct RFnRequest<A, R, Codec> {
    /// Function argument.
    pub argument: A,
    /// Channel for result transmission.
    pub result_tx: oneshot::Sender<R, Codec>,
}
