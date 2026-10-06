use std::sync::Mutex;

use actix_web::{App, HttpServer, web};
use cex_rust::{engine::spawn_engine, routes, types::AppState};

#[actix_web::main]
async fn main() -> Result<(), std::io::Error> {
    let front_comm = web::Data::new(spawn_engine());
    let app_state = web::Data::new(AppState {
        users_list: Mutex::new(Vec::new())
    });

    let server = HttpServer::new(
        move||
        App::new()
        .app_data(app_state.clone())
        .app_data(front_comm.clone())
        .configure(routes::config)
    ).bind(("0.0.0.0", 3000))?;
    println!("Listening on port 3000");
    server.run().await
}
