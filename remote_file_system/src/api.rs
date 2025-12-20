use libc::off64_t;
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
        let end = offset + size as u64 - 1;
        let range = format!("bytes={}-{}", offset, end);

        let url = format!("{}{}{}", self.base_url, "files", path);
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
}
