use image::DynamicImage;
use mailparse::ParsedMail;

/// Find the first image/* MIME part and decode it into an `image::DynamicImage`.
pub fn first_image_from_rfc822(raw_rfc822: &[u8]) -> Option<DynamicImage> {
    let parsed = mailparse::parse_mail(raw_rfc822).ok()?;
    let bytes = find_first_image_bytes(&parsed)?;
    image::load_from_memory(&bytes).ok()
}

/// Recursively search MIME tree for first image/* part and return decoded bytes.
fn find_first_image_bytes(p: &ParsedMail) -> Option<Vec<u8>> {
    let mt = p.ctype.mimetype.to_ascii_lowercase();

    if mt.starts_with("image/") {
        // `get_body_raw()` returns decoded body bytes (transfer-encoding already handled)
        return p.get_body_raw().ok();
    }

    for sp in &p.subparts {
        if let Some(b) = find_first_image_bytes(sp) {
            return Some(b);
        }
    }

    None
}
