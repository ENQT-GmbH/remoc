use std::time::Duration;

#[cfg(feature = "js")]
use wasm_bindgen_test::wasm_bindgen_test;

use remoc::{
    exec::time::sleep,
    robs::vec_deque::{ObservableVecDeque, VecDequeEvent},
};

#[cfg_attr(not(feature = "js"), tokio::test)]
#[cfg_attr(feature = "js", wasm_bindgen_test)]
async fn standalone() {
    let mut obs: ObservableVecDeque<_, remoc::codec::Default> = ObservableVecDeque::new();

    obs.push_back(0);
    obs.push_front(10);
    obs.push_back(20);
    assert_eq!(obs.pop_back(), Some(20));
    assert_eq!(obs.pop_front(), Some(10));

    obs.remove(0);
    assert_eq!(obs.len(), 0);

    obs.clear();
}

#[cfg_attr(not(feature = "js"), tokio::test)]
#[cfg_attr(feature = "js", wasm_bindgen_test)]
async fn events() {
    let mut obs: ObservableVecDeque<_, remoc::codec::Default> = ObservableVecDeque::new();

    let mut sub = obs.subscribe(1024);
    assert!(!sub.is_complete());
    assert!(!sub.is_done());

    use std::collections::VecDeque;
    assert_eq!(sub.take_initial(), Some(VecDeque::new()));
    assert!(sub.is_complete());
    assert!(!sub.is_done());

    obs.push_back(0u32);
    assert_eq!(sub.recv().await.unwrap(), Some(VecDequeEvent::PushBack(0)));

    obs.push_front(10);
    assert_eq!(sub.recv().await.unwrap(), Some(VecDequeEvent::PushFront(10)));

    obs.push_back(20);
    assert_eq!(sub.recv().await.unwrap(), Some(VecDequeEvent::PushBack(20)));

    assert_eq!(obs.pop_back(), Some(20));
    assert_eq!(sub.recv().await.unwrap(), Some(VecDequeEvent::PopBack));

    assert_eq!(obs.pop_front(), Some(10));
    assert_eq!(sub.recv().await.unwrap(), Some(VecDequeEvent::PopFront));

    obs.insert(0, 30);
    assert_eq!(sub.recv().await.unwrap(), Some(VecDequeEvent::Insert(0, 30)));

    *obs.get_mut(0).unwrap() = 50;
    assert_eq!(sub.recv().await.unwrap(), Some(VecDequeEvent::Set(0, 50)));

    assert_eq!(obs.remove(0), Some(50));
    assert_eq!(sub.recv().await.unwrap(), Some(VecDequeEvent::Remove(0)));

    obs.push_back(60);
    obs.push_back(70);
    assert_eq!(sub.recv().await.unwrap(), Some(VecDequeEvent::PushBack(60)));
    assert_eq!(sub.recv().await.unwrap(), Some(VecDequeEvent::PushBack(70)));

    obs.clear();
    assert_eq!(sub.recv().await.unwrap(), Some(VecDequeEvent::Clear));

    obs.done();
    assert!(obs.is_done());
    assert_eq!(sub.recv().await.unwrap(), Some(VecDequeEvent::Done));
    assert_eq!(sub.recv().await.unwrap(), None);
    assert!(sub.is_done());
}

#[cfg_attr(not(feature = "js"), tokio::test)]
#[cfg_attr(feature = "js", wasm_bindgen_test)]
async fn incremental() {
    let mut obs: ObservableVecDeque<_, remoc::codec::Default> = ObservableVecDeque::new();
    obs.push_back(0u32);
    obs.push_back(10);
    obs.push_back(20);

    let mut sub = obs.subscribe_incremental(1024);
    assert!(!sub.is_complete());
    assert!(!sub.is_done());

    assert_eq!(sub.recv().await.unwrap(), Some(VecDequeEvent::PushBack(0)));
    assert_eq!(sub.recv().await.unwrap(), Some(VecDequeEvent::PushBack(10)));
    assert_eq!(sub.recv().await.unwrap(), Some(VecDequeEvent::PushBack(20)));
    assert_eq!(sub.recv().await.unwrap(), Some(VecDequeEvent::InitialComplete));
    assert!(sub.is_complete());

    obs.push_front(30);
    assert_eq!(sub.recv().await.unwrap(), Some(VecDequeEvent::PushFront(30)));

    obs.done();
    assert_eq!(sub.recv().await.unwrap(), Some(VecDequeEvent::Done));
    assert_eq!(sub.recv().await.unwrap(), None);
}

