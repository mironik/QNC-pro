//! The wire of the Broadcast Player program (`qnc-broadcast-player` of QNC-shared, contract
//! 0.3.0). The program is taken over as it is; this block speaks to it as its own client did:
//! a control request is one TCP connection with a 16-byte header (`QNCPB01\0`, version, kind,
//! token length, body length), the token, a JSON body, and a reply with the same header and a
//! status. Every message carries the session id, the source generation and a sequence; the
//! player refuses what does not match. Commands and events are the names the program knows.

mod envelope;
mod frame;

pub use envelope::{CommandEnvelope, EventEnvelope, SessionQuery, SessionReply, SessionRequest};
pub use frame::{post, MAX_CONTROL_BYTES};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The contract version of the player program this client speaks.
pub const CONTRACT_VERSION: &str = "0.3.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timebase {
    pub fps_num: i64,
    pub fps_den: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameRange {
    pub start_frame: u64,
    pub end_frame: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransportStatus {
    Empty,
    Preparing,
    Ready,
    Playing,
    Paused,
    Stopped,
}

/// The commands the client sends; the names and fields are the program's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    CueFrame { frame: u64, present_frame: bool },
    SetOutputLevels { levels_percent: Vec<u16> },
    SetOutputRoutes { channels: Vec<u16> },
    StartSyncBed { source_frame: u64, bed_frame: u64 },
    StopSyncBed,
    SetPartLevel { output: u16, from_frame: u64, to_frame: u64, gain_db: i16, off: bool },
    SetLaneCurve { output: u16, points: Vec<(u64, i16)> },
    Play,
    Pause,
    Stop,
    Shutdown,
}

/// The events the client reads; the program sends more, those pass as `Other`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    CommandAccepted { command_id: String, command_name: String },
    CommandRejected { command_id: String, command_name: String, reason: String },
    PlaybackReadinessChanged { source_id: Option<String>, frame: u64, ready: bool },
    CarrierPositionChanged { source_id: Option<String>, frame: u64, range: Option<FrameRange>, timebase: Option<Timebase>, status: TransportStatus },
    TransportStatusChanged { status: TransportStatus },
    PlaybackBoundaryReached { frame: u64 },
    SourceFailed { source_id: String, reason: String },
    PlaybackError { reason: String },
    AudioLevelChanged { track_id: String, peak_dbfs_x100: i32 },
    Other { name: String, body: Value },
}

impl Event {
    /// One event as the program wrote it: `{"Name": {fields}}` or `"Name"`.
    pub fn of(value: Value) -> Event {
        let (name, body) = match value {
            Value::Object(map) if map.len() == 1 => map.into_iter().next().unwrap_or_default(),
            Value::String(name) => (name, Value::Null),
            other => return Event::Other { name: String::new(), body: other },
        };
        let text = |key: &str| body.get(key).and_then(Value::as_str).map(str::to_string);
        let number = |key: &str| body.get(key).and_then(Value::as_u64);
        let known = match name.as_str() {
            "CommandAccepted" => text("command_id").zip(text("command_name")).map(|(command_id, command_name)| Event::CommandAccepted { command_id, command_name }),
            "CommandRejected" => text("command_id").zip(text("command_name")).zip(text("reason")).map(|((command_id, command_name), reason)| Event::CommandRejected { command_id, command_name, reason }),
            "PlaybackReadinessChanged" => number("frame").zip(body.get("ready").and_then(Value::as_bool)).map(|(frame, ready)| Event::PlaybackReadinessChanged { source_id: text("source_id"), frame, ready }),
            "CarrierPositionChanged" => number("frame").zip(body.get("status").and_then(|s| serde_json::from_value(s.clone()).ok())).map(|(frame, status)| Event::CarrierPositionChanged {
                source_id: text("source_id"),
                frame,
                range: body.get("range").and_then(|r| serde_json::from_value(r.clone()).ok()),
                timebase: body.get("timebase").and_then(|t| serde_json::from_value(t.clone()).ok()),
                status,
            }),
            "TransportStatusChanged" => body.get("status").and_then(|s| serde_json::from_value(s.clone()).ok()).map(|status| Event::TransportStatusChanged { status }),
            "PlaybackBoundaryReached" => number("frame").map(|frame| Event::PlaybackBoundaryReached { frame }),
            "SourceFailed" => text("source_id").zip(text("reason")).map(|(source_id, reason)| Event::SourceFailed { source_id, reason }),
            "PlaybackError" => text("reason").map(|reason| Event::PlaybackError { reason }),
            "AudioLevelChanged" => text("track_id").zip(body.get("peak_dbfs_x100").and_then(Value::as_i64)).map(|(track_id, peak)| Event::AudioLevelChanged { track_id, peak_dbfs_x100: peak as i32 }),
            _ => None,
        };
        known.unwrap_or(Event::Other { name, body })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_is_written_as_the_program_reads_it_and_an_event_read_as_it_writes_it() {
        assert_eq!(serde_json::to_string(&Command::Play).unwrap(), r#""Play""#);
        assert_eq!(serde_json::to_string(&Command::CueFrame { frame: 7, present_frame: true }).unwrap(), r#"{"CueFrame":{"frame":7,"present_frame":true}}"#);
        let carrier = serde_json::json!({"CarrierPositionChanged": {"source_id": "c", "frame": 12, "range": {"start_frame": 0, "end_frame": 100}, "timebase": {"fps_num": 50, "fps_den": 1}, "status": "Paused"}});
        assert_eq!(
            Event::of(carrier),
            Event::CarrierPositionChanged { source_id: Some("c".into()), frame: 12, range: Some(FrameRange { start_frame: 0, end_frame: 100 }), timebase: Some(Timebase { fps_num: 50, fps_den: 1 }), status: TransportStatus::Paused }
        );
        assert!(matches!(Event::of(serde_json::json!({"DroppedFrame": {"expected_frame": 3}})), Event::Other { name, .. } if name == "DroppedFrame"));
    }
}
