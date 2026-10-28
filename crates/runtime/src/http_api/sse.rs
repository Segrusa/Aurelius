//! Minimal Server-Sent Events parser: accumulates bytes, yields complete events.

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Event {
    pub event: Option<String>,
    pub data: String,
}

#[derive(Default)]
pub struct Parser {
    buffer: String,
}

/// Upper bound for one unterminated event (bounded memory).
const MAX_PENDING: usize = 4 * 1024 * 1024;

impl Parser {
    pub fn push(&mut self, bytes: &[u8]) -> Vec<Event> {
        self.buffer.push_str(&String::from_utf8_lossy(bytes));
        if self.buffer.len() > MAX_PENDING {
            self.buffer.clear();
            return vec![Event { event: Some("error".into()), data: r#"{"type":"error","error":{"type":"stream_too_large"}}"#.into() }];
        }
        let normalized = self.buffer.replace("\r\n", "\n");
        let mut events = Vec::new();
        let mut rest = normalized.as_str();
        while let Some(end) = rest.find("\n\n") {
            let block = &rest[..end];
            rest = &rest[end + 2..];
            let mut event = Event::default();
            let mut data = Vec::new();
            for line in block.lines() {
                if let Some(v) = line.strip_prefix("event:") {
                    event.event = Some(v.trim().to_owned());
                } else if let Some(v) = line.strip_prefix("data:") {
                    data.push(v.strip_prefix(' ').unwrap_or(v).to_owned());
                }
            }
            event.data = data.join("\n");
            if event.event.is_some() || !event.data.is_empty() {
                events.push(event);
            }
        }
        self.buffer = rest.to_owned();
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_split_across_chunks() {
        let mut p = Parser::default();
        assert!(p.push(b"event: a\ndata: {\"x\"").is_empty());
        let events = p.push(b":1}\n\nevent: b\r\ndata: 2\r\n\r\n");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event.as_deref(), Some("a"));
        assert_eq!(events[0].data, "{\"x\":1}");
        assert_eq!(events[1].data, "2");
    }
}
