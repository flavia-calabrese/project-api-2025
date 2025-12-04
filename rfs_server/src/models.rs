use serde::{Deserialize, Serialize};
use std::os::unix::fs::MetadataExt;
use std::time::SystemTime;

#[derive(Debug, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified_at: SystemTime,
    pub permissions: u32, // Permessi in formato Unix (es. 0o755)
}

impl FileEntry {
    // Metodo per convertire i metadati del file system locale in FileEntry
    pub fn from_metadata(name: String, metadata: std::fs::Metadata) -> Self {
        FileEntry {
            name,
            is_dir: metadata.is_dir(),
            size: metadata.len(),
            modified_at: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            permissions: metadata.mode(), // Metodo di estensione di Unix
        }
    }
}

use actix_web::{
    FromRequest, HttpRequest,
    dev::Payload,
    error::{ErrorBadRequest, ErrorInternalServerError, ErrorNotFound},
    http::Method,
    web,
};
use futures_util::FutureExt;
use futures_util::future::{LocalBoxFuture, ready};
use std::path::PathBuf;

// Directory Radice (o importala dal tuo modulo originale)
const ROOT_DIR: &str = "/tmp/rfs_storage";

// Il tipo che conterrà il percorso validato e canonico
pub struct SafePath(PathBuf);

// Metodo per accedere al percorso interno
impl SafePath {
    pub fn into_inner(self) -> PathBuf {
        self.0
    }
}

impl FromRequest for SafePath {
    // Il tipo di errore che l'estrattore può generare
    type Error = actix_web::Error;
    // Il futuro che risolve il nostro tipo (o un errore)
    type Future = LocalBoxFuture<'static, Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, payload: &mut Payload) -> Self::Future {
        // 1. Estrai il path grezzo da web::Path
        let path_result: Result<web::Path<PathBuf>, actix_web::Error> =
            web::Path::from_request(req, payload).into_inner();

        let root_path = PathBuf::from(ROOT_DIR);

        match path_result {
            Ok(path) => {
                let relative_path = path.into_inner();
                let mut last_part: Option<PathBuf> = None;
                dbg!(&relative_path);
                let mut full_path = root_path.clone();
                full_path.push(relative_path.as_path());

                let path_to_verify = match *req.method() {
                    Method::POST | Method::PUT => {
                        // Per POST e PUT, non verifichiamo del path completo, ma solo quella del padre prima della canonicalizzazione
                        last_part = Some(
                            relative_path
                                .file_name()
                                .map(PathBuf::from)
                                .unwrap_or_else(|| PathBuf::from("")),
                        );
                        full_path.parent().unwrap()
                    }
                    _ => {
                        // Per tutti gli altri metodi, verifichiamo l'esistenza del percorso completo
                        //relative_path = PathBuf::from("");
                        &full_path
                    }
                };

                // Se il percorso non esiste, non possiamo canonializzare (necessario per POST/PUT)
                // Per GET/LIST, usiamo la canonicalizzazione per validare la sicurezza.
                if !path_to_verify.exists() {
                    // Qui gestiamo l'errore per i percorsi non trovati (es. per LIST/GET)
                    return ready(Err(ErrorNotFound(format!(
                        "Risorsa non trovata: {:?}",
                        path_to_verify
                    ))))
                    .boxed_local();
                }

                dbg!(&path_to_verify, &full_path);

                match path_to_verify.canonicalize() {
                    Ok(canonical_path) => {
                        // 2. Controllo di sicurezza: Path Traversal
                        match root_path.canonicalize() {
                            Ok(canonical_root) => {
                                if canonical_path.starts_with(&canonical_root) {
                                    // 3. Successo: il percorso è valido e sicuro
                                    let result_path = if let Some(last_part) = last_part {
                                        canonical_path.join(last_part)
                                    } else {
                                        canonical_path
                                    };
                                    ready(Ok(SafePath(result_path))).boxed_local()
                                } else {
                                    // 4. Fallimento: Tentativo di uscire dalla ROOT_DIR
                                    ready(Err(ErrorBadRequest(
                                        "Tentativo di Path Traversal non consentito.".to_string(),
                                    )))
                                    .boxed_local()
                                }
                            }
                            Err(e) => ready(Err(ErrorInternalServerError(format!(
                                "Errore nel percorso radice: {}",
                                e
                            ))))
                            .boxed_local(),
                        }
                    }
                    Err(e) => ready(Err(ErrorInternalServerError(e))).boxed_local(),
                }
            }
            // Se l'estrazione di Path fallisce (ad es. per encoding non valido)
            Err(e) => ready(Err(e)).boxed_local(),
        }
    }
}
