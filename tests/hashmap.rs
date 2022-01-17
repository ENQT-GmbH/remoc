use remoc_obs::hashmap::{HashMapEvent, ObservableHashMap};

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

    obs.insert(10, "10".to_string());
    assert_eq!(sub.events.recv().await.unwrap(), HashMapEvent::Set(10, "10".to_string()));

    obs.insert(20, "20".to_string());
    assert_eq!(sub.events.recv().await.unwrap(), HashMapEvent::Set(20, "20".to_string()));

    obs.remove(&10);
    assert_eq!(sub.events.recv().await.unwrap(), HashMapEvent::Remove(10));

    obs.clear();
    assert_eq!(sub.events.recv().await.unwrap(), HashMapEvent::Clear);
}

#[tokio::test]
async fn mirrored() {
    let mut obs: ObservableHashMap<_, _, remoc::codec::Default> = ObservableHashMap::new();

    let sub = obs.subscribe(1024);
    let mut mirror = sub.mirror();

    for i in 1..500 {
        obs.insert(i, format!("{i}"));
    }

    loop {
        let mb = mirror.borrow_and_update().await.unwrap();
        println!("original: {obs:?}");
        println!("mirrored: {mb:?}");
        if *mb == *obs {
            break;
        }
    }

    println!("remove");
    obs.remove(&10);
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
    }
}
