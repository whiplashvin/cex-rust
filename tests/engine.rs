use cex_rust::{
    engine::Engine,
    types::{AvailableMarkets, EngineError, Side},
};
use futures::channel::oneshot;

const ETH: AvailableMarkets = AvailableMarkets::ETH;
const SOL: AvailableMarkets = AvailableMarkets::SOL;

type Reply = Result<String, EngineError>;

fn call(f: impl FnOnce(oneshot::Sender<Reply>)) -> Option<Reply> {
    let (tx, mut rx) = oneshot::channel();
    f(tx);
    rx.try_recv().ok().flatten()
}

fn onramp(e: &mut Engine, user: u64, amount: u64) {
    let reply = call(|tx| e.onramp(user, amount, tx));
    assert!(matches!(reply, Some(Ok(_))));
}

fn deposit(e: &mut Engine, user: u64, market: AvailableMarkets, amount: u64) {
    let reply = call(|tx| e.deposit(user, market, amount, tx));
    assert!(matches!(reply, Some(Ok(_))));
}

fn order(e: &mut Engine, user: u64, market: AvailableMarkets, side: Side, price: u64, qty: u64) -> Option<Reply> {
    call(|tx| e.place_order(user, market, side, price, qty, tx))
}

fn usd(e: &Engine, user: u64) -> (u64, u64) {
    e.usd_balance
        .get(&user)
        .map(|b| (b.available, b.locked))
        .unwrap_or((0, 0))
}

fn stock(e: &Engine, user: u64, market: AvailableMarkets) -> (u64, u64) {
    e.stock_balance
        .get(&user)
        .and_then(|s| s.0.get(&market))
        .map(|b| (b.available, b.locked))
        .unwrap_or((0, 0))
}

fn bid_qty(e: &Engine, market: AvailableMarkets, price: u64) -> u64 {
    e.order_book[&market].bids.get(&price).map(|o| o.total_qty).unwrap_or(0)
}

fn ask_qty(e: &Engine, market: AvailableMarkets, price: u64) -> u64 {
    e.order_book[&market].asks.get(&price).map(|o| o.total_qty).unwrap_or(0)
}

#[test]
fn onramp_credits_usd() {
    let mut e = Engine::new();
    onramp(&mut e, 1, 100);
    onramp(&mut e, 1, 50);
    assert_eq!(usd(&e, 1), (150, 0));
}

#[test]
fn deposit_credits_stock_per_market() {
    let mut e = Engine::new();
    deposit(&mut e, 1, ETH, 5);
    deposit(&mut e, 1, SOL, 7);
    assert_eq!(stock(&e, 1, ETH), (5, 0));
    assert_eq!(stock(&e, 1, SOL), (7, 0));
}

#[test]
fn rejects_zero_price() {
    let mut e = Engine::new();
    onramp(&mut e, 1, 100);
    let reply = order(&mut e, 1, ETH, Side::Buy, 0, 5);
    assert!(matches!(reply, Some(Err(EngineError::InvalidAmount))));
    assert_eq!(usd(&e, 1), (100, 0));
}

#[test]
fn rejects_zero_qty() {
    let mut e = Engine::new();
    deposit(&mut e, 1, ETH, 5);
    let reply = order(&mut e, 1, ETH, Side::Sell, 10, 0);
    assert!(matches!(reply, Some(Err(EngineError::InvalidAmount))));
    assert_eq!(stock(&e, 1, ETH), (5, 0));
}

#[test]
fn rejects_buy_when_order_value_overflows() {
    let mut e = Engine::new();
    onramp(&mut e, 1, 100);
    let reply = order(&mut e, 1, ETH, Side::Buy, u64::MAX, 2);
    assert!(matches!(reply, Some(Err(EngineError::AmountTooLarge))));
    assert_eq!(usd(&e, 1), (100, 0));
}

