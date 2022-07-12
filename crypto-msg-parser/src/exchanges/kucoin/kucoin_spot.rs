use crypto_market_type::MarketType;
use crypto_msg_type::MessageType;
use chrono::Utc;
use crypto_message::{Order, OrderBookMsg, TradeMsg, TradeSide, BboMsg};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use simple_error::SimpleError;
use std::collections::HashMap;
use super::{super::utils::calc_quantity_and_volume};

use super::message::WebsocketMsg;

const EXCHANGE_NAME: &str = "kucoin";

// https://docs.kucoin.com/#match-execution-data
#[derive(Serialize, Deserialize)]
struct SpotTradeMsg {
    symbol: String,
    sequence: String,
    side: String, // buy, sell
    size: String,
    price: String,
    time: String,
    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

#[derive(Serialize, Deserialize)]
struct Changes {
    asks: Vec<[String; 3]>, //price, size, sequence
    bids: Vec<[String; 3]>,
}
// https://docs.kucoin.com/#level-2-market-data
#[derive(Serialize, Deserialize)]
#[allow(non_snake_case)]
struct SpotOrderbookMsg {
    sequenceStart: i64,
    symbol: String,
    changes: Changes,
    sequenceEnd: i64,
    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

// https://docs.kucoin.com/#level2-5-best-ask-bid-orders
#[derive(Serialize, Deserialize)]
#[allow(non_snake_case)]
struct SpotL2TopKMsg {
    timestamp: i64,
    asks: Vec<[String; 2]>,
    bids: Vec<[String; 2]>,
    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

// https://docs.kucoin.com/cn/#d1139053ae
#[derive(Serialize, Deserialize)]
#[allow(non_snake_case)]
//"data":{
//    "sequence":"1545896668986",
//    "price":"0.08",
//    "size":"0.011",
//    "bestAsk":"0.08",
//    "bestAskSize":"0.18",
//    "bestBid":"0.049",
//    "bestBidSize":"0.036"
//}
struct TickerRawMsg {
    sequence: String,
    price: String,
    size: String,
    bestAsk: String,
    bestAskSize: String,
    bestBid: String,
    bestBidSize: String,
}


pub(super) fn parse_trade(msg: &str) -> Result<Vec<TradeMsg>, SimpleError> {
    let ws_msg = serde_json::from_str::<WebsocketMsg<SpotTradeMsg>>(msg).map_err(|_e| {
        SimpleError::new(format!(
            "Failed to deserialize {} to WebsocketMsg<SpotOrderbookMsg>",
            msg
        ))
    })?;
    debug_assert_eq!(ws_msg.subject, "trade.l3match");
    debug_assert!(ws_msg.topic.starts_with("/market/match:"));
    let raw_trade = ws_msg.data;
    let price = raw_trade.price.parse::<f64>().unwrap();
    let quantity = raw_trade.size.parse::<f64>().unwrap();

    let trade = TradeMsg {
        exchange: EXCHANGE_NAME.to_string(),
        market_type: MarketType::Spot,
        symbol: raw_trade.symbol.clone(),
        pair: crypto_pair::normalize_pair(&raw_trade.symbol, EXCHANGE_NAME).ok_or_else(|| {
            SimpleError::new(format!(
                "Failed to normalize {} from {}",
                raw_trade.symbol, msg
            ))
        })?,
        msg_type: MessageType::Trade,
        timestamp: raw_trade.time.parse::<i64>().unwrap() / 1000000,
        price,
        quantity_base: quantity,
        quantity_quote: price * quantity,
        quantity_contract: None,
        side: if raw_trade.side == "sell" {
            TradeSide::Sell
        } else {
            TradeSide::Buy
        },
        trade_id: raw_trade.sequence.to_string(),
        json: msg.to_string(),
    };

    Ok(vec![trade])
}

pub(super) fn parse_l2(msg: &str, timestamp: i64) -> Result<Vec<OrderBookMsg>, SimpleError> {
    let ws_msg = serde_json::from_str::<WebsocketMsg<SpotOrderbookMsg>>(msg).map_err(|_e| {
        SimpleError::new(format!(
            "Failed to deserialize {} to WebsocketMsg<SpotOrderbookMsg>",
            msg
        ))
    })?;
    debug_assert_eq!(ws_msg.subject, "trade.l2update");
    debug_assert!(ws_msg.topic.starts_with("/market/level2:"));
    let symbol = ws_msg.data.symbol;
    let pair = crypto_pair::normalize_pair(&symbol, EXCHANGE_NAME)
        .ok_or_else(|| SimpleError::new(format!("Failed to normalize {} from {}", symbol, msg)))?;

    let parse_order = |raw_order: &[String; 3]| -> Order {
        let price = raw_order[0].parse::<f64>().unwrap();
        let quantity_base = raw_order[1].parse::<f64>().unwrap();

        Order {
            price,
            quantity_base,
            quantity_quote: price * quantity_base,
            quantity_contract: None,
        }
    };

    let orderbook = OrderBookMsg {
        exchange: EXCHANGE_NAME.to_string(),
        market_type: MarketType::Spot,
        symbol,
        pair,
        msg_type: MessageType::L2Event,
        timestamp,
        seq_id: Some(ws_msg.data.sequenceStart as u64),
        prev_seq_id: None,
        asks: ws_msg
            .data
            .changes
            .asks
            .iter()
            .map(|x| parse_order(x))
            .collect(),
        bids: ws_msg
            .data
            .changes
            .bids
            .iter()
            .map(|x| parse_order(x))
            .collect(),
        snapshot: false,
        json: msg.to_string(),
    };

    Ok(vec![orderbook])
}

pub(super) fn parse_l2_topk(msg: &str) -> Result<Vec<OrderBookMsg>, SimpleError> {
    let ws_msg = serde_json::from_str::<WebsocketMsg<SpotL2TopKMsg>>(msg).map_err(|_e| {
        SimpleError::new(format!(
            "Failed to deserialize {} to WebsocketMsg<SpotOrderbookMsg>",
            msg
        ))
    })?;
    debug_assert_eq!(ws_msg.subject, "level2");
    debug_assert!(ws_msg.topic.starts_with("/spotMarket/level2Depth5:"));
    let symbol = ws_msg.topic.split(':').last().unwrap();
    let pair = crypto_pair::normalize_pair(symbol, EXCHANGE_NAME).unwrap();
    let timestamp = ws_msg.data.timestamp;

    let parse_order = |raw_order: &[String; 2]| -> Order {
        let price = raw_order[0].parse::<f64>().unwrap();
        let quantity_base = raw_order[1].parse::<f64>().unwrap();

        Order {
            price,
            quantity_base,
            quantity_quote: price * quantity_base,
            quantity_contract: None,
        }
    };

    let orderbook = OrderBookMsg {
        exchange: EXCHANGE_NAME.to_string(),
        market_type: MarketType::Spot,
        symbol: symbol.to_string(),
        pair,
        msg_type: MessageType::L2TopK,
        timestamp,
        seq_id: None,
        prev_seq_id: None,
        asks: ws_msg.data.asks.iter().map(|x| parse_order(x)).collect(),
        bids: ws_msg.data.bids.iter().map(|x| parse_order(x)).collect(),
        snapshot: true,
        json: msg.to_string(),
    };

    Ok(vec![orderbook])
}

pub(super) fn parse_ticker(
    market_type: MarketType,
    msg: &str,
    received_at: Option<i64>,
) 
    -> Result<BboMsg, SimpleError> {
    let ws_msg = serde_json::from_str::<WebsocketMsg<TickerRawMsg>>(msg).map_err(|_e| {
        SimpleError::new(format!(
            "Failed to deserialize {} to WebsocketMsg<SpotOrderbookMsg>", msg
        ))
    })?;


    //debug_assert_eq!(ws_msg.subject, "level2");
    //debug_assert!(ws_msg.topic.starts_with("/spotMarket/level2Depth5:"));
    let mut symbol = ws_msg.subject; //.unwrap();
    if symbol == "trade.ticker" {
        if ws_msg.topic.contains(':') {
            let vec: Vec<&str> = ws_msg.topic.split(':').collect();
            symbol = vec.last().unwrap().to_string();
        } 
    }

    let pair = crypto_pair::normalize_pair(symbol.as_str(), EXCHANGE_NAME).unwrap();
    let timestamp = Utc::now().timestamp();


    let (ask_quantity_base, ask_quantity_quote, ask_quantity_contract) = calc_quantity_and_volume(
            EXCHANGE_NAME,
            market_type,
            &pair,
            ws_msg.data.bestAsk.parse::<f64>().unwrap(),
            ws_msg.data.bestAskSize.parse::<f64>().unwrap(),
        );
    
        let (bid_quantity_base, bid_quantity_quote, bid_quantity_contract) = calc_quantity_and_volume(
            EXCHANGE_NAME,
            market_type,
            &pair,
            ws_msg.data.bestBid.parse::<f64>().unwrap(),
            ws_msg.data.bestBidSize.parse::<f64>().unwrap(),
        );
    
        let bbo_msg = BboMsg {
            exchange: EXCHANGE_NAME.to_string(),
            market_type,
            symbol: symbol, //.to_string(),
            pair,
            msg_type: MessageType::BBO,
            timestamp,
            ask_price: ws_msg.data.bestAsk.parse::<f64>().unwrap(),
            ask_quantity_base,
            ask_quantity_quote,
            ask_quantity_contract,
            bid_price: ws_msg.data.bestBid.parse::<f64>().unwrap(),
            bid_quantity_base,
            bid_quantity_quote,
            bid_quantity_contract,
            id: Some(ws_msg.data.sequence.as_str().parse::<u64>().unwrap()),
            json: msg.to_string(),
        };
        Ok(bbo_msg)

}