#[cfg_attr(not(feature = "js"), tokio::test)]
#[cfg_attr(feature = "js", wasm_bindgen_test)]
async fn mirror() {
    let mut obs: ObservableVecDeque<_, remoc::codec::Default> = ObservableVecDeque::new();
    obs.push_back(0u32);
    obs.push_back(10);
    obs.push_back(20);

    let sub = obs.subscribe(1024);
    let mut mir = sub.mirror(1024);

    sleep(Duration::from_millis(10)).await;

    {
        let borrow = mir.borrow().await.unwrap();
        assert_eq!(borrow.len(), 3);
        assert_eq!(borrow[0], 0);
        assert_eq!(borrow[1], 10);
        assert_eq!(borrow[2], 20);
        assert!(borrow.is_complete());
        assert!(!borrow.is_done());
    }

    obs.push_front(30);
    mir.changed().await;

    {
        let borrow = mir.borrow().await.unwrap();
        assert_eq!(borrow.len(), 4);
        assert_eq!(borrow[0], 30);
        assert_eq!(borrow[1], 0);
        assert_eq!(borrow[2], 10);
        assert_eq!(borrow[3], 20);
    }

    obs.pop_back();
    mir.changed().await;

    {
        let borrow = mir.borrow().await.unwrap();
        assert_eq!(borrow.len(), 3);
        assert_eq!(borrow[0], 30);
        assert_eq!(borrow[1], 0);
        assert_eq!(borrow[2], 10);
    }

    obs.done();
    mir.changed().await;

    {
        let borrow = mir.borrow().await.unwrap();
        assert!(borrow.is_done());
    }

    use std::collections::VecDeque;
    let final_state = mir.detach().await;
    let mut expected = VecDeque::new();
    expected.push_back(30);
    expected.push_back(0);
    expected.push_back(10);
    assert_eq!(final_state, expected);
}

#[cfg_attr(not(feature = "js"), tokio::test)]
#[cfg_attr(feature = "js", wasm_bindgen_test)]
async fn mirror_incremental() {
    let mut obs: ObservableVecDeque<_, remoc::codec::Default> = ObservableVecDeque::new();
    obs.push_back(0u32);
    obs.push_back(10);
    obs.push_back(20);

    let sub = obs.subscribe_incremental(1024);
    let mut mir = sub.mirror(1024);

    sleep(Duration::from_millis(100)).await;

    {
        let borrow = mir.borrow().await.unwrap();
        assert_eq!(borrow.len(), 3);
        assert_eq!(borrow[0], 0);
        assert_eq!(borrow[1], 10);
        assert_eq!(borrow[2], 20);
        assert!(borrow.is_complete());
        assert!(!borrow.is_done());
    }

    obs.push_front(30);
    mir.changed().await;
    sleep(Duration::from_millis(10)).await;

    {
        let borrow = mir.borrow().await.unwrap();
        assert_eq!(borrow.len(), 4);
        assert_eq!(borrow[0], 30);
    }

    obs.done();
    mir.changed().await;
    sleep(Duration::from_millis(10)).await;

    {
        let borrow = mir.borrow().await.unwrap();
        assert!(borrow.is_done());
    }
}

#[cfg_attr(not(feature = "js"), tokio::test)]
#[cfg_attr(feature = "js", wasm_bindgen_test)]
async fn swap_remove() {
    let mut obs: ObservableVecDeque<_, remoc::codec::Default> = ObservableVecDeque::new();

    let mut sub = obs.subscribe(1024);
    assert_eq!(sub.take_initial(), Some(std::collections::VecDeque::new()));

    obs.push_back(0u32);
    obs.push_back(10);
    obs.push_back(20);
    obs.push_back(30);

    assert_eq!(sub.recv().await.unwrap(), Some(VecDequeEvent::PushBack(0)));
    assert_eq!(sub.recv().await.unwrap(), Some(VecDequeEvent::PushBack(10)));
    assert_eq!(sub.recv().await.unwrap(), Some(VecDequeEvent::PushBack(20)));
    assert_eq!(sub.recv().await.unwrap(), Some(VecDequeEvent::PushBack(30)));

    // Test swap_remove_back - it should remove element at index 1 (value 10) and replace with last element (30)
    assert_eq!(obs.swap_remove_back(1), Some(10));
    assert_eq!(sub.recv().await.unwrap(), Some(VecDequeEvent::SwapRemoveBack(1)));

    // After swap_remove_back(1), the deque should be [0, 30, 20]
    // Test swap_remove_front - it should remove element at index 1 (value 30) and replace with first element (0)
    assert_eq!(obs.swap_remove_front(1), Some(30));
    assert_eq!(sub.recv().await.unwrap(), Some(VecDequeEvent::SwapRemoveFront(1)));
}

