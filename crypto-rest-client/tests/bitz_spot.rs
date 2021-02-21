use std::collections::HashMap;

use crypto_rest_client::BitzSpotRestClient;
use serde_json::Value;

#[test]
fn test_l2_snapshot() {
    let text = BitzSpotRestClient::fetch_l2_snapshot("btc_usdt").unwrap();
    let obj = serde_json::from_str::<HashMap<String, Value>>(&text).unwrap();

    assert_eq!(obj.get("status").unwrap().as_i64().unwrap(), 200);

    let data = obj.get("data").unwrap().as_object().unwrap();
    assert!(data.get("asks").unwrap().as_array().unwrap().len() > 0);
    assert!(data.get("bids").unwrap().as_array().unwrap().len() > 0);
}
