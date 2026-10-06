# cex-rust

An in-memory order book and matching engine for a centralized exchange.

The engine is the core of the project. The HTTP layer (actix-web) is a thin wrapper that exists to drive it.

## Engine

- A single thread owns all exchange state: balances, order books, and the order lookup. Nothing is shared, so the book needs no locks.
- HTTP handlers send an `Actions` message over an `mpsc` channel and wait for the result on a `oneshot` channel.
- Orders are limit orders with price-time priority. Price levels are stored in a `BTreeMap`, and orders at the same price are filled first-in, first-out from a `VecDeque`.
- Funds are locked when an order is placed: USD for buys, the asset for sells. A buy that fills below its limit price is refunded the difference.
- Any unfilled quantity rests on the book and can be cancelled. Cancelling unlocks the unfilled remainder.
- Markets: `ETH` and `SOL`. Prices and quantities are `u64` integers; there are no decimals.

Source layout:

| Path             | Contents                               |
| ---------------- | -------------------------------------- |
| `src/engine`     | Matching engine and the engine thread  |
| `src/types`      | Shared types, `Actions`, `EngineError` |
| `src/routes`     | HTTP handlers                          |
| `src/middleware` | JWT extractor                          |

## HTTP API

All routes except `/health`, `/signup` and `/signin` require an `Authorization: Bearer <token>` header.

| Method | Path       | Body                                                           |
| ------ | ---------- | -------------------------------------------------------------- |
| GET    | `/health`  |                                                                |
| POST   | `/signup`  | `{ "username": "...", "password": "..." }`                     |
| POST   | `/signin`  | `{ "username": "...", "password": "..." }`                     |
| POST   | `/onramp`  | `{ "amount": 1000 }`                                           |
| POST   | `/deposit` | `{ "token": "ETH", "amount": 5 }`                              |
| POST   | `/trade`   | `{ "market": "ETH", "side": "Buy", "price": 10, "amount": 5 }` |
| POST   | `/cancel`  | `{ "order_id": 3 }`                                            |

`/trade` returns the order ID when part of the order rests on the book. That ID is what `/cancel` expects.

## Running

```
cargo run      # listens on 0.0.0.0:3000
cargo test     # engine tests
```

## Limitations

This is a learning project and is not suitable for real funds.

- **No persistence.** Balances, order books and users exist only in memory. There is no snapshot or event log, so a crash or restart loses everything.
- **Single point of failure.** If the engine thread panics, the exchange stops serving requests.
- **Passwords are stored in plain text** and compared directly. They are not hashed.
- **The JWT secret is hardcoded** as `"secret"`.
- **Onramp and deposit are unverified.** Any authenticated user can credit themselves any amount.
- **No read endpoints.** Balances, open orders and the order book cannot be queried over HTTP.