#[cfg_attr(not(feature = "js"), tokio::test)]
#[cfg_attr(feature = "js", wasm_bindgen_test)]
async fn pop_operations_comprehensive() {
    let mut obs: ObservableVecDeque<_, remoc::codec::Default> = ObservableVecDeque::new();
    let mut sub = obs.subscribe(1024);

    use std::collections::VecDeque;
    assert_eq!(sub.take_initial(), Some(VecDeque::new()));

    // Test popping from empty deque
    assert_eq!(obs.pop_front(), None);
    assert_eq!(obs.pop_back(), None);
    // No events should be sent for failed pops

    // Add some elements
    obs.push_back(10);
    obs.push_back(20);
    obs.push_front(5);
    obs.push_front(1);
    // Deque is now: [1, 5, 10, 20]

    assert_eq!(sub.recv().await.unwrap(), Some(VecDequeEvent::PushBack(10)));
    assert_eq!(sub.recv().await.unwrap(), Some(VecDequeEvent::PushBack(20)));
    assert_eq!(sub.recv().await.unwrap(), Some(VecDequeEvent::PushFront(5)));
    assert_eq!(sub.recv().await.unwrap(), Some(VecDequeEvent::PushFront(1)));

    // Test pop_front
    assert_eq!(obs.pop_front(), Some(1));
    assert_eq!(sub.recv().await.unwrap(), Some(VecDequeEvent::PopFront));
    assert_eq!(obs.len(), 3);

    // Test pop_back
    assert_eq!(obs.pop_back(), Some(20));
    assert_eq!(sub.recv().await.unwrap(), Some(VecDequeEvent::PopBack));
    assert_eq!(obs.len(), 2);

    // Continue alternating pops
    assert_eq!(obs.pop_front(), Some(5));
    assert_eq!(sub.recv().await.unwrap(), Some(VecDequeEvent::PopFront));
    assert_eq!(obs.len(), 1);

    assert_eq!(obs.pop_back(), Some(10));
    assert_eq!(sub.recv().await.unwrap(), Some(VecDequeEvent::PopBack));
    assert_eq!(obs.len(), 0);

    // Test popping from empty again
    assert_eq!(obs.pop_front(), None);
    assert_eq!(obs.pop_back(), None);

    obs.done();
    assert_eq!(sub.recv().await.unwrap(), Some(VecDequeEvent::Done));
}

#[cfg_attr(not(feature = "js"), tokio::test)]
#[cfg_attr(feature = "js", wasm_bindgen_test)]
async fn pop_operations_single_element() {
    let mut obs: ObservableVecDeque<_, remoc::codec::Default> = ObservableVecDeque::new();
    let mut sub = obs.subscribe(1024);

    use std::collections::VecDeque;
    assert_eq!(sub.take_initial(), Some(VecDeque::new()));

    // Test pop_front on single element
    obs.push_back(42);
    assert_eq!(sub.recv().await.unwrap(), Some(VecDequeEvent::PushBack(42)));

    assert_eq!(obs.pop_front(), Some(42));
    assert_eq!(sub.recv().await.unwrap(), Some(VecDequeEvent::PopFront));
    assert_eq!(obs.len(), 0);

    // Test pop_back on single element
    obs.push_front(99);
    assert_eq!(sub.recv().await.unwrap(), Some(VecDequeEvent::PushFront(99)));

    assert_eq!(obs.pop_back(), Some(99));
    assert_eq!(sub.recv().await.unwrap(), Some(VecDequeEvent::PopBack));
    assert_eq!(obs.len(), 0);

    obs.done();
    assert_eq!(sub.recv().await.unwrap(), Some(VecDequeEvent::Done));
}

