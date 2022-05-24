use std::collections::HashSet;

use remoc::robs::{
    hash_set::{HashSetEvent, ObservableHashSet},
    RecvError,
};

#[tokio::test]
async fn standalone() {
    let mut obs: ObservableHashSet<_, remoc::codec::Default> = ObservableHashSet::new();

    obs.insert(10);
    obs.insert(20);
    assert_eq!(obs.get(&10), Some(&10));
    assert_eq!(obs.get(&20), Some(&20));

    obs.remove(&10);
    assert_eq!(obs.get(&10), None);

    obs.clear();
}

#[tokio::test]
async fn events() {
    let mut obs: ObservableHashSet<_, remoc::codec::Default> = ObservableHashSet::new();

    let mut sub = obs.subscribe(1024);
    assert!(!sub.is_complete());
    assert!(!sub.is_done());

    assert_eq!(sub.take_initial(), Some(HashSet::new()));
    assert!(sub.is_complete());
    assert!(!sub.is_done());

    obs.insert(10u32);
    assert_eq!(sub.recv().await.unwrap(), Some(HashSetEvent::Set(10)));

    obs.insert(20);
    assert_eq!(sub.recv().await.unwrap(), Some(HashSetEvent::Set(20)));

    obs.remove(&10);
    assert_eq!(sub.recv().await.unwrap(), Some(HashSetEvent::Remove(10)));

    obs.clear();
    assert_eq!(sub.recv().await.unwrap(), Some(HashSetEvent::Clear));

    obs.done();
    assert_eq!(sub.recv().await.unwrap(), Some(HashSetEvent::Done));
    assert!(sub.is_done());
}

#[tokio::test]
async fn events_incremental() {
    let mut hs = HashSet::new();
    hs.insert(0u32);
    hs.insert(1);
    hs.insert(2);

    let mut obs: ObservableHashSet<_, remoc::codec::Default> = ObservableHashSet::from(hs.clone());

    let mut sub = obs.subscribe_incremental(1024);
    assert!(!sub.is_complete());
    assert!(!sub.is_done());

    let mut hs2 = HashSet::new();
    for _ in 0..3 {
        match sub.recv().await.unwrap() {
            Some(HashSetEvent::Set(k)) => {
                hs2.insert(k);
            }
            other => panic!("unexpected event {other:?}"),
        }
    }
    assert_eq!(hs, hs2);

    assert_eq!(sub.recv().await.unwrap(), Some(HashSetEvent::InitialComplete));
    assert!(sub.is_complete());
    assert!(!sub.is_done());

    obs.done();
    assert_eq!(sub.recv().await.unwrap(), Some(HashSetEvent::Done));
    assert!(sub.is_done());
}

#[tokio::test]
async fn mirrored() {
    let mut pre = HashSet::new();
    for i in 1000..1500i32 {
        pre.insert(i);
    }
    let len = pre.len();

    let mut obs: ObservableHashSet<_, remoc::codec::Default> = ObservableHashSet::from(pre);
    assert_eq!(obs.len(), len);

    let sub = obs.subscribe(1024);
    let mut mirror = sub.mirror(1000);

    for i in 1..500 {
        obs.insert(i);
    }

    loop {
        let mb = mirror.borrow_and_update().await.unwrap();
        assert!(mb.is_complete());
        assert!(!mb.is_done());

        println!("original: {obs:?}");
        println!("mirrored: {mb:?}");
        if *mb == *obs {
            break;
        }
    }

    println!("remove");
    obs.remove(&10);
    mirror.changed().await;
    obs.shrink_to_fit();
    mirror.changed().await;

    {
        let mb = mirror.borrow_and_update().await.unwrap();
        assert_eq!(*mb, *obs);
    }

    println!("clear");
    obs.clear();
    mirror.changed().await;

    {
        let mb = mirror.borrow().await.unwrap();
        assert!(mb.is_empty());
        assert!(!mb.is_done());
    }

    assert!(!obs.is_done());
    println!("done");
    obs.done();
    assert!(obs.is_done());
    mirror.changed().await;

    {
        let mb = mirror.borrow().await.unwrap();
        assert!(mb.is_done());
    }
}

#[tokio::test]
async fn mirrored_disconnect() {
    let mut obs: ObservableHashSet<_, remoc::codec::Default> = ObservableHashSet::new();

    let sub = obs.subscribe(1024);
    let mut mirror = sub.mirror(1000);

    for i in 1..500 {
        obs.insert(i);
    }

    println!("drop");
    drop(obs);
    mirror.changed().await;
    assert!(matches!(mirror.borrow().await, Err(RecvError::Closed)));
}

#[tokio::test]
async fn mirrored_disconnect_after_done() {
    let mut obs: ObservableHashSet<_, remoc::codec::Default> = ObservableHashSet::new();

    let sub = obs.subscribe(1024);
    let mut mirror = sub.mirror(1000);

    for i in 1..500 {
        obs.insert(i);
    }

    println!("done and drop");
    obs.done();
    let hs = obs.into_inner();

    loop {
        let mb = mirror.borrow_and_update().await.unwrap();
        assert!(mb.is_complete());

        println!("original: {hs:?}");
        println!("mirrored: {mb:?}");
        if *mb == hs {
            break;
        }
    }

    let mb = mirror.borrow_and_update().await.unwrap();
    assert!(mb.is_done());
}

#[tokio::test]
async fn incremental() {
    let mut pre = HashSet::new();
    for i in 0..5000 {
        pre.insert(i);
    }
    let len = pre.len();

    let obs: ObservableHashSet<_, remoc::codec::Default> = ObservableHashSet::from(pre);
    assert_eq!(obs.len(), len);

    let sub = obs.subscribe_incremental(1024);
    let mut mirror = sub.mirror(10000);

    loop {
        let mb = mirror.borrow_and_update().await.unwrap();
        #[allow(clippy::comparison_chain)]
        if mb.len() < len {
            assert!(!mb.is_complete());
        } else if mb.len() > len {
            assert!(mb.is_complete());
        }

        println!("original: {obs:?}");
        println!("mirrored: {mb:?}");
        if *mb == *obs {
            break;
        }
    }
}
