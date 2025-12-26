// use libc::off64_t;
use reqwest::{StatusCode, blocking::Client};
use shared::file_entry::FileEntry;

#[derive(Debug, Clone)]
pub struct Api {
    pub base_url: String,
    pub client: Client,
}

impl Api {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            base_url: "http://127.0.0.1:8080/api/".to_string(),
        }
    }

    pub fn list_dir(&self, path: &str) -> reqwest::Result<Vec<FileEntry>> {
        assert!(path.starts_with('/'));
        let url = format!("{}list{}", self.base_url, path);
        let resp = self.client.get(&url).send()?.json::<Vec<FileEntry>>()?;
        Ok(resp)
    }

    pub fn read_file_contents(
        &self,
        path: &str,
        offset: u64,
        size: u32,
    ) -> Result<Vec<u8>, std::io::Error> {
        
        // se size == 0, non devo leggere nulla
        if size == 0 {
            return Ok(Vec::new());
        }

        let end = offset + size as u64 - 1;
        let range = format!("bytes={}-{}", offset, end);
        
        // check no "//"
        let clean_path = path.trim_start_matches('/');

        let url = format!("{}files/{}", self.base_url, clean_path);
        //dbg!(&url);
        //dbg!(&range);

        let resp = self
            .client
            .get(url)
            .header(reqwest::header::RANGE, range)
            .send()
            .map_err(|_| std::io::ErrorKind::Other)?;

        match resp.status() {
            StatusCode::PARTIAL_CONTENT => resp
                .bytes()
                .map(|x| x.to_vec())
                .map_err(|_| std::io::ErrorKind::Other.into()),
            StatusCode::NOT_FOUND => Err(std::io::ErrorKind::NotFound.into()),
            _ => Err(std::io::ErrorKind::Other.into()),
        }
    }

    pub fn write_file_contents(
        &self, 
        path: &str, 
        data: Vec<u8>
    ) -> Result<(), std::io::Error> {
        let url = format!("{}files{}", self.base_url, path);
    
        let resp = self.client.put(url)
            .body(data)
            .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
            .send()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

        if resp.status().is_success() {
            Ok(())
        } else {
            println!("il server ha risposto con errore: {}", resp.status());
            Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "Errore scrittura server"))
        }
    }

    // pub fn delete_file(&self, path: &str) -> reqwest::Result<()> {
    //     let url = format!("{}files{}", self.base_url, path);
    //     let resp = self.client.delete(&url).send()?;
    //     if resp.status().is_success() { Ok(()) } else { Err(resp.error_for_status().unwrap_err()) }
    // }

    pub fn create_directory(&self, path: &str) -> reqwest::Result<()> {
        let clean_path = path.trim_start_matches('/');

        let url = format!("{}mkdir/{}", self.base_url, clean_path);

        let resp = self.client.post(&url).send()?;

        if resp.status().is_success() { 
            Ok(()) 
        } else { 
            Err(resp.error_for_status().unwrap_err()) 
        }
    }

    pub fn rename_entry(&self, old_path: &str, new_name: &str) -> reqwest::Result<()> {
        let clean_old_path = old_path.trim_start_matches('/');
        let url = format!("{}files/{}", self.base_url, clean_old_path);
        
        // Il server si aspetta un JSON con { "new_name": "..." }
        let body = serde_json::json!({ "new_name": new_name });

        let resp = self.client.patch(&url)
            .json(&body)
            .send()?;
        
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(resp.error_for_status().unwrap_err())
        }
    }
}
