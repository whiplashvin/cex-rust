use actix_web::{HttpResponse, Responder, get, post, web};
use chrono::{Duration, Utc};
use futures::channel::oneshot;
use jsonwebtoken::{EncodingKey, Header, encode};

use crate::{middleware::AuthUser, types::{Actions, AppState, CancelBody, Claims, DepositBody, EngineError, FrontComm, OnrampBody, ResponseBody, SignupBody, TradeBody, User}};

pub fn config(cfg: &mut web::ServiceConfig) {
    cfg
    .service(health)
    .service(signup)
    .service(signin)
    .service(onramp)
    .service(deposit)
    .service(trade)
    .service(cancel);
}

#[get("/health")]
async fn health() -> impl Responder {
    HttpResponse::Ok().json(ResponseBody{message: String::from("Healhy server")})
}

#[post("/signup")]
async fn signup(body: web::Json<SignupBody>, app_state: web::Data<AppState>) -> impl Responder {
    let mut users = app_state.users_list.lock().unwrap();
    let found = users.iter().any(|u| u.username == body.username);
    match found {
        true => HttpResponse::BadRequest().json(ResponseBody {message: String::from("User already exists")}),
        false => {
            let id = users.len() + 1;
            let new_user = User {
                id: id as u64,
                username: body.username.clone(),
                password: body.password.clone()
            };
            users.push(new_user);
            HttpResponse::Ok().json(ResponseBody{message: String::from("Signed up successfully")})
        }
    }
}

#[post("/signin")]
async fn signin(body: web::Json<SignupBody>, app_state: web::Data<AppState>) -> impl Responder {
    let users = app_state.users_list.lock().unwrap();
    let found = users.iter().find(|u| u.username == body.username && u.password == body.password);

    match found.is_none() {
        true => HttpResponse::Unauthorized().json(ResponseBody{message: String::from("Incorrect username or password")}),
        false => {
            let user = found.unwrap();
            let exp = Utc::now()
            .checked_add_signed(Duration::hours(24))
            .expect("msg")
            .timestamp() as usize;

            let claim = Claims {
                sub: user.id,
                exp
            };

            let token = encode(
                &Header::default(),
                &claim,
                &EncodingKey::from_secret(String::from("secret").as_ref())
            ).unwrap();

            HttpResponse::Ok().json(ResponseBody{message: format!("your jwt: {}", token)})
        }
    }
}

#[post("/onramp")]
async fn onramp(body: web::Json<OnrampBody>, user: AuthUser, front_comm: web::Data<FrontComm>) -> impl Responder {
    let onshot = oneshot::channel::<Result<String, EngineError>>();
    front_comm.tx.send(Actions::Onramp(user.0, body.amount, onshot.0)).unwrap();
    let res = onshot.1.await.unwrap();

    match res {
        Ok(msg) => HttpResponse::Ok().json(ResponseBody{message: msg}),
        Err(err) => HttpResponse::BadRequest().json(ResponseBody{message: format!("Error: {}", err)})
    }
}

#[post("/deposit")]
async fn deposit(body: web::Json<DepositBody>, user: AuthUser, front_comm: web::Data<FrontComm>) -> impl Responder {
    let b = body.token;
    let a = body.amount;

    let onshot = oneshot::channel::<Result<String, EngineError>>();
    front_comm.tx.send(Actions::Deposit(user.0, b, a, onshot.0)).unwrap();

    let res = onshot.1.await.unwrap();

    match res {
        Ok(msg) => HttpResponse::Ok().json(ResponseBody{message: msg}),
        Err(err) => HttpResponse::BadRequest().json(ResponseBody{message: format!("Error: {}", err)})
    }
}

#[post("/trade")]
async fn trade(body: web::Json<TradeBody>, user: AuthUser, front_comm: web::Data<FrontComm>) -> impl Responder {
    let oneshot = oneshot::channel::<Result<String, EngineError>>();
    front_comm.tx.send(Actions::PlaceOrder(user.0, body.market, body.side, body.price, body.amount, oneshot.0)).unwrap();
    let res = oneshot.1.await.unwrap();
    match res {
        Ok(msg) => HttpResponse::Ok().json(ResponseBody{message: msg}),
        Err(err) => HttpResponse::BadRequest().json(ResponseBody{message: format!("Error: {}", err)})
    }
}

#[post("/cancel")]
async fn cancel(body: web::Json<CancelBody>, user: AuthUser, front_comm: web::Data<FrontComm>) -> impl Responder {
    let oneshot = oneshot::channel::<Result<String, EngineError>>();
    front_comm.tx.send(Actions::CancelOrder(user.0, body.order_id, oneshot.0)).unwrap();
    let res = oneshot.1.await.unwrap();
    match res {
        Ok(msg) => HttpResponse::Ok().json(ResponseBody{message: msg}),
        Err(err) => {
            let body = ResponseBody{message: format!("Error: {}", err)};
            match err {
                EngineError::OrderNotFound => HttpResponse::NotFound().json(body),
                EngineError::NotYourOrder => HttpResponse::Forbidden().json(body),
                _ => HttpResponse::BadRequest().json(body),
            }
        }
    }
}
