use std::io::SeekFrom;

use actix_web::{
    HttpRequest, HttpResponse,
    error::{ErrorBadRequest, ErrorInternalServerError},
    http::header,
    web::{Json, Query},
};
use serde::Deserialize;
use shared::file_entry::FileEntry;
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
};

use crate::{helpers::parse_range, models::*};
use actix_web::web::{Bytes, Payload};
use tokio_util::codec::{BytesCodec, FramedRead};

// Importa i tratti necessari per lo streaming e la gestione degli errori asincroni
use futures_util::stream::{StreamExt, TryStreamExt};

// Directory Radice
pub const ROOT_DIR: &str = "/tmp/rfs_storage";

// --- GET /list/{nome_file} ---
pub async fn list_directory(safe_path: SafePath) -> Result<HttpResponse, actix_web::Error> {
    let full_path = safe_path.into_inner();

    if !fs::metadata(&full_path).await?.is_dir() {
        return Err(ErrorBadRequest(
            "Il percorso specificato non è una directory.",
        ));
    }

    let mut entries = Vec::new();

    match fs::read_dir(&full_path).await {
        Ok(mut dir) => {
            while let Some(entry) = dir.next_entry().await.transpose() {
                match entry {
                    Ok(entry) => {
                        let file_name = entry.file_name().to_string_lossy().into_owned();
                        if let Ok(metadata) = entry.metadata().await {
                            let file_entry = FileEntry::from_metadata(file_name, metadata.into());
                            entries.push(file_entry);
                        }
                    }
                    Err(e) => eprintln!("Errore nella lettura dell'elemento: {}", e),
                }
            }
            Ok(HttpResponse::Ok().json(entries))
        }
        Err(e) => {
            eprintln!("Errore di I/O nella directory: {}", e);
            Err(ErrorInternalServerError(format!(
                "Errore I/O server: {}",
                e
            )))
        }
    }
}

// ---  GET /files/{nome_file} (Lettura in Streaming) ---
pub async fn read_file_contents(
    req: HttpRequest,
    path: SafePath,
) -> Result<HttpResponse, actix_web::Error> {
    let full_path = path.into_inner();
    // dbg!("read_file_contents", &full_path);

    let metadata = fs::metadata(&full_path).await?;
    if !metadata.is_file() {
        return Err(ErrorBadRequest("Il percorso non è un file"));
    }
    let file_size = metadata.len();

    let (start, end) = parse_range(&req, file_size)?;
    let length = end - start + 1;

    let mut file = match fs::File::open(&full_path).await {
        Ok(f) => f,
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                return Ok(
                    HttpResponse::NotFound().body(format!("File non trovato: {:?}", full_path))
                );
            }
            return Err(ErrorInternalServerError(format!(
                "Errore di apertura file: {}",
                e
            )));
        }
    };

    file.seek(SeekFrom::Start(start)).await?;

    let stream = FramedRead::new(file.take(length), BytesCodec::new())
        .map_ok(|bytes| Bytes::from(bytes.freeze()))
        .map_err(|e| ErrorInternalServerError(e));

    Ok(HttpResponse::PartialContent()
        .insert_header((
            header::CONTENT_RANGE,
            format!("bytes {}-{}/{}", start, end, file_size),
        ))
        .insert_header((header::ACCEPT_RANGES, "bytes"))
        .insert_header((header::CONTENT_LENGTH, length))
        .content_type("application/octet-stream")
        .streaming(stream))
}

// --- PUT /files/{nome_file} (Scrittura in Streaming) ---
pub async fn write_file_contents(
    path: SafePath,
    mut payload: Payload,
) -> Result<HttpResponse, actix_web::Error> {
    let full_path = path.into_inner();
    // dbg!(&full_path);
    let mut file = match fs::File::create(&full_path).await {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Errore nella creazione del file {:?}: {}", full_path, e);
            return Err(ErrorInternalServerError(format!(
                "Errore I/O server: {}",
                e
            )));
        }
    };

    while let Some(chunk) = payload.next().await {
        match chunk {
            Ok(bytes) => {
                if let Err(e) = file.write_all(bytes.as_ref()).await {
                    eprintln!("Errore di scrittura su file {:?}: {}", full_path, e);
                    return Err(ErrorInternalServerError(format!(
                        "Errore di scrittura: {}",
                        e
                    )));
                }
            }
            Err(e) => {
                eprintln!("Errore nella lettura del payload HTTP: {}", e);
                return Err(ErrorInternalServerError(
                    "Errore di streaming HTTP".to_string(),
                ));
            }
        }
    }

    Ok(HttpResponse::Created().body(format!("File creato/aggiornato: {:?}", full_path)))
}

// --- POST /mkdir/{nome_directory} (Crea Directory) ---
pub async fn create_directory(path: SafePath) -> Result<HttpResponse, actix_web::Error> {
    let full_path = path.into_inner();
    // dbg!(&full_path);
    match fs::create_dir_all(&full_path).await {
        Ok(_) => Ok(HttpResponse::Created().body(format!("Directory creata: {:?}", full_path))),
        Err(e) => {
            eprintln!(
                "Errore nella creazione della directory {:?}: {}",
                full_path, e
            );
            Err(ErrorInternalServerError(format!(
                "Errore I/O server: {}",
                e
            )))
        }
    }
}

// --- DELETE /files/{nome_file} ---
pub async fn delete_file_or_directory(path: SafePath) -> Result<HttpResponse, actix_web::Error> {
    let full_path = path.into_inner();
    // dbg!(&full_path);
    let result = if fs::metadata(&full_path).await?.is_dir() {
        fs::remove_dir_all(full_path).await
    } else {
        fs::remove_file(full_path).await
    };
    match result {
        Ok(_) => Ok(HttpResponse::Ok().body("Eliminazione avvenuta con successo.")),
        Err(e) => {
            eprintln!("Errore durante l'eliminazione: {}", e);
            Err(ErrorInternalServerError(format!(
                "Errore I/O server: {}",
                e
            )))
        }
    }
}

#[derive(Deserialize)]
pub struct RenameRequest {
    new_name: String,
}

// --- PATCH /files/{nome_file}
pub async fn rename_file_or_directory(
    path: SafePath,
    body: Json<RenameRequest>,
) -> Result<HttpResponse, actix_web::Error> {
    let full_path = path.into_inner();
    let new_name = &body.new_name;

    let parent = full_path.parent().ok_or_else(|| {
        actix_web::error::ErrorBadRequest("Impossibile determinare la cartella del file")
    })?;

    // Costruisci il percorso completo del nuovo file
    let new_path = parent.join(new_name);

    // Esegui il rename
    let result = fs::rename(&full_path, &new_path).await;

    match result {
        Ok(_) => Ok(HttpResponse::Ok().body(format!("File rinominato in {}", new_path.display()))),
        Err(e) => {
            eprintln!("Errore durante il renaming: {}", e);
            Err(ErrorInternalServerError(format!(
                "Errore I/O server: {}",
                e
            )))
        }
    }
}
