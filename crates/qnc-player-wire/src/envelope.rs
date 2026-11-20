//! The session envelopes: the player accepts a command only with its session id, source
//! generation and a sequence above the last; it answers with events under the same checks.

use serde::{Deserialize, Serialize};

use crate::{Command, CONTRACT_VERSION};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionQuery {
    pub contract_version: String,
    pub session_id: String,
    pub source_generation: u64,
}

impl SessionQuery {
    pub fn of(session_id: &str, source_generation: u64) -> SessionQuery {
        SessionQuery { contract_version: CONTRACT_VERSION.into(), session_id: session_id.into(), source_generation }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandEnvelope {
    pub contract_version: String,
    pub session_id: String,
    pub request_id: String,
    pub source_generation: u64,
    pub sequence: u64,
    pub command: Command,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventEnvelope {
    pub contract_version: String,
    pub session_id: String,
    pub source_generation: u64,
    pub sequence: u64,
    pub events: Vec<serde_json::Value>,
}

impl EventEnvelope {
    /// The reply belongs to this session and comes after the last one.
    pub fn check(&self, query: &SessionQuery, last_sequence: u64) -> Result<(), String> {
        if self.contract_version != CONTRACT_VERSION {
            return Err("unsupported player contract version".into());
        }
        if query.session_id.trim().is_empty() || self.session_id != query.session_id {
            return Err("player session mismatch".into());
        }
        if self.source_generation != query.source_generation {
            return Err("stale source generation".into());
        }
        if self.sequence <= last_sequence {
            return Err("duplicate or out-of-order player message".into());
        }
        Ok(())
    }

    pub fn events(&self) -> Vec<crate::Event> {
        self.events.iter().cloned().map(crate::Event::of).collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum SessionRequest {
    State(SessionQuery),
    Command(CommandEnvelope),
}

pub type SessionReply = Result<EventEnvelope, String>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reply_of_another_session_generation_version_or_an_old_sequence_is_refused() {
        let query = SessionQuery::of("one", 1);
        let mut reply = EventEnvelope { contract_version: CONTRACT_VERSION.into(), session_id: "one".into(), source_generation: 1, sequence: 5, events: vec![] };
        assert!(reply.check(&query, 4).is_ok());
        assert!(reply.check(&query, 5).is_err());
        reply.session_id = "two".into();
        assert!(reply.check(&query, 4).is_err());
        reply.session_id = "one".into();
        reply.source_generation = 2;
        assert!(reply.check(&query, 4).is_err());
        let request = SessionRequest::State(query);
        assert_eq!(serde_json::to_string(&request).unwrap(), r#"{"State":{"contract_version":"0.3.0","session_id":"one","source_generation":1}}"#);
        let said: SessionReply = serde_json::from_str(r#"{"Err":"no"}"#).unwrap();
        assert_eq!(said, Err("no".into()));
    }
}
