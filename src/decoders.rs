pub fn decode_subject(raw: &[u8]) -> String {
    // mailparse expects a full "Key: value" header line
    let mut line = b"Subject: ".to_vec();
    line.extend_from_slice(raw);
    line.extend_from_slice(b"\r\n");

    match mailparse::parse_header(&line) {
        Ok((h, _idx)) => h.get_value(), // decodes RFC 2047 encoded-words
        Err(_) => String::from_utf8_lossy(raw).into_owned(),
    }
}
