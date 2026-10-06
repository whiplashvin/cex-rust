use std::{collections::{BTreeMap, HashMap, VecDeque}, sync::mpsc, thread};
use futures::channel::oneshot;
use crate::types::{Actions, AvailableMarkets, CancelOrder, EngineError, FrontComm, Order, OrderBook, Orders, Side, StockBalance, Stocks, Usd};

pub fn spawn_engine() -> FrontComm {
    let (tx, rx) = mpsc::channel::<Actions>();

    thread::spawn(move || {
        let mut engine = Engine::new();

        loop {
            let Ok(actions) = rx.recv() else { break };
            match actions {
                Actions::Onramp(user_id,amount ,oneshot_tx ) => engine.onramp(user_id, amount, oneshot_tx),
                Actions::Deposit(user_id,token , amount , oneshot_tx ) => engine.deposit(user_id, token, amount, oneshot_tx),
                Actions::PlaceOrder(user_id, token, side, price, qty, oneshot_tx ) => engine.place_order(user_id, token, side, price, qty, oneshot_tx),
                Actions::CancelOrder(user_id, order_id, oneshot_tx) => engine.cancel_order(user_id, order_id, oneshot_tx),
            }
        }
    });

    FrontComm { tx }
}

pub struct Engine {
    pub usd_balance: HashMap<u64, Usd>,
    pub stock_balance: HashMap<u64, Stocks>,
    pub order_book: HashMap<AvailableMarkets, OrderBook>,
    pub order_id_to_price_lookup: HashMap<u64, CancelOrder>,
    pub next_order_id: u64
}

impl Engine {
    pub fn new() -> Self {
        let mut new_market = Engine {
            usd_balance: HashMap::new(),
            stock_balance: HashMap::new(),
            order_book: HashMap::new(),
            order_id_to_price_lookup: HashMap::new(),
            next_order_id: 0
        };

        new_market.order_book.insert(
            AvailableMarkets::ETH,
            OrderBook { 
                bids: BTreeMap::new(),
                asks: BTreeMap::new()
            }
        );
        new_market.order_book.insert(
            AvailableMarkets::SOL,
            OrderBook { 
                bids: BTreeMap::new(),
                asks: BTreeMap::new()
            }
        );
        new_market
    }

    pub fn onramp(&mut self, user_id: u64, amount: u64, oneshot_tx: oneshot::Sender<Result<String, EngineError>>){
        let balance = self.usd_balance.entry(user_id).or_insert(Usd { available: 0, locked: 0 });
        if balance.available.checked_add(balance.locked).and_then(|total| total.checked_add(amount)).is_none() {
            let _ = oneshot_tx.send(Err(EngineError::BalanceTooLarge));
            return;
        }
        balance.available += amount;

        let _ = oneshot_tx.send(Ok(String::from("onramped succesfully")));
    }

    pub fn get_usd_balance(&mut self, user_id: u64) {
        self.usd_balance.entry(user_id)
        .or_insert(Usd { available: 0, locked: 0 })
        .available;
    }

    pub fn deposit(&mut self, user_id: u64, token: AvailableMarkets, amount: u64, oneshot_tx: oneshot::Sender<Result<String, EngineError>>) {
        let balance = self.stock_balance.entry(user_id)
        .or_insert(Stocks(HashMap::new()))
        .0.entry(token)
        .or_insert(StockBalance { available: 0, locked: 0 });
        if balance.available.checked_add(balance.locked).and_then(|total| total.checked_add(amount)).is_none() {
            let _ = oneshot_tx.send(Err(EngineError::BalanceTooLarge));
            return;
        }
        balance.available += amount;

        let _ = oneshot_tx.send(Ok(String::from("deposited succesfully.")));
    }

