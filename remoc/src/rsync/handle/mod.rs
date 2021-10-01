//! Remote handle to an object.

use serde::{Deserialize, Serialize};
use std::{
    any::Any,
    collections::HashMap,
    fmt,
    marker::PhantomData,
    mem,
    ops::{Deref, DerefMut},
    sync::Arc,
};
use tokio::sync::{OwnedRwLockMappedWriteGuard, OwnedRwLockReadGuard, OwnedRwLockWriteGuard};
use uuid::Uuid;

use super::{
    mpsc,
    remote::{PortDeserializer, PortSerializer},
};
use crate::codec::CodecT;

/// An error during getting the value of a handle.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum HandleError {
    /// The value of the handle is not stored locally or it has already
    /// been taken.
    Unknown,
    /// The values of the handle is of another type.
    MismatchedType(String),
}

impl fmt::Display for HandleError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            HandleError::Unknown => write!(f, "unknown, taked or non-local handle"),
            HandleError::MismatchedType(ty) => write!(f, "mismatched handle type: {}", ty),
        }
    }
}

impl std::error::Error for HandleError {}

/// Box containing any value that is Send, Sync and static.
pub type AnyBox = Box<dyn Any + Send + Sync + 'static>;

/// Stores arbitrary data using automatically generated keys.
#[derive(Clone)]
pub struct HandleStorage {
    entries: Arc<std::sync::Mutex<HandleMap>>,
}

type HandleEntry = Arc<tokio::sync::RwLock<Option<AnyBox>>>;
type HandleMap = HashMap<Uuid, HandleEntry>;

impl HandleStorage {
    /// Creates a new handle storage.
    pub fn new() -> Self {
        Self { entries: Arc::new(std::sync::Mutex::new(HandleMap::new())) }
    }

    /// Insert a new entry into the storage and return its key.
    pub fn insert(&self, entry: HandleEntry) -> Uuid {
        let key = Uuid::new_v4();
        let mut entries = self.entries.lock().unwrap();
        if entries.insert(key, entry).is_some() {
            panic!("duplicate UUID");
        }
        key
    }

    /// Returns the value from the handle storage for the specified key.
    pub fn get(&self, key: Uuid) -> Option<HandleEntry> {
        let entries = self.entries.lock().unwrap();
        entries.get(&key).cloned()
    }

    /// Removes the value for the specified key from the handle storage and returns it.
    pub fn remove(&self, key: Uuid) -> Option<HandleEntry> {
        let mut entries = self.entries.lock().unwrap();
        entries.remove(&key)
    }
}

impl Default for HandleStorage {
    fn default() -> Self {
        Self::new()
    }
}

/// Provider for a handle.
pub struct Provider {
    keep_tx: Option<tokio::sync::watch::Sender<bool>>,
}

impl fmt::Debug for Provider {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Provider").finish_non_exhaustive()
    }
}

impl Provider {
    /// Keeps the provider alive until the handle is dropped.
    pub fn keep(mut self) {
        let _ = self.keep_tx.take().unwrap().send(true);
    }

    /// Waits until the handle provider can be safely dropped.
    ///
    /// This is the case when the handle is dropped.
    pub async fn done(&mut self) {
        self.keep_tx.as_mut().unwrap().closed().await
    }
}

impl Drop for Provider {
    fn drop(&mut self) {
        // empty
    }
}

/// Handle state.
#[derive(Clone)]
enum State<Codec> {
    /// Empty (for dropping).
    Empty,
    /// Value has been created locally.
    LocalCreated {
        /// Reference to value.
        entry: HandleEntry,
        /// Keep notification.
        keep_rx: tokio::sync::watch::Receiver<bool>,
    },
    /// Value is stored locally and handle has been received from
    /// a remote endpoint.
    LocalReceived {
        /// Reference to value.
        entry: HandleEntry,
        /// Id in local handle storage.
        id: Uuid,
        /// Dropped notification.
        dropped_tx: mpsc::Sender<(), Codec, 1>,
    },
    /// Value is stored on a remote endpoint.
    Remote {
        /// Id in remote handle storage.
        id: Uuid,
        /// Dropped notification.
        dropped_tx: mpsc::Sender<(), Codec, 1>,
    },
}

impl<Codec> Default for State<Codec> {
    fn default() -> Self {
        Self::Empty
    }
}

impl<Codec> fmt::Debug for State<Codec> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "Empty"),
            Self::LocalCreated { .. } => f.debug_struct("LocalCreated").finish_non_exhaustive(),
            Self::LocalReceived { id, .. } => {
                f.debug_struct("LocalReceived").field("id", id).finish_non_exhaustive()
            }
            Self::Remote { id, .. } => f.debug_struct("Remote").field("id", id).finish_non_exhaustive(),
        }
    }
}

/// A handle to a value that is possibly stored on a remote endpoint.
///
/// If the value is stored locally, the handle can be used to obtain the value
/// or a (mutable) reference to it.
#[derive(Clone)]
pub struct Handle<T, Codec> {
    state: State<Codec>,
    _data: PhantomData<T>,
}

impl<T, Codec> fmt::Debug for Handle<T, Codec> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Handle").field("state", &self.state).finish_non_exhaustive()
    }
}

