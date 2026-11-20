//! Saved record -> player input, exactly as the Broadcast Player program reads it (contract
//! 0.3.0). The project says what plays (original, proxy, optimized) and how many outputs;
//! the saved record says what the medium is. Nothing is probed, decoded or guessed: a record
//! that is not final, not complete for the chosen representation, or whose proxy timing does
//! not match the original is refused with the reason.

mod layout;

use qnc_media_model::MediaRepresentation;
use qnc_media_record::{Phase, Snapshot};
use serde::{Deserialize, Serialize};

pub use layout::{AudioChannel, StreamLayout, VideoInput};

pub const CONTRACT_VERSION: &str = "0.3.0";
/// The highest level of a lead channel, percent (four times as recorded).
pub const MAX_LEVEL_PERCENT: u16 = 400;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "code", content = "detail", rename_all = "snake_case", deny_unknown_fields)]
pub enum InputError {
    Settings(String),
    WrongWorkspace,
    InvalidRecord(String),
    NotFinal,
    MissingProxy,
    IncompleteMedia(Vec<String>),
    UnsupportedMedia(String),
    InvalidDescriptor,
}

impl std::fmt::Display for InputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "player input: {self:?}")
    }
}

pub type Result<T> = std::result::Result<T, InputError>;

/// Whether a player error says the record is still being completed in the background.
pub fn is_incomplete_media(message: &str) -> bool {
    message.contains("IncompleteMedia") || message.contains("NotFinal")
}

/// What the project plays (`playback.input` of its settings).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackInput {
    Original,
    Proxy,
    ProxyIfAvailable,
    OptimizedIfAvailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Representation {
    Original,
    Proxy,
    Optimized,
}

/// The project's output audio, from its settings, not from the source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectAudio {
    pub channels: u16,
    pub sample_rate_hz: u32,
}

impl ProjectAudio {
    pub fn validate(&self) -> Result<()> {
        if !(1..=64).contains(&self.channels) || !(8000..=384000).contains(&self.sample_rate_hz) {
            return Err(InputError::Settings("Project audio format is outside supported limits.".into()));
        }
        Ok(())
    }
}

/// What of the project's settings the player input needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaybackSettings {
    pub workspace_db_uri: String,
    pub playback_input: PlaybackInput,
    pub audio: ProjectAudio,
}

/// The saved clip as the catalog gives it to the player.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerClipRecord {
    pub name: String,
    pub snapshot: Snapshot,
    pub imported_media_uri: Option<String>,
    #[serde(default)]
    pub imported_copy_of: Option<Representation>,
    #[serde(default)]
    pub imported_optimized: Option<MediaRepresentation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedInput {
    pub contract_version: String,
    pub workspace_db_uri: String,
    pub playback_input: PlaybackInput,
    pub project_audio: ProjectAudio,
    pub representation: Representation,
    pub snapshot: Snapshot,
    pub layout: StreamLayout,
    #[serde(default)]
    pub lead_audio_channels: Vec<u16>,
    #[serde(default)]
    pub lead_audio_levels: Vec<u16>,
    #[serde(default)]
    pub optimized: Option<MediaRepresentation>,
}

/// The input of a clip for the project's settings.
pub fn prepare(settings: &PlaybackSettings, stored: &PlayerClipRecord) -> Result<PreparedInput> {
    validate_workspace(&settings.workspace_db_uri)?;
    settings.audio.validate()?;
    if stored.name.trim().is_empty() || stored.name.len() > 1024 {
        return Err(InputError::InvalidRecord("Invalid clip name in the player input.".into()));
    }
    validate_snapshot(&stored.snapshot)?;
    let snapshot = imported_snapshot(stored)?;
    let optimized = stored.imported_optimized.clone();
    let representation = choose(settings.playback_input, &snapshot, optimized.is_some())?;
    Ok(PreparedInput {
        contract_version: CONTRACT_VERSION.into(),
        workspace_db_uri: settings.workspace_db_uri.clone(),
        playback_input: settings.playback_input,
        project_audio: settings.audio.clone(),
        representation,
        layout: layout::layout(&snapshot, optimized.as_ref(), representation)?,
        snapshot,
        lead_audio_channels: Vec::new(),
        lead_audio_levels: Vec::new(),
        optimized,
    })
}

impl PreparedInput {
    /// The same input with `lead` channels first; a channel the clip lacks is refused.
    pub fn with_lead_channels(mut self, lead: &[u16]) -> Result<Self> {
        let count = self.layout.audio_channels.len();
        if let Some(missing) = lead.iter().find(|c| usize::from(**c) >= count) {
            return Err(InputError::UnsupportedMedia(format!("The clip has no audio channel {}.", missing + 1)));
        }
        self.lead_audio_channels = lead.to_vec();
        Ok(self)
    }

    /// The same input with the lead channels at these levels (percent, 0..=400).
    pub fn with_lead_levels(mut self, levels: &[u16]) -> Result<Self> {
        if levels.len() != self.lead_audio_channels.len() || levels.iter().any(|l| *l > MAX_LEVEL_PERCENT) {
            return Err(InputError::InvalidDescriptor);
        }
        self.lead_audio_levels = levels.to_vec();
        Ok(self)
    }