#[test]
fn rejects_buy_without_enough_usd() {
    let mut e = Engine::new();
    onramp(&mut e, 1, 49);
    let reply = order(&mut e, 1, ETH, Side::Buy, 10, 5);
    assert!(matches!(reply, Some(Err(EngineError::InsufficientFunds))));
    assert_eq!(usd(&e, 1), (49, 0));
    assert_eq!(bid_qty(&e, ETH, 10), 0);
}

#[test]
fn rejects_sell_without_enough_stock() {
    let mut e = Engine::new();
    deposit(&mut e, 1, ETH, 4);
    let reply = order(&mut e, 1, ETH, Side::Sell, 10, 5);
    assert!(matches!(reply, Some(Err(EngineError::InsufficientFunds))));
    assert_eq!(stock(&e, 1, ETH), (4, 0));
    assert_eq!(ask_qty(&e, ETH, 10), 0);
}

#[test]
fn stock_in_one_market_cannot_be_sold_in_another() {
    let mut e = Engine::new();
    deposit(&mut e, 1, SOL, 5);
    let reply = order(&mut e, 1, ETH, Side::Sell, 10, 5);
    assert!(matches!(reply, Some(Err(EngineError::InsufficientFunds))));
}

#[test]
fn buy_with_no_asks_rests_and_locks_usd() {
    let mut e = Engine::new();
    onramp(&mut e, 1, 100);
    order(&mut e, 1, ETH, Side::Buy, 10, 5);
    assert_eq!(usd(&e, 1), (50, 50));
    assert_eq!(bid_qty(&e, ETH, 10), 5);
}

#[test]
fn sell_with_no_bids_rests_and_locks_stock() {
    let mut e = Engine::new();
    deposit(&mut e, 1, ETH, 10);
    order(&mut e, 1, ETH, Side::Sell, 10, 4);
    assert_eq!(stock(&e, 1, ETH), (6, 4));
    assert_eq!(ask_qty(&e, ETH, 10), 4);
}

#[test]
fn buy_below_best_ask_does_not_match() {
    let mut e = Engine::new();
    deposit(&mut e, 1, ETH, 5);
    onramp(&mut e, 2, 100);
    order(&mut e, 1, ETH, Side::Sell, 10, 5);
    order(&mut e, 2, ETH, Side::Buy, 9, 5);
    assert_eq!(ask_qty(&e, ETH, 10), 5);
    assert_eq!(bid_qty(&e, ETH, 9), 5);
    assert_eq!(stock(&e, 2, ETH), (0, 0));
}

#[test]
fn markets_have_separate_books() {
    let mut e = Engine::new();
    deposit(&mut e, 1, SOL, 5);
    onramp(&mut e, 2, 100);
    order(&mut e, 1, SOL, Side::Sell, 10, 5);
    order(&mut e, 2, ETH, Side::Buy, 10, 5);
    assert_eq!(ask_qty(&e, SOL, 10), 5);
    assert_eq!(bid_qty(&e, ETH, 10), 5);
    assert_eq!(stock(&e, 2, SOL), (0, 0));
}

#[test]
fn resting_orders_get_unique_ids() {
    let mut e = Engine::new();
    onramp(&mut e, 1, 100);
    deposit(&mut e, 2, ETH, 5);
    order(&mut e, 1, ETH, Side::Buy, 10, 1);
    order(&mut e, 2, ETH, Side::Sell, 20, 1);
    order(&mut e, 1, ETH, Side::Buy, 10, 1);

    let book = &e.order_book[&ETH];
    let bid_ids: Vec<u64> = book.bids[&10].queue.iter().map(|o| o.order_id).collect();
    let ask_id = book.asks[&20].queue[0].order_id;
    assert_eq!(bid_ids, vec![1, 3]);
    assert_eq!(ask_id, 2);
}

#[test]
fn buy_fully_fills_resting_ask() {
    let mut e = Engine::new();
    deposit(&mut e, 1, ETH, 5);
    onramp(&mut e, 2, 100);
    order(&mut e, 1, ETH, Side::Sell, 10, 5);
    order(&mut e, 2, ETH, Side::Buy, 10, 5);

    assert_eq!(usd(&e, 1), (50, 0));
    assert_eq!(stock(&e, 1, ETH), (0, 0));
    assert_eq!(usd(&e, 2), (50, 0));
    assert_eq!(stock(&e, 2, ETH), (5, 0));
    assert!(e.order_book[&ETH].asks.is_empty());
    assert!(e.order_book[&ETH].bids.is_empty());
}

