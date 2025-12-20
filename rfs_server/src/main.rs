// src/main.rs

use actix_web::{App, HttpServer, web};
use tokio::fs;

mod api; // Dichiara il modulo API (che contiene i handlers)
mod helpers;
mod models; // Dichiara il modulo per le strutture dati

// La funzione principale per avviare il server
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Assicura che la directory radice esista all'avvio
    fs::create_dir_all(api::handlers::ROOT_DIR).await?;

    println!("Server RFS avviato su http://127.0.0.1:8080");
    println!("Directory radice: {}", api::handlers::ROOT_DIR);

    HttpServer::new(|| {
        App::new()
            // Inizializza lo scope /api e usa la configurazione delle rotte del modulo api
            .service(web::scope("/api").configure(api::config_routes))
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