    /// The representation the picture plays from.
    pub fn media(&self) -> Result<&MediaRepresentation> {
        match self.representation {
            Representation::Original => Ok(&self.snapshot.metadata.original),
            Representation::Proxy => self.snapshot.metadata.proxy.as_ref().ok_or(InputError::MissingProxy),
            Representation::Optimized => self.optimized.as_ref().ok_or(InputError::InvalidDescriptor),
        }
    }

    /// The sound always comes from the original.
    pub fn audio_media(&self) -> &MediaRepresentation {
        &self.snapshot.metadata.original
    }

    /// The input is for this workspace and clip and agrees with the record and settings in it.
    pub fn validate_for(&self, workspace_uri: &str, clip_id: &str) -> Result<()> {
        if self.contract_version != CONTRACT_VERSION || self.snapshot.metadata.clip_id != clip_id {
            return Err(InputError::InvalidDescriptor);
        }
        validate_workspace(&self.workspace_db_uri)?;
        if self.workspace_db_uri != workspace_uri {
            return Err(InputError::WrongWorkspace);
        }
        validate_snapshot(&self.snapshot)?;
        self.project_audio.validate()?;
        if self.representation != choose(self.playback_input, &self.snapshot, self.optimized.is_some())? {
            return Err(InputError::InvalidDescriptor);
        }
        if self.layout != layout::layout(&self.snapshot, self.optimized.as_ref(), self.representation)? {
            return Err(InputError::InvalidDescriptor);
        }
        if self.lead_audio_channels.iter().any(|c| usize::from(*c) >= self.layout.audio_channels.len()) {
            return Err(InputError::InvalidDescriptor);
        }
        if self.lead_audio_levels.len() > self.lead_audio_channels.len() || self.lead_audio_levels.iter().any(|l| *l > MAX_LEVEL_PERCENT) {
            return Err(InputError::InvalidDescriptor);
        }
        Ok(())
    }
}

fn choose(mode: PlaybackInput, snapshot: &Snapshot, optimized: bool) -> Result<Representation> {
    if mode == PlaybackInput::OptimizedIfAvailable {
        return Ok(if optimized { Representation::Optimized } else { Representation::Original });
    }
    match (mode, snapshot.metadata.proxy.is_some()) {
        (PlaybackInput::Original, _) | (PlaybackInput::ProxyIfAvailable | PlaybackInput::OptimizedIfAvailable, false) => Ok(Representation::Original),
        (PlaybackInput::Proxy, false) => Err(InputError::MissingProxy),
        (_, true) => Ok(Representation::Proxy),
    }
}

/// Media linked where it is keeps the saved record; a byte copy in the project stands for the
/// representation it copies (same streams, timing, channels, nothing probed again); any other
/// imported URI has no saved description and is refused.
fn imported_snapshot(stored: &PlayerClipRecord) -> Result<Snapshot> {
    let saved = &stored.snapshot;
    let Some(imported) = &stored.imported_media_uri else { return Ok(saved.clone()) };
    if imported == &saved.binding.original_uri || Some(imported) == saved.binding.proxy_uri.as_ref() {
        return Ok(saved.clone());
    }
    let copied = match stored.imported_copy_of {
        Some(Representation::Original) => saved.binding.original_uri.clone(),
        Some(Representation::Proxy) => saved.binding.proxy_uri.clone().ok_or(InputError::MissingProxy)?,
        Some(Representation::Optimized) | None => {
            return Err(InputError::UnsupportedMedia("Imported media URI has no saved original/proxy representation binding.".into()))
        }
    };
    let mut snapshot = saved.clone();
    let metadata = &mut snapshot.metadata;
    let uris = [Some(&mut snapshot.binding.original_uri), snapshot.binding.proxy_uri.as_mut(), Some(&mut metadata.original.media_uri), metadata.proxy.as_mut().map(|p| &mut p.media_uri)];
    for uri in uris.into_iter().flatten().chain(metadata.evidence.iter_mut().map(|e| &mut e.media_uri)) {
        if *uri == copied {
            *uri = imported.clone();
        }
    }
    validate_snapshot(&snapshot)?;
    Ok(snapshot)
}

fn validate_workspace(uri: &str) -> Result<()> {
    let parsed = qnc_uri::parse(uri).map_err(|_| InputError::WrongWorkspace)?;
    if parsed.resource_kind != "db" {
        return Err(InputError::WrongWorkspace);
    }
    let id = parsed.resource_id.strip_prefix("project_workspace/").ok_or(InputError::WrongWorkspace)?;
    qnc_uri::valid_id(id).map_err(|_| InputError::WrongWorkspace)
}

fn validate_snapshot(snapshot: &Snapshot) -> Result<()> {
    snapshot.validate().map_err(|e| InputError::InvalidRecord(e.to_string()))?;
    if snapshot.phase != Phase::Final {
        return Err(InputError::NotFinal);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
