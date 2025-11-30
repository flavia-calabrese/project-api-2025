// src/api/mod.rs

use actix_web::web;

pub mod handlers; // Rende visibile il modulo handlers

/// Configurazione delle rotte RESTful per il File System
pub fn config_routes(cfg: &mut web::ServiceConfig) {
    // GET /list/{tail:.*}
    cfg.route("/list/{tail:.*}", web::get().to(handlers::list_directory));
    
    // GET /files/{tail:.*} (Lettura)
    cfg.route("/files/{tail:.*}", web::get().to(handlers::read_file_contents));

    // PUT /files/{tail:.*} (Scrittura/Sovrascrittura)
    cfg.route("/files/{tail:.*}", web::put().to(handlers::write_file_contents));

    // Rotte da aggiungere: POST /mkdir, DELETE /files
}