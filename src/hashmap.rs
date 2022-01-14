use remoc::prelude::*;
use serde::{Deserialize, Serialize};
use std::{
    borrow::Borrow,
    collections::HashMap,
    error::Error,
    fmt,
    hash::Hash,
    iter::FusedIterator,
    ops::{Deref, DerefMut},
    sync::Arc,
};
use tokio::sync::{OwnedRwLockReadGuard, RwLock, RwLockReadGuard};

pub use rch::broadcast::RecvError;

/// An error occurred during sending an event for an observable HashMap change.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SendError {
    /// Sending to a remote endpoint failed.
    RemoteSend(rch::base::SendErrorKind),
    /// Connecting a sent channel failed.
    RemoteConnect(chmux::ConnectError),
    /// Listening to a received channel failed.
    RemoteListen(chmux::ListenerError),
    /// Forwarding at a remote endpoint to another remote endpoint failed.
    RemoteForward,
}

impl fmt::Display for SendError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::RemoteSend(err) => write!(f, "send error: {}", err),
            Self::RemoteConnect(err) => write!(f, "connect error: {}", err),
            Self::RemoteListen(err) => write!(f, "listen error: {}", err),
            Self::RemoteForward => write!(f, "forwarding error"),
        }
    }
}

impl Error for SendError {}

impl<T> TryFrom<rch::broadcast::SendError<T>> for SendError {
    type Error = rch::broadcast::SendError<T>;

    fn try_from(err: rch::broadcast::SendError<T>) -> Result<Self, Self::Error> {
        match err {
            rch::broadcast::SendError::RemoteSend(err) => Ok(Self::RemoteSend(err)),
            rch::broadcast::SendError::RemoteConnect(err) => Ok(Self::RemoteConnect(err)),
            rch::broadcast::SendError::RemoteListen(err) => Ok(Self::RemoteListen(err)),
            rch::broadcast::SendError::RemoteForward => Ok(Self::RemoteForward),
            other => Err(other),
        }
    }
}

/// A hash map change event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HashMapEvent<K, V> {
    /// An item was inserted or modified.
    Set(K, V),
    /// An item was removed.
    Remove(K),
    /// All items were removed.
    Clear,
}

fn send_event<K, V, Codec>(
    tx: &rch::broadcast::Sender<HashMapEvent<K, V>, Codec>, on_err: &Box<dyn Fn(SendError)>,
    event: HashMapEvent<K, V>,
) where
    Codec: remoc::codec::Codec,
    HashMapEvent<K, V>: RemoteSend + Clone,
{
    match tx.send(event) {
        Ok(()) => (),
        Err(err) if err.is_disconnected() => (),
        Err(err) => match err.try_into() {
            Ok(err) => (on_err)(err),
            Err(_) => unreachable!(),
        },
    }
}

/// A hash map that emits an event for each change.
pub struct ObservableHashMap<K, V, Codec = remoc::codec::Default> {
    hm: HashMap<K, V>,
    tx: rch::broadcast::Sender<HashMapEvent<K, V>, Codec>,
    on_err: Box<dyn Fn(SendError)>,
}

impl<K, V> fmt::Debug for ObservableHashMap<K, V>
where
    K: fmt::Debug,
    V: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.hm.fmt(f)
    }
}

impl<K, V, Codec> From<HashMap<K, V>> for ObservableHashMap<K, V, Codec>
where
    K: Eq + Hash + Clone + RemoteSend,
    V: Clone + RemoteSend,
    Codec: remoc::codec::Codec,
{
    fn from(hm: HashMap<K, V>) -> Self {
        let (tx, _rx) = rch::broadcast::channel::<_, _, rch::buffer::Default>(1);
        let on_err = |err: SendError| {
            tracing::warn!("sending event failed: {}", err);
        };
        Self { hm, tx, on_err: Box::new(on_err) }
    }
}

impl<K, V, Codec> From<ObservableHashMap<K, V, Codec>> for HashMap<K, V> {
    fn from(ohm: ObservableHashMap<K, V, Codec>) -> Self {
        ohm.hm
    }
}

impl<K, V, Codec> ObservableHashMap<K, V, Codec>
where
    K: Eq + Hash + Clone + RemoteSend,
    V: Clone + RemoteSend,
    Codec: remoc::codec::Codec,
{
    /// Creates an empty observable hash map.
    pub fn new() -> Self {
        Self::from(HashMap::new())
    }

    /// Sets the error handler function that is called when sending a change
    /// event fails.
    pub fn set_error_handler<E>(&mut self, on_err: E)
    where
        E: Fn(SendError) + 'static,
    {
        self.on_err = Box::new(on_err);
    }

    /// Subscribes to change events from this observable hash map.
    pub fn subscribe(&self, send_buffer: usize) -> HashMapSubscription<K, V, Codec> {
        HashMapSubscription { initial: self.hm.clone(), events: self.tx.subscribe(send_buffer) }
    }

    /// Inserts a value under a key.
    ///
    /// A [HashMapEvent::Set] change event is sent.
    ///
    /// Returns the value previously stored under the key, if any.
    pub fn insert(&mut self, k: K, v: V) -> Result<Option<V>, SendError> {
        send_event(&self.tx, &self.on_err, HashMapEvent::Set(k.clone(), v.clone()));
        Ok(self.hm.insert(k, v))
    }

    /// Removes the value under the specified key.
    ///
    /// A [HashMapEvent::Remove] change event is sent.
    ///
    /// The value is returned.
    pub fn remove<Q>(&mut self, k: &Q) -> Result<Option<V>, SendError>
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        match self.hm.remove_entry(k) {
            Some((k, v)) => {
                send_event(&self.tx, &self.on_err, HashMapEvent::Remove(k));
                Ok(Some(v))
            }
            None => Ok(None),
        }
    }

    /// Removes all items.
    ///
    /// A [HashMapEvent::Clear] change event is sent.
    pub fn clear(&mut self) {
        if !self.hm.is_empty() {
            self.hm.clear();
            send_event(&self.tx, &self.on_err, HashMapEvent::Clear);
        }
    }

    /// Retains only the elements specified by the predicate.
    ///
    /// A [HashMapEvent::Remove] change event is sent for every element that is removed.
    pub fn retain<F>(&mut self, mut f: F)
    where
        F: FnMut(&K, &mut V) -> bool,
    {
        self.hm.retain(|k, v| {
            if f(k, v) {
                true
            } else {
                send_event(&self.tx, &self.on_err, HashMapEvent::Remove(k.clone()));
                false
            }
        });
    }

    /// Gets a mutable reference to the value under the specified key.
    ///
    /// A [HashMapEvent::Set] change event is sent if the reference is accessed mutably.
    pub fn get_mut<Q>(&mut self, k: &Q) -> Option<RefMut<'_, K, V, Codec>>
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
        V: Eq,
    {
        match self.hm.get_key_value(k) {
            Some((key, _)) => {
                let key = key.clone();
                let value = self.hm.get_mut(k).unwrap();
                Some(RefMut { key, value, changed: false, tx: &self.tx, on_err: &self.on_err })
            }
            None => None,
        }
    }

    /// Mutably iterates over the key-value pairs.
    ///
    /// A [HashMapEvent::Set] change event is sent for each value that is accessed mutably.
    pub fn iter_mut(&mut self) -> IterMut<'_, K, V, Codec> {
        IterMut { inner: self.hm.iter_mut(), tx: &self.tx, on_err: &self.on_err }
    }

    /// Shrinks the capacity of the hash map as much as possible.
    pub fn shrink_to_fit(&mut self) {
        self.hm.shrink_to_fit()
    }
}

impl<K, V, Codec> Deref for ObservableHashMap<K, V, Codec> {
    type Target = HashMap<K, V>;

    fn deref(&self) -> &Self::Target {
        &self.hm
    }
}

/// A mutable reference to a value inside an [observable hash map](ObservableHashMap).
///
/// A [HashMapEvent::Set] change event is sent when this reference is dropped and the
/// value has been accessed mutably.
pub struct RefMut<'a, K, V, Codec>
where
    K: Clone + RemoteSend,
    V: Clone + RemoteSend,
    Codec: remoc::codec::Codec,
{
    key: K,
    value: &'a mut V,
    changed: bool,
    tx: &'a rch::broadcast::Sender<HashMapEvent<K, V>, Codec>,
    on_err: &'a Box<dyn Fn(SendError)>,
}

impl<'a, K, V, Codec> Deref for RefMut<'a, K, V, Codec>
where
    K: Clone + RemoteSend,
    V: Clone + RemoteSend,
    Codec: remoc::codec::Codec,
{
    type Target = V;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<'a, K, V, Codec> DerefMut for RefMut<'a, K, V, Codec>
where
    K: Clone + RemoteSend,
    V: Clone + RemoteSend,
    Codec: remoc::codec::Codec,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.changed = true;
        &mut self.value
    }
}

impl<'a, K, V, Codec> Drop for RefMut<'a, K, V, Codec>
where
    K: Clone + RemoteSend,
    V: Clone + RemoteSend,
    Codec: remoc::codec::Codec,
{
    fn drop(&mut self) {
        if self.changed {
            send_event(self.tx, self.on_err, HashMapEvent::Set(self.key.clone(), self.value.clone()));
        }
    }
}

/// A mutable iterator over the key-value pairs in an [observable hash map](ObservableHashMap).
///
/// A [HashMapEvent::Set] change event is sent for each value that is accessed mutably.
pub struct IterMut<'a, K, V, Codec> {
    inner: std::collections::hash_map::IterMut<'a, K, V>,
    tx: &'a rch::broadcast::Sender<HashMapEvent<K, V>, Codec>,
    on_err: &'a Box<dyn Fn(SendError)>,
}