#[test]
fn sell_fully_fills_resting_bid() {
    let mut e = Engine::new();
    onramp(&mut e, 1, 100);
    deposit(&mut e, 2, ETH, 5);
    order(&mut e, 1, ETH, Side::Buy, 10, 5);
    order(&mut e, 2, ETH, Side::Sell, 10, 5);

    assert_eq!(usd(&e, 1), (50, 0));
    assert_eq!(stock(&e, 1, ETH), (5, 0));
    assert_eq!(usd(&e, 2), (50, 0));
    assert_eq!(stock(&e, 2, ETH), (0, 0));
    assert!(e.order_book[&ETH].asks.is_empty());
    assert!(e.order_book[&ETH].bids.is_empty());
}

#[test]
fn buyer_pays_the_ask_price_not_their_limit() {
    let mut e = Engine::new();
    deposit(&mut e, 1, ETH, 5);
    onramp(&mut e, 2, 100);
    order(&mut e, 1, ETH, Side::Sell, 8, 5);
    order(&mut e, 2, ETH, Side::Buy, 10, 5);

    assert_eq!(usd(&e, 1), (40, 0));
    assert_eq!(usd(&e, 2), (60, 0));
}

#[test]
fn seller_gets_the_bid_price_not_their_limit() {
    let mut e = Engine::new();
    onramp(&mut e, 1, 100);
    deposit(&mut e, 2, ETH, 5);
    order(&mut e, 1, ETH, Side::Buy, 10, 5);
    order(&mut e, 2, ETH, Side::Sell, 8, 5);

    assert_eq!(usd(&e, 1), (50, 0));
    assert_eq!(usd(&e, 2), (50, 0));
}

#[test]
fn partial_buy_fill_rests_the_remainder() {
    let mut e = Engine::new();
    deposit(&mut e, 1, ETH, 3);
    onramp(&mut e, 2, 100);
    order(&mut e, 1, ETH, Side::Sell, 10, 3);
    order(&mut e, 2, ETH, Side::Buy, 10, 5);

    assert_eq!(stock(&e, 2, ETH), (3, 0));
    assert_eq!(usd(&e, 2), (50, 20));
    assert_eq!(bid_qty(&e, ETH, 10), 2);
    assert!(e.order_book[&ETH].asks.is_empty());
}

#[test]
fn partial_sell_fill_rests_the_remainder() {
    let mut e = Engine::new();
    onramp(&mut e, 1, 100);
    deposit(&mut e, 2, ETH, 5);
    order(&mut e, 1, ETH, Side::Buy, 10, 3);
    order(&mut e, 2, ETH, Side::Sell, 10, 5);

    assert_eq!(usd(&e, 2), (30, 0));
    assert_eq!(stock(&e, 2, ETH), (0, 2));
    assert_eq!(ask_qty(&e, ETH, 10), 2);
    assert!(e.order_book[&ETH].bids.is_empty());
}

#[test]
fn partially_filled_resting_order_keeps_its_remainder() {
    let mut e = Engine::new();
    deposit(&mut e, 1, ETH, 5);
    onramp(&mut e, 2, 100);
    order(&mut e, 1, ETH, Side::Sell, 10, 5);
    order(&mut e, 2, ETH, Side::Buy, 10, 2);

    assert_eq!(ask_qty(&e, ETH, 10), 3);
    assert_eq!(e.order_book[&ETH].asks[&10].queue[0].qty, 3);
    assert_eq!(stock(&e, 1, ETH), (0, 3));
}