impl<T, Codec> Handle<T, Codec>
where
    T: Send + Sync + 'static,
{
    /// Creates a new handle for the value.
    pub fn new(value: T) -> Self {
        let (handle, provider) = Self::provided(value);
        provider.keep();
        handle
    }

    /// Creates a new handle for the value and returns it together
    /// with its provider.
    pub fn provided(value: T) -> (Self, Provider) {
        let (keep_tx, keep_rx) = tokio::sync::watch::channel(false);

        let handle = Self {
            state: State::LocalCreated {
                entry: Arc::new(tokio::sync::RwLock::new(Some(Box::new(value)))),
                keep_rx,
            },
            _data: PhantomData,
        };
        let provider = Provider { keep_tx: Some(keep_tx) };

        (handle, provider)
    }

    /// Takes the value of the handle and returns it, if it is stored locally.
    ///
    /// The handle and all its clones become invalid.
    pub async fn into_inner(mut self) -> Result<T, HandleError> {
        let entry = match mem::take(&mut self.state) {
            State::LocalCreated { entry, .. } => entry,
            State::LocalReceived { entry, .. } => entry,
            _ => return Err(HandleError::Unknown),
        };

        let mut entry = entry.write().await;
        match entry.take() {
            Some(any) => match any.downcast::<T>() {
                Ok(value) => Ok(*value),
                Err(any) => Err(HandleError::MismatchedType(format!("{:?}", any.type_id()))),
            },
            None => Err(HandleError::Unknown),
        }
    }

    /// Returns a reference to the value of the handle, if it is stored locally.
    pub async fn as_ref(&self) -> Result<Ref<T>, HandleError> {
        let entry = match &self.state {
            State::LocalCreated { entry, .. } | State::LocalReceived { entry, .. } => entry.clone(),
            _ => return Err(HandleError::Unknown),
        };

        let entry = entry.read_owned().await;
        match &*entry {
            Some(any) => {
                if !any.is::<T>() {
                    return Err(HandleError::MismatchedType(format!("{:?}", any.type_id())));
                }
                let value_ref = OwnedRwLockReadGuard::map(entry, |entry| {
                    entry.as_ref().unwrap().downcast_ref::<T>().unwrap()
                });
                Ok(Ref(value_ref))
            }
            None => Err(HandleError::Unknown),
        }
    }

    /// Returns a mutable reference to the value of the handle, if it is stored locally.
    pub async fn as_mut(&mut self) -> Result<RefMut<T>, HandleError> {
        let entry = match &self.state {
            State::LocalCreated { entry, .. } | State::LocalReceived { entry, .. } => entry.clone(),
            _ => return Err(HandleError::Unknown),
        };

        let entry = entry.write_owned().await;
        match &*entry {
            Some(any) => {
                if !any.is::<T>() {
                    return Err(HandleError::MismatchedType(format!("{:?}", any.type_id())));
                }
                let value_ref = OwnedRwLockWriteGuard::map(entry, |entry| {
                    entry.as_mut().unwrap().downcast_mut::<T>().unwrap()
                });
                Ok(RefMut(value_ref))
            }
            None => Err(HandleError::Unknown),
        }
    }
}

impl<T, Codec> Drop for Handle<T, Codec> {
    fn drop(&mut self) {
        // empty
    }
}

/// Handle in transport.
#[derive(Debug, Serialize, Deserialize)]
#[serde(bound(serialize = "Codec: CodecT"))]
#[serde(bound(deserialize = "Codec: CodecT"))]
pub struct TransportedHandle<T, Codec> {
    /// Handle id.
    id: Uuid,
    /// Dropped notification.
    dropped_tx: mpsc::Sender<(), Codec, 1>,
    /// Data type.
    data: PhantomData<T>,
    /// Codec
    codec: PhantomData<Codec>,
}

impl<T, Codec> Serialize for Handle<T, Codec>
where
    Codec: CodecT,
{
    /// Serializes this handle for sending over a chmux channel.
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let (id, dropped_tx) = match self.state.clone() {
            State::LocalCreated { entry, mut keep_rx, .. } => {
                let handle_storage = PortSerializer::handle_storage()?;
                let id = handle_storage.insert(entry.clone());

                let (dropped_tx, mut dropped_rx) = mpsc::channel::<_, _, 1, 1>(1);

                tokio::spawn(async move {
                    loop {
                        if *keep_rx.borrow_and_update() {
                            let _ = dropped_rx.recv().await;
                        } else {
                            tokio::select! {
                                biased;
                                res = keep_rx.changed() => {
                                    if res.is_err() {
                                        break;
                                    }
                                },
                                _ = dropped_rx.recv() => (),
                            }
                        }
                    }

                    handle_storage.remove(id);
                });

                (id, dropped_tx)
            }
            State::LocalReceived { id, dropped_tx, .. } | State::Remote { id, dropped_tx } => (id, dropped_tx),
            State::Empty => unreachable!("state is only empty when dropping"),
        };

        let transported = TransportedHandle::<T, Codec> { id, dropped_tx, data: PhantomData, codec: PhantomData };

        transported.serialize(serializer)
    }
}

impl<'de, T, Codec> Deserialize<'de> for Handle<T, Codec>
where
    Codec: CodecT,
{
    /// Deserializes this handle after it has been received over a chmux channel.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let TransportedHandle { id, dropped_tx, .. } = TransportedHandle::<T, Codec>::deserialize(deserializer)?;

        let handle_storage = PortDeserializer::handle_storage()?;
        let state = match handle_storage.remove(id) {
            Some(entry) => State::LocalReceived { entry, id, dropped_tx },
            None => State::Remote { id, dropped_tx },
        };

        Ok(Self { state, _data: PhantomData })
    }
}

/// An owned reference to the value of a handle.
pub struct Ref<T>(OwnedRwLockReadGuard<Option<AnyBox>, T>);

impl<T> Deref for Ref<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

impl<T> fmt::Debug for Ref<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", &*self)
    }
}

/// An owned mutable reference to the value of a handle.
pub struct RefMut<T>(OwnedRwLockMappedWriteGuard<Option<AnyBox>, T>);

impl<T> Deref for RefMut<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

impl<T> DerefMut for RefMut<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut *self.0
    }
}

impl<T> fmt::Debug for RefMut<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", &*self)
    }
}
