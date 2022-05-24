use std::collections::HashMap;

use remoc::robs::{
    hash_map::{HashMapEvent, ObservableHashMap},
    RecvError,
};

#[tokio::test]
async fn standalone() {
    let mut obs: ObservableHashMap<_, _, remoc::codec::Default> = ObservableHashMap::new();

    obs.insert(10, "10".to_string());
    obs.insert(20, "20".to_string());
    assert_eq!(obs.get(&10), Some(&"10".to_string()));
    assert_eq!(obs.get(&20), Some(&"20".to_string()));

    obs.remove(&10);
    assert_eq!(obs.get(&10), None);

    obs.clear();
}

#[tokio::test]
async fn events() {
    let mut obs: ObservableHashMap<_, _, remoc::codec::Default> = ObservableHashMap::new();

    let mut sub = obs.subscribe(1024);
    assert!(!sub.is_complete());
    assert!(!sub.is_done());

    assert_eq!(sub.take_initial(), Some(HashMap::new()));
    assert!(sub.is_complete());
    assert!(!sub.is_done());

    obs.insert(10u32, "10".to_string());
    assert_eq!(sub.recv().await.unwrap(), Some(HashMapEvent::Set(10, "10".to_string())));

    obs.insert(20, "20".to_string());
    assert_eq!(sub.recv().await.unwrap(), Some(HashMapEvent::Set(20, "20".to_string())));

    obs.remove(&10);
    assert_eq!(sub.recv().await.unwrap(), Some(HashMapEvent::Remove(10)));

    obs.clear();
    assert_eq!(sub.recv().await.unwrap(), Some(HashMapEvent::Clear));

    obs.done();
    assert_eq!(sub.recv().await.unwrap(), Some(HashMapEvent::Done));
    assert!(sub.is_done());
}

#[tokio::test]
async fn events_incremental() {
    let mut hm = HashMap::new();
    hm.insert(0, "zero".to_string());
    hm.insert(1, "one".to_string());
    hm.insert(2, "two".to_string());

    let mut obs: ObservableHashMap<_, _, remoc::codec::Default> = ObservableHashMap::from(hm.clone());

    let mut sub = obs.subscribe_incremental(1024);
    assert!(!sub.is_complete());
    assert!(!sub.is_done());

    let mut hm2 = HashMap::new();
    for _ in 0..3 {
        match sub.recv().await.unwrap() {
            Some(HashMapEvent::Set(k, v)) => {
                hm2.insert(k, v);
            }
            other => panic!("unexpected event {other:?}"),
        }
    }
    assert_eq!(hm, hm2);

    assert_eq!(sub.recv().await.unwrap(), Some(HashMapEvent::InitialComplete));
    assert!(sub.is_complete());
    assert!(!sub.is_done());

    obs.done();
    assert_eq!(sub.recv().await.unwrap(), Some(HashMapEvent::Done));
    assert!(sub.is_done());
}

#[tokio::test]
async fn mirrored() {
    let mut pre = HashMap::new();
    for i in 1000..1500 {
        pre.insert(i, format!("pre {i}"));
    }
    let len = pre.len();

    let mut obs: ObservableHashMap<_, _, remoc::codec::Default> = ObservableHashMap::from(pre);
    assert_eq!(obs.len(), len);

    let sub = obs.subscribe(1024);
    let mut mirror = sub.mirror(1000);

    for i in 1..500 {
        obs.insert(i, format!("{i}"));
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
    let mut obs: ObservableHashMap<_, _, remoc::codec::Default> = ObservableHashMap::new();

    let sub = obs.subscribe(1024);
    let mut mirror = sub.mirror(1000);

    for i in 1..500 {
        obs.insert(i, format!("{i}"));
    }

    println!("drop");
    drop(obs);
    mirror.changed().await;
    assert!(matches!(mirror.borrow().await, Err(RecvError::Closed)));
}

#[tokio::test]
async fn mirrored_disconnect_after_done() {
    let mut obs: ObservableHashMap<_, _, remoc::codec::Default> = ObservableHashMap::new();

    let sub = obs.subscribe(1024);
    let mut mirror = sub.mirror(1000);

    for i in 1..500 {
        obs.insert(i, format!("{i}"));
    }

    println!("done and drop");
    obs.done();
    let hm = obs.into_inner();

    loop {
        let mb = mirror.borrow_and_update().await.unwrap();
        assert!(mb.is_complete());

        println!("original: {hm:?}");
        println!("mirrored: {mb:?}");
        if *mb == hm {
            break;
        }
    }

    let mb = mirror.borrow_and_update().await.unwrap();
    assert!(mb.is_done());
}

#[tokio::test]
async fn incremental() {
    let mut pre = HashMap::new();
    for i in 0..5000 {
        pre.insert(i, format!("pre {i}"));
    }
    let len = pre.len();

    let obs: ObservableHashMap<_, _, remoc::codec::Default> = ObservableHashMap::from(pre);
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