    pub fn place_order(&mut self, user_id: u64, market: AvailableMarkets, side: Side, price: u64, qty: u64, oneshot_tx: oneshot::Sender<Result<String, EngineError>>) {
        if price == 0 || qty == 0 {
            let _ = oneshot_tx.send(Err(EngineError::InvalidAmount));
            return;
        }

        let pending = match side {
            Side::Buy => {
                let user_bal = self.usd_balance.entry(user_id).or_insert(Usd { available: 0, locked: 0 });
                let Some(to_lock) = price.checked_mul(qty) else {
                    let _ = oneshot_tx.send(Err(EngineError::AmountTooLarge));
                    return;
                };
                if user_bal.available < to_lock {
                    let _ = oneshot_tx.send(Err(EngineError::InsufficientFunds));
                    return;
                }

                user_bal.available -= to_lock;
                user_bal.locked += to_lock;

                let pending = self.match_ask(user_id, market, price, qty);

                if pending > 0 {
                    let ob = self.order_book.get_mut(&market).unwrap();
                    let new_order_id = self.next_order_id + 1;
                    let new_order = Order {
                        order_id: new_order_id,
                        user_id,
                        qty: pending
                    };
                    let existing_orders = ob.bids.entry(price).or_insert(Orders{total_qty: 0, queue: VecDeque::new()});
                    existing_orders.total_qty += pending;
                    existing_orders.queue.push_back(new_order);
                    self.order_id_to_price_lookup.insert(new_order_id, CancelOrder { market, side, price });
                    self.next_order_id = new_order_id;
                }
                self.usd_balance.get_mut(&user_id).unwrap().locked -= (qty - pending) * price;
                pending
            },
            Side::Sell => {
                let available_stocks = self.stock_balance.entry(user_id).or_insert(Stocks(HashMap::new())).0.entry(market).or_insert(StockBalance { available: 0, locked: 0 });
                if available_stocks.available < qty {
                    let _ = oneshot_tx.send(Err(EngineError::InsufficientFunds));
                    return;
                }

                available_stocks.available -= qty;
                available_stocks.locked += qty;

                let pending = self.match_buy(user_id, market, price, qty);
                if pending > 0 {
                    let ob = self.order_book.get_mut(&market).unwrap();
                    let new_order_id = self.next_order_id + 1;
                    let new_order = Order {
                        order_id: new_order_id,
                        user_id,
                        qty: pending
                    };
                    let existing_orders = ob.asks.entry(price).or_insert(Orders { total_qty: 0, queue: VecDeque::new() });
                    existing_orders.total_qty += pending;
                    existing_orders.queue.push_back(new_order);
                    self.order_id_to_price_lookup.insert(new_order_id, CancelOrder { market, side, price });
                    self.next_order_id = new_order_id;
                }
                pending
            },
        };

        let msg = if pending > 0 {
            format!("order_id {} placed: {} filled, {} resting", self.next_order_id, qty - pending, pending)
        } else {
            format!("order_id filled: {} filled", qty)
        };
        let _ = oneshot_tx.send(Ok(msg));
    }

    fn match_ask(&mut self, buyer: u64, market: AvailableMarkets, price: u64, qty: u64) -> u64 {
        let mut remaining = qty;
        let ob = self.order_book.get_mut(&market).unwrap();

        while remaining > 0 {
            let Some(mut price_level) = ob.asks.first_entry() else { break };
            let ask_price = *price_level.key();
            if ask_price > price { break };
            let orders = price_level.get_mut();     
            while remaining > 0 {
                let Some(order) = orders.queue.front_mut() else { break };
                let traded = remaining.min(order.qty);
                remaining -= traded;
                orders.total_qty -= traded;
                order.qty -= traded;

                let seller = order.user_id;
                if order.qty == 0 {
                    self.order_id_to_price_lookup.remove(&order.order_id);
                    orders.queue.pop_front();
                }

                // credit buyer's stock balance
                self.stock_balance.entry(buyer)
                .or_insert(Stocks(HashMap::new()))
                .0.entry(market)
                .or_insert(StockBalance { available: 0, locked: 0 })
                .available += traded;

                // debit seller's locked stock balance
                self.stock_balance.get_mut(&seller).unwrap()
                .0.get_mut(&market).unwrap()
                .locked -= traded;

                // credit seller's usd balance
                self.usd_balance.entry(seller)
                .or_insert(Usd { available: 0, locked: 0 })
                .available += ask_price * traded;

                // if seller's ask price is less than buyer's bid price, credit buyer's usd balance with the difference * traded qty
                self.usd_balance.get_mut(&buyer).unwrap().available += (price - ask_price) * traded;
            }      
            if orders.queue.len() == 0 {
                price_level.remove();
            }
        } remaining
    }

