pub fn decode_subject(raw: &[u8]) -> String {
    // mailparse expects a full "Key: value" header line
    let mut line = b"Subject: ".to_vec();
    line.extend_from_slice(raw);
    line.extend_from_slice(b"\r\n");

    match mailparse::parse_header(&line) {
        Ok((h, _idx)) => h.get_value(), // decodes RFC 2047
        Err(_) => String::from_utf8_lossy(raw).into_owned(),
    }
}

pub fn normalize_snippet(s: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for line in s.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(line);
        if out.chars().count() >= max_chars {
            break;
        }
    }
    out.chars().take(max_chars).collect()
}

pub fn decode_mime_words(raw: &[u8]) -> String {
    let mut line = b"X: ".to_vec();
    line.extend_from_slice(raw);
    line.extend_from_slice(b"\r\n");

    match mailparse::parse_header(&line) {
        Ok((h, _idx)) => h.get_value(),
        Err(_) => String::from_utf8_lossy(raw).into_owned(),
    }
}
