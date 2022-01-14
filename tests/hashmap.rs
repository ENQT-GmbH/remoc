use remoc_obs::hashmap::ObservableHashMap;


#[tokio::test]
async fn unobserved() {
    let mut obs: ObservableHashMap<_, _, remoc::codec::Default> = ObservableHashMap::new();

    obs.insert(10, "10".to_string()).unwrap();
    obs.insert(20, "20".to_string()).unwrap();
    assert_eq!(obs.get(&10), Some(&"10".to_string()));
    assert_eq!(obs.get(&20), Some(&"20".to_string()));

    obs.clear();
}