    fn match_buy(&mut self, seller: u64, market: AvailableMarkets, price: u64, qty: u64) -> u64{
        let mut remaining = qty;
        let ob = self.order_book.get_mut(&market).unwrap();

        while remaining > 0 {
            let Some(mut price_level) = ob.bids.last_entry() else { break };
            let bid_price = *price_level.key();

            if bid_price < price { break };

            let orders = price_level.get_mut();

            while remaining > 0 {
                let Some(order) = orders.queue.front_mut() else { break };
                let traded = remaining.min(order.qty);
                remaining -= traded;
                orders.total_qty -= traded;
                order.qty -= traded;

                let buyer = order.user_id;
                if order.qty == 0 {
                    self.order_id_to_price_lookup.remove(&order.order_id);
                    orders.queue.pop_front();
                }

                self.stock_balance.entry(buyer)
                .or_insert(Stocks(HashMap::new()))
                .0.entry(market)
                .or_insert(StockBalance { available: 0, locked: 0 })
                .available += traded;

                self.stock_balance
                .get_mut(&seller).unwrap()
                .0.get_mut(&market).unwrap()
                .locked -= traded;

                self.usd_balance.entry(seller)
                .or_insert(Usd { available: 0, locked: 0 })
                .available += bid_price * traded;

                self.usd_balance.get_mut(&buyer).unwrap().locked -= bid_price * traded;
            }
            if orders.queue.len() == 0 {
                price_level.remove();
            }
        }
        remaining
    }

    pub fn cancel_order(&mut self, user_id: u64, order_id: u64, oneshot_tx: oneshot::Sender<Result<String, EngineError>>) {
        let Some(entry) = self.order_id_to_price_lookup.get(&order_id) else {
            let _ = oneshot_tx.send(Err(EngineError::OrderNotFound));
            return;
        };
        let (market, side, price) = (entry.market, entry.side, entry.price);

        let ob = self.order_book.get_mut(&market).unwrap();
        let levels = match side {
            Side::Buy => &mut ob.bids,
            Side::Sell => &mut ob.asks,
        };

        let Some(level) = levels.get_mut(&price) else {
            let _ = oneshot_tx.send(Err(EngineError::OrderNotFound));
            return;
        };
        let Some(index) = level.queue.iter().position(|o| o.order_id == order_id) else {
            let _ = oneshot_tx.send(Err(EngineError::OrderNotFound));
            return;
        };
        if level.queue[index].user_id != user_id {
            let _ = oneshot_tx.send(Err(EngineError::NotYourOrder));
            return;
        }

        let order = level.queue.remove(index).unwrap();
        level.total_qty -= order.qty;
        if level.queue.is_empty() {
            levels.remove(&price);
        }
        self.order_id_to_price_lookup.remove(&order_id);

        match side {
            Side::Buy => {
                let balance = self.usd_balance.get_mut(&user_id).unwrap();
                balance.available += price * order.qty;
                balance.locked -= price * order.qty;
            }
            Side::Sell => {
                let balance = self.stock_balance.get_mut(&user_id).unwrap()
                .0.get_mut(&market).unwrap();
                balance.available += order.qty;
                balance.locked -= order.qty;
            }
        }

        let _ = oneshot_tx.send(Ok(String::from("order cancelled")));
    }
}