#[test]
fn buy_matches_lowest_ask_first() {
    let mut e = Engine::new();
    deposit(&mut e, 1, ETH, 1);
    deposit(&mut e, 2, ETH, 1);
    onramp(&mut e, 3, 100);
    order(&mut e, 1, ETH, Side::Sell, 12, 1);
    order(&mut e, 2, ETH, Side::Sell, 10, 1);
    order(&mut e, 3, ETH, Side::Buy, 12, 1);

    assert_eq!(usd(&e, 2), (10, 0));
    assert_eq!(usd(&e, 1), (0, 0));
    assert_eq!(ask_qty(&e, ETH, 12), 1);
    assert_eq!(ask_qty(&e, ETH, 10), 0);
}

#[test]
fn sell_matches_highest_bid_first() {
    let mut e = Engine::new();
    onramp(&mut e, 1, 100);
    onramp(&mut e, 2, 100);
    deposit(&mut e, 3, ETH, 1);
    order(&mut e, 1, ETH, Side::Buy, 8, 1);
    order(&mut e, 2, ETH, Side::Buy, 10, 1);
    order(&mut e, 3, ETH, Side::Sell, 8, 1);

    assert_eq!(stock(&e, 2, ETH), (1, 0));
    assert_eq!(stock(&e, 1, ETH), (0, 0));
    assert_eq!(usd(&e, 3), (10, 0));
    assert_eq!(bid_qty(&e, ETH, 8), 1);
}

#[test]
fn same_price_fills_oldest_order_first() {
    let mut e = Engine::new();
    deposit(&mut e, 1, ETH, 1);
    deposit(&mut e, 2, ETH, 1);
    onramp(&mut e, 3, 100);
    order(&mut e, 1, ETH, Side::Sell, 10, 1);
    order(&mut e, 2, ETH, Side::Sell, 10, 1);
    order(&mut e, 3, ETH, Side::Buy, 10, 1);

    assert_eq!(usd(&e, 1), (10, 0));
    assert_eq!(usd(&e, 2), (0, 0));
    assert_eq!(e.order_book[&ETH].asks[&10].queue[0].user_id, 2);
}

#[test]
fn buy_sweeps_multiple_price_levels() {
    let mut e = Engine::new();
    deposit(&mut e, 1, ETH, 2);
    deposit(&mut e, 2, ETH, 2);
    onramp(&mut e, 3, 100);
    order(&mut e, 1, ETH, Side::Sell, 10, 2);
    order(&mut e, 2, ETH, Side::Sell, 11, 2);
    order(&mut e, 3, ETH, Side::Buy, 11, 3);

    assert_eq!(usd(&e, 3), (69, 0));
    assert_eq!(stock(&e, 3, ETH), (3, 0));
    assert_eq!(ask_qty(&e, ETH, 10), 0);
    assert_eq!(ask_qty(&e, ETH, 11), 1);
}

#[test]
fn trades_conserve_total_usd_and_stock() {
    let mut e = Engine::new();
    onramp(&mut e, 1, 500);
    onramp(&mut e, 2, 500);
    deposit(&mut e, 3, ETH, 20);
    deposit(&mut e, 4, ETH, 20);

    order(&mut e, 3, ETH, Side::Sell, 12, 7);
    order(&mut e, 1, ETH, Side::Buy, 11, 10);
    order(&mut e, 4, ETH, Side::Sell, 9, 15);
    order(&mut e, 2, ETH, Side::Buy, 13, 12);
    order(&mut e, 1, ETH, Side::Buy, 10, 4);

    let total_usd: u64 = (1..=4).map(|u| { let (a, l) = usd(&e, u); a + l }).sum();
    let total_eth: u64 = (1..=4).map(|u| { let (a, l) = stock(&e, u, ETH); a + l }).sum();
    assert_eq!(total_usd, 1000);
    assert_eq!(total_eth, 40);
}

#[test]
fn place_order_replies_on_success() {
    let mut e = Engine::new();
    onramp(&mut e, 1, 100);
    let reply = order(&mut e, 1, ETH, Side::Buy, 10, 5);
    assert!(matches!(reply, Some(Ok(_))));
}
