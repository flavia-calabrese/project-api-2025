use reqwest::blocking::Client;
use shared::file_entry::FileEntry;

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
}
