//! JSONL event processing - parses and transforms agent events

use crate::jsonl_parser::{JsonlParser, JsonlEvent};

/// Processes JSONL events from agent logs
pub struct EventProcessor {
    parser: JsonlParser,
}

impl EventProcessor {
    pub fn new(log_path: &str) -> Self {
        Self {
            parser: JsonlParser::new(log_path),
        }
    }

    /// Read new events since last check
    pub fn read_new_events(&mut self) -> Vec<JsonlEvent> {
        self.parser.read_new_events().unwrap_or_default()
    }
}