impl<'a, K, V, Codec> Iterator for IterMut<'a, K, V, Codec>
where
    K: Clone + RemoteSend,
    V: Clone + RemoteSend,
    Codec: remoc::codec::Codec,
{
    type Item = RefMut<'a, K, V, Codec>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.inner.next() {
            Some((key, value)) => {
                Some(RefMut { key: key.clone(), value, changed: false, tx: &self.tx, on_err: &self.on_err })
            }
            None => None,
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<'a, K, V, Codec> ExactSizeIterator for IterMut<'a, K, V, Codec>
where
    K: Clone + RemoteSend,
    V: Clone + RemoteSend,
    Codec: remoc::codec::Codec,
{
    fn len(&self) -> usize {
        self.inner.len()
    }
}

impl<'a, K, V, Codec> FusedIterator for IterMut<'a, K, V, Codec>
where
    K: Clone + RemoteSend,
    V: Clone + RemoteSend,
    Codec: remoc::codec::Codec,
{
}

/// Observable hash map subscription.
#[derive(Debug, Serialize, Deserialize)]
#[serde(bound(serialize = "K: RemoteSend + Eq + Hash, V: RemoteSend, Codec: remoc::codec::Codec"))]
#[serde(bound(deserialize = "K: RemoteSend + Eq + Hash, V: RemoteSend, Codec: remoc::codec::Codec"))]
pub struct HashMapSubscription<K, V, Codec> {
    /// Value of hash map at time of subscription.
    pub initial: HashMap<K, V>,
    /// Change events receiver.
    pub events: rch::broadcast::Receiver<HashMapEvent<K, V>, Codec>,
}

impl<K, V, Codec> HashMapSubscription<K, V, Codec>
where
    K: RemoteSend + Eq + Hash + Clone + RemoteSend + Sync,
    V: RemoteSend + Clone + RemoteSend + Sync,
    Codec: remoc::codec::Codec,
{
    /// Mirror the observed hash map that this subscription is receiving events from.
    pub fn mirror(self) -> MirroredHashMap<K, V, Codec> {
        let Self { initial, mut events } = self;

        let inner = Arc::new(RwLock::new(Some(MirroredHashMapInner { hm: initial, error: None })));
        let inner_weak = Arc::downgrade(&inner);

        let (tx, _rx) = rch::broadcast::channel::<_, _, rch::buffer::Default>(1);
        let tx_send = tx.clone();

        tokio::spawn(async move {
            loop {
                let event = events.recv().await;

                let inner = match inner_weak.upgrade() {
                    Some(inner) => inner,
                    None => break,
                };
                let mut inner = inner.write().await;
                let mut inner = match inner.as_mut() {
                    Some(inner) => inner,
                    None => break,
                };

                match event {
                    Ok(event) => {
                        let _ = tx_send.send(event.clone());
                        inner.handle_event(event);

                        loop {
                            match events.try_recv() {
                                Ok(event) => {
                                    let _ = tx_send.send(event.clone());
                                    inner.handle_event(event);
                                }
                                Err(err) => {
                                    if let Ok(err) = RecvError::try_from(err) {
                                        inner.error = Some(err);
                                    }
                                    break;
                                }
                            }
                        }
                    }
                    Err(err) => {
                        inner.error = Some(err);
                        break;
                    }
                }
            }
        });

        MirroredHashMap { inner, tx }
    }
}

/// A hash map that is mirroring an observable hash map.
pub struct MirroredHashMap<K, V, Codec> {
    inner: Arc<RwLock<Option<MirroredHashMapInner<K, V>>>>,
    tx: rch::broadcast::Sender<HashMapEvent<K, V>, Codec>,
}

struct MirroredHashMapInner<K, V> {
    hm: HashMap<K, V>,
    error: Option<RecvError>,
}

impl<K, V> MirroredHashMapInner<K, V>
where
    K: Eq + Hash,
{
    fn handle_event(&mut self, event: HashMapEvent<K, V>) {
        match event {
            HashMapEvent::Set(k, v) => {
                self.hm.insert(k, v);
            }
            HashMapEvent::Remove(k) => {
                self.hm.remove(&k);
            }
            HashMapEvent::Clear => {
                self.hm.clear();
            }
        }
    }
}

impl<K, V, Codec> MirroredHashMap<K, V, Codec>
where
    K: RemoteSend + Clone,
    V: RemoteSend + Clone,
    Codec: remoc::codec::Codec,
{
    /// Locks the hash map for reading.
    ///
    /// Updates are paused while the read lock is held.
    pub async fn read(&self) -> Result<ObsHashMapView<'_, K, V>, RecvError> {
        let inner = self.inner.read().await;
        let inner = RwLockReadGuard::map(inner, |inner| inner.as_ref().unwrap());
        match &inner.error {
            None => Ok(ObsHashMapView(RwLockReadGuard::map(inner, |inner| &inner.hm))),
            Some(err) => Err(err.clone()),
        }
    }

    /// Stops updating the hash map and returns its current version.
    pub async fn take(self) -> HashMap<K, V> {
        let mut inner = self.inner.write().await;
        inner.take().unwrap().hm
    }

    /// Subscribes to change events from this mirrored hash map.
    pub async fn subscribe(&self, send_buffer: usize) -> Result<HashMapSubscription<K, V, Codec>, RecvError> {
        let view = self.read().await?;
        let initial = view.clone();
        let events = self.tx.subscribe(send_buffer);
        drop(view);

        Ok(HashMapSubscription { initial, events })
    }
}

/// A snapshot view of an observable hash map.
pub struct ObsHashMapView<'a, K, V>(RwLockReadGuard<'a, HashMap<K, V>>);

impl<'a, K, V> fmt::Debug for ObsHashMapView<'a, K, V>
where
    K: fmt::Debug,
    V: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<'a, K, V> Deref for ObsHashMapView<'a, K, V> {
    type Target = HashMap<K, V>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
