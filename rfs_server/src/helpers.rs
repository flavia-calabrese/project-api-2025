use actix_web::{HttpRequest, error::ErrorBadRequest, http::header};

pub fn parse_range(req: &HttpRequest, file_size: u64) -> Result<(u64, u64), actix_web::Error> {
    let range = req
        .headers()
        .get(header::RANGE)
        .ok_or_else(|| ErrorBadRequest("Missing Range header"))?
        .to_str()
        .map_err(|_| ErrorBadRequest("Invalid Range header"))?;

    //dbg!(&range);
    //dbg!(&file_size);
    if !range.starts_with("bytes=") {
        return Err(ErrorBadRequest("Invalid Range unit"));
    }

    let range = &range[6..];
    let mut parts = range.split('-');
    //dbg!(&range);
    //dbg!(&parts);
    let start: u64 = parts
        .next()
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| ErrorBadRequest("Invalid Range start"))?;

    // saturating_sub(1) restituisce 0 se il valore è 0, invece di crashare
    let file_last_byte = file_size.saturating_sub(1);

    let mut end: u64 = parts
        .next()
        .and_then(|v| v.parse().ok())
        .unwrap_or(file_last_byte);

    if end >= file_size && file_size > 0 {
        // end = file_size - 1;
        end = file_last_byte;
    }

    if file_size == 0 {
        return Ok((0,0));
    }

    /*|| end >= file_size*/
    if start > end {
        return Err(ErrorBadRequest("Invalid Range bounds"));
    }

    Ok((start, end))
}