#[cfg_attr(not(feature = "js"), tokio::test)]
#[cfg_attr(feature = "js", wasm_bindgen_test)]
async fn pop_operations_mirror() {
    let mut obs: ObservableVecDeque<_, remoc::codec::Default> = ObservableVecDeque::new();

    // Pre-populate with some data
    obs.push_back(10);
    obs.push_back(20);
    obs.push_front(5);
    // Deque is now: [5, 10, 20]

    let sub = obs.subscribe(1024);
    let mut mir = sub.mirror(1024);

    sleep(Duration::from_millis(10)).await;

    // Verify initial state
    {
        let borrow = mir.borrow().await.unwrap();
        assert_eq!(borrow.len(), 3);
        assert_eq!(borrow[0], 5);
        assert_eq!(borrow[1], 10);
        assert_eq!(borrow[2], 20);
    }

    // Test pop_front
    assert_eq!(obs.pop_front(), Some(5));
    mir.changed().await;

    {
        let borrow = mir.borrow().await.unwrap();
        assert_eq!(borrow.len(), 2);
        assert_eq!(borrow[0], 10);
        assert_eq!(borrow[1], 20);
    }

    // Test pop_back
    assert_eq!(obs.pop_back(), Some(20));
    mir.changed().await;

    {
        let borrow = mir.borrow().await.unwrap();
        assert_eq!(borrow.len(), 1);
        assert_eq!(borrow[0], 10);
    }

    // Pop the last element
    assert_eq!(obs.pop_front(), Some(10));
    mir.changed().await;

    {
        let borrow = mir.borrow().await.unwrap();
        assert_eq!(borrow.len(), 0);
        assert!(borrow.is_empty());
    }

    // Add more elements and test different patterns
    obs.push_back(100);
    obs.push_front(50);
    obs.push_back(150);
    mir.changed().await;

    {
        let borrow = mir.borrow().await.unwrap();
        assert_eq!(borrow.len(), 3);
        assert_eq!(borrow[0], 50);
        assert_eq!(borrow[1], 100);
        assert_eq!(borrow[2], 150);
    }

    obs.done();
    mir.changed().await;

    use std::collections::VecDeque;
    let final_state = mir.detach().await;
    let mut expected = VecDeque::new();
    expected.push_back(50);
    expected.push_back(100);
    expected.push_back(150);
    assert_eq!(final_state, expected);
}

#[cfg_attr(not(feature = "js"), tokio::test)]
#[cfg_attr(feature = "js", wasm_bindgen_test)]
async fn pop_operations_stress_test() {
    let mut obs: ObservableVecDeque<_, remoc::codec::Default> = ObservableVecDeque::new();
    let mut sub = obs.subscribe(2048);

    use std::collections::VecDeque;
    assert_eq!(sub.take_initial(), Some(VecDeque::new()));

    // Stress test with many push/pop operations
    let mut expected_events = Vec::new();

    // Fill the deque
    for i in 0..10 {
        obs.push_back(i);
        expected_events.push(VecDequeEvent::PushBack(i));
    }

    for i in 10..20 {
        obs.push_front(i);
        expected_events.push(VecDequeEvent::PushFront(i));
    }

    // Now pop elements in various patterns
    // Pop from front 5 times
    for _ in 0..5 {
        let val = obs.pop_front();
        assert!(val.is_some());
        expected_events.push(VecDequeEvent::PopFront);
    }

    // Pop from back 3 times
    for _ in 0..3 {
        let val = obs.pop_back();
        assert!(val.is_some());
        expected_events.push(VecDequeEvent::PopBack);
    }

    // Add a few more elements
    obs.push_back(100);
    obs.push_front(200);
    expected_events.push(VecDequeEvent::PushBack(100));
    expected_events.push(VecDequeEvent::PushFront(200));

    // Pop remaining elements alternating
    while obs.len() > 1 {
        if obs.len() % 2 == 0 {
            let val = obs.pop_front();
            assert!(val.is_some());
            expected_events.push(VecDequeEvent::PopFront);
        } else {
            let val = obs.pop_back();
            assert!(val.is_some());
            expected_events.push(VecDequeEvent::PopBack);
        }
    }

    // Pop the last element
    if obs.len() > 0 {
        let val = obs.pop_front();
        assert!(val.is_some());
        expected_events.push(VecDequeEvent::PopFront);
    }

    assert_eq!(obs.len(), 0);

    // Verify all events were received correctly
    for expected_event in expected_events {
        let received_event = sub.recv().await.unwrap();
        assert_eq!(received_event, Some(expected_event));
    }

    obs.done();
    assert_eq!(sub.recv().await.unwrap(), Some(VecDequeEvent::Done));
}
