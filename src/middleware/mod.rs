use core::fmt;
use std::future::{Ready, ready};

use actix_web::{Error, FromRequest, HttpResponse, ResponseError};
use jsonwebtoken::{DecodingKey, Validation, decode};

use crate::types::{Claims, ResponseBody};

#[derive(Debug)]
pub struct AuthErr;

impl fmt::Display for AuthErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Invalid or missing tokens")
    }
}

impl ResponseError for AuthErr {
    fn error_response(&self) -> actix_web::HttpResponse<actix_web::body::BoxBody> {
        HttpResponse::Unauthorized().json(ResponseBody{message: String::from("Invalid or missing tokens")})
    }
}

pub struct AuthUser (pub u64);

impl FromRequest for AuthUser {
    type Error = Error;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &actix_web::HttpRequest, _payload: &mut actix_web::dev::Payload) -> Self::Future {
        let tokens = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.trim_start_matches("Bearer").trim().to_string());

        let token = match tokens {
            Some(t) => t,
            None => return  ready(Err(AuthErr.into()))
        };

        let decoded = decode::<Claims>(
            token, 
            &DecodingKey::from_secret(String::from("secret").as_ref()), 
            &Validation::default()
        );

        match decoded {
            Ok(data) => ready(Ok(AuthUser(data.claims.sub))),
            Err(_) => ready(Err(AuthErr.into()))
        }
    }
}

