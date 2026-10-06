use std::{collections::{BTreeMap, HashMap, VecDeque}, fmt, sync::{Mutex, mpsc::Sender}};
use futures::channel::oneshot;
use serde::{ Deserialize, Serialize };

#[derive(Serialize)]
pub struct ResponseBody {
    pub message: String,
}

#[derive(Serialize, Clone)]
pub struct User {
    pub id: u64,
    pub username: String,
    pub password: String,
}

pub struct AppState {
    pub users_list: Mutex<Vec<User>>,
}

#[derive(Deserialize,Clone )]
pub struct SignupBody {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct OnrampBody {
    pub amount: u64
}

#[derive(Deserialize)]
pub struct DepositBody {
    pub token: AvailableMarkets,
    pub amount: u64
}

#[derive(Deserialize)]
pub struct TradeBody {
    pub market: AvailableMarkets,
    pub side: Side,
    pub price: u64,
    pub amount: u64
}

#[derive(Deserialize)]
pub struct CancelBody {
    pub order_id: u64
}

#[derive(Deserialize, Serialize)]
pub struct Claims {
    pub sub: u64,
    pub exp: usize
}

#[derive(Debug)]
pub struct Usd {
    pub available: u64,
    pub locked: u64,
}

#[derive(Deserialize, Clone, Copy, Hash, PartialEq, Eq, Debug)]
pub enum AvailableMarkets {
    ETH,
    SOL
}
#[derive(Debug)]
pub struct StockBalance {
    pub available: u64,
    pub locked: u64,
}

#[derive(Debug)]
pub struct Stocks (pub HashMap<AvailableMarkets, StockBalance>);

pub struct Order {
    pub order_id: u64,
    pub user_id:  u64,
    pub qty: u64
}

pub struct Orders {
    pub total_qty: u64,
    pub queue: VecDeque<Order>,
}

pub struct OrderBook {
    pub bids: BTreeMap<u64, Orders>,
    pub asks: BTreeMap<u64, Orders>,
}

#[derive(Deserialize, Clone, Copy)]
pub enum Side {
    Buy,
    Sell
}

pub struct CancelOrder {
    pub market: AvailableMarkets,
    pub side: Side,
    pub price: u64
}

#[derive(Clone)]
pub struct FrontComm {
    pub tx: Sender<Actions>
}

#[derive(Debug)]
pub enum EngineError {
    InsufficientFunds,
    InvalidAmount,
    AmountTooLarge,
    BalanceTooLarge,
    OrderNotFound,
    NotYourOrder,
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            EngineError::InsufficientFunds => "Insufficient funds",
            EngineError::InvalidAmount => "Price and quantity must be greater than zero",
            EngineError::AmountTooLarge => "Order value is too large",
            EngineError::BalanceTooLarge => "Balance would exceed the maximum",
            EngineError::OrderNotFound => "Order not found",
            EngineError::NotYourOrder => "Order belongs to another user",
        };
        write!(f, "{}", msg)
    }
}

pub enum Actions {
    Onramp (u64, u64, oneshot::Sender<Result<String, EngineError>>),
    Deposit (u64, AvailableMarkets, u64, oneshot::Sender<Result<String, EngineError>>),
    PlaceOrder (u64, AvailableMarkets, Side, u64, u64, oneshot::Sender<Result<String, EngineError>>),
    CancelOrder (u64, u64, oneshot::Sender<Result<String, EngineError>>)
}