use remoc::robs::{
    vec::{ObservableVec, VecEvent},
    RecvError,
};

#[tokio::test]
async fn standalone() {
    let mut obs: ObservableVec<_, remoc::codec::Default> = ObservableVec::new();

    obs.push(0);
    obs.push(10);
    obs.push(20);
    assert_eq!(obs.pop(), Some(20));
    assert_eq!(obs.pop(), Some(10));

    obs.remove(0);
    assert_eq!(obs.len(), 0);

    obs.clear();
}

#[tokio::test]
async fn events() {
    let mut obs: ObservableVec<_, remoc::codec::Default> = ObservableVec::new();

    let mut sub = obs.subscribe(1024);
    assert!(!sub.is_complete());
    assert!(!sub.is_done());

    assert_eq!(sub.take_initial(), Some(Vec::new()));
    assert!(sub.is_complete());
    assert!(!sub.is_done());

    obs.push(0u32);
    assert_eq!(sub.recv().await.unwrap(), Some(VecEvent::Push(0)));

    obs.push(10);
    assert_eq!(sub.recv().await.unwrap(), Some(VecEvent::Push(10)));

    obs.push(20);
    assert_eq!(sub.recv().await.unwrap(), Some(VecEvent::Push(20)));

    obs.remove(0);
    assert_eq!(sub.recv().await.unwrap(), Some(VecEvent::Remove(0)));

    obs.swap_remove(0);
    assert_eq!(sub.recv().await.unwrap(), Some(VecEvent::SwapRemove(0)));

    obs.clear();
    assert_eq!(sub.recv().await.unwrap(), Some(VecEvent::Clear));

    obs.done();
    assert_eq!(sub.recv().await.unwrap(), Some(VecEvent::Done));
    assert!(sub.is_done());
}

#[tokio::test]
async fn events_incremental() {
    let hs = vec![0u32, 1, 2];
    let mut obs: ObservableVec<_, remoc::codec::Default> = ObservableVec::from(hs.clone());

    let mut sub = obs.subscribe_incremental(1024);
    assert!(!sub.is_complete());
    assert!(!sub.is_done());

    let mut hs2 = Vec::new();
    for _ in 0..3 {
        match sub.recv().await.unwrap() {
            Some(VecEvent::Push(k)) => {
                hs2.push(k);
            }
            other => panic!("unexpected event {other:?}"),
        }
    }
    assert_eq!(hs, hs2);

    assert_eq!(sub.recv().await.unwrap(), Some(VecEvent::InitialComplete));
    assert!(sub.is_complete());
    assert!(!sub.is_done());

    obs.done();
    assert_eq!(sub.recv().await.unwrap(), Some(VecEvent::Done));
    assert!(sub.is_done());
}

#[tokio::test]
async fn mirrored() {
    let mut pre = Vec::new();
    for i in 1000..1500i32 {
        pre.push(i);
    }
    let len = pre.len();

    let mut obs: ObservableVec<_, remoc::codec::Default> = ObservableVec::from(pre);
    assert_eq!(obs.len(), len);

    let sub = obs.subscribe(1024);
    let mut mirror = sub.mirror(1000);

    for i in 1..500 {
        obs.insert(5, i);
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
    obs.remove(100);
    mirror.changed().await;
    obs.shrink_to_fit();
    mirror.changed().await;

    println!("set");
    *obs.get_mut(100).unwrap() = 102;
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
    let mut obs: ObservableVec<_, remoc::codec::Default> = ObservableVec::new();

    let sub = obs.subscribe(1024);
    let mut mirror = sub.mirror(1000);

    for i in 1..500 {
        obs.insert(0, i);
    }

    println!("drop");
    drop(obs);
    mirror.changed().await;
    assert!(matches!(mirror.borrow().await, Err(RecvError::Closed)));
}

#[tokio::test]
async fn mirrored_disconnect_after_done() {
    let mut obs: ObservableVec<_, remoc::codec::Default> = ObservableVec::new();

    let sub = obs.subscribe(1024);
    let mut mirror = sub.mirror(1000);

    for i in 1..500 {
        obs.push(i);
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
    let mut pre = Vec::new();
    for i in 0..5000 {
        pre.push(i);
    }
    let len = pre.len();

    let obs: ObservableVec<_, remoc::codec::Default> = ObservableVec::from(pre);
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
