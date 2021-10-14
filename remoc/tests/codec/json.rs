use remoc::codec::{Codec, Json};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestStruct {
    simple: String,
    #[serde(with = "remoc::codec::serde_map::btreemap")]
    btree: BTreeMap<Vec<u8>, String>,
    #[serde(with = "remoc::codec::serde_map::hashmap")]
    hash: HashMap<(u16, String), u8>,
}

#[test]
fn roundtrip() {
    let mut data = TestStruct { simple: "test_string".to_string(), btree: BTreeMap::new(), hash: HashMap::new() };
    data.btree.insert(vec![1, 2, 3], "first value".to_string());
    data.btree.insert(vec![4, 5, 6, 7], "second value".to_string());
    data.hash.insert((1, "one".to_string()), 10);
    data.hash.insert((2, "two".to_string()), 20);
    data.hash.insert((3, "three".to_string()), 30);
    println!("data:\n{:?}", &data);

    let mut buffer = Vec::new();
    <Json as Codec>::serialize(&mut buffer, &data).unwrap();
    println!("serialized:\n{}", String::from_utf8_lossy(&buffer));

    let deser: TestStruct = <Json as Codec>::deserialize(buffer.as_slice()).unwrap();
    assert_eq!(deser, data);
}
