//! SSE decoder keeps bytes until complete lines, so split UTF-8 tokens stay intact.
use serde_json::Value;
#[derive(Default)]
pub struct Decoder {
    buffer: Vec<u8>,
    data: Vec<String>,
}
impl Decoder {
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<Value>, String> {
        self.buffer.extend_from_slice(bytes);
        if self.buffer.len() > 1024 * 1024 {
            return Err("Sự kiện streaming quá lớn".into());
        }
        let mut events = Vec::new();
        while let Some(end) = self.buffer.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = self.buffer.drain(..=end).collect();
            let line = std::str::from_utf8(&line)
                .map_err(|_| "Streaming UTF-8 không hợp lệ")?
                .trim_end_matches(['\r', '\n']);
            if line.is_empty() {
                if !self.data.is_empty() {
                    let text = self.data.join("\n");
                    self.data.clear();
                    events.push(
                        serde_json::from_str(&text)
                            .map_err(|_| "Sự kiện streaming không hợp lệ")?,
                    );
                }
            } else if let Some(data) = line.strip_prefix("data:") {
                self.data
                    .push(data.strip_prefix(' ').unwrap_or(data).to_owned());
                if self.data.iter().map(String::len).sum::<usize>() > 1024 * 1024 {
                    return Err("Sự kiện streaming quá lớn".into());
                }
            }
        }
        Ok(events)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn split_unicode_and_crlf() {
        let bytes =
            "event: content_block_delta\r\ndata: {\"delta\":{\"text\":\"chào 🌱\"}}\r\n\r\n"
                .as_bytes();
        let mut decoder = Decoder::default();
        let mut output = vec![];
        for byte in bytes {
            output.extend(decoder.push(&[*byte]).unwrap());
        }
        assert_eq!(output.len(), 1);
        assert_eq!(output[0]["delta"]["text"], "chào 🌱");
    }
    #[test]
    fn multiple_events_and_comments() {
        let mut decoder = Decoder::default();
        assert_eq!(decoder.push(b":keepalive\n\ndata: {\"type\":\"ping\"}\n\ndata: {\"type\":\"message_stop\"}\n\n").unwrap().len(),2);
    }
    #[test]
    fn malformed_event_is_error() {
        assert!(Decoder::default().push(b"data: invalid\n\n").is_err());
    }
}
