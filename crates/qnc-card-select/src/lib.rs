//! Select of a card: what the card says of its clips, before anything is copied or probed.
//! The card is read only (the index, each material's sidecar); the camera is told by the
//! profiles; every clip becomes a camera-phase catalog clip bound to the card's file URIs
//! (`qnc://local/source/volume-<serial>/file/<path>`), with the documents it was read from
//! named by their SHA-256 (`qnc://local/artifact/camera-document/<sha256>/<source>/file/<path>`),
//! exactly as the old QNC named them. A material whose sidecar or facts are not sound is
//! reported and skipped; the others go on.

use std::path::Path;

use qnc_camera_profile::Profile;
use qnc_catalog::CatalogClip;
use qnc_media_record::{inspect_phase, Binding, Completeness, Phase, Snapshot};
use qnc_sony_index::{CameraIndex, Material};

pub const SOURCE_INDEX_URI: &str = "qnc://local/db/source_index";
pub const MEDIA_RECORDS_URI: &str = "qnc://local/db/media_records";

/// The card as a source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    /// The disk, e.g. `E:\`.
    pub disk: std::path::PathBuf,
    /// Eight hex digits of the volume.
    pub serial: String,
    pub volume_name: String,
    /// The recording root inside the disk (`PRIVATE/XDROOT`, `XDROOT`, ...).
    pub root_path: String,
}

impl Card {
    pub fn source_uri(&self) -> String {
        format!("qnc://local/source/volume-{}", self.serial.to_ascii_lowercase())
    }

    fn file(&self, relative: &str) -> std::path::PathBuf {
        let mut path = self.disk.clone();
        for part in self.root_path.split('/').chain(relative.split('/')).filter(|p| !p.is_empty()) {
            path.push(part);
        }
        path
    }

    fn card_path(&self, relative: &str) -> String {
        if self.root_path.is_empty() { relative.to_string() } else { format!("{}/{relative}", self.root_path.trim_matches('/')) }
    }

    /// The URI of a document of the card as the records name it: by its text.
    pub fn document_uri(&self, relative: &str, text: &str) -> Result<String, String> {
        let file = qnc_uri::file_uri(&self.source_uri(), &self.card_path(relative))?;
        let rest = file.strip_prefix("qnc://local/source/").ok_or("not a local source")?;
        Ok(format!("qnc://local/artifact/camera-document/{}/{rest}", qnc_checksum::sha256_hex(text.as_bytes())))
    }
}

/// What a Select found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selected {
    pub clips: Vec<CatalogClip>,
    /// Materials not taken, with why.
    pub skipped: Vec<(String, String)>,
    pub profile: String,
    pub recognised: bool,
}

/// The index of the card read from the disk.
pub fn read_index(card: &Card) -> Result<(CameraIndex, String), String> {
    let path = card.file(qnc_sony_index::INDEX_FILE);
    let text = std::fs::read_to_string(&path).map_err(|_| qnc_text_path("select.index_unreadable", &path))?;
    let uri = card.document_uri(qnc_sony_index::INDEX_FILE, &text)?;
    Ok((qnc_sony_index::read_index(&text, &uri)?, text))
}

fn qnc_text_path(code: &str, path: &Path) -> String {
    format!("{code}: {}", path.display())
}

/// Every clip the card describes, in camera phase, recorded at `recorded_at_unix_ms`.
pub fn select(card: &Card, profiles: &[Profile], recorded_at_unix_ms: u64) -> Result<Selected, String> {
    let (index, index_text) = read_index(card)?;
    let sidecar_texts: Vec<String> = index.materials.iter().filter_map(|m| m.sidecar().ok().flatten()).filter_map(|s| std::fs::read_to_string(card.file(&s.relative_path)).ok()).take(1).collect();
    let mut records: Vec<&str> = vec![index_text.as_str()];
    records.extend(sidecar_texts.iter().map(String::as_str));
    let (profile, recognised) = qnc_camera_profile::recognise(profiles, qnc_camera_profile::SONY_READER, &records).ok_or_else(|| format!("select.no_profile: {}", card.disk.display()))?;
    let mut clips = Vec::new();
    let mut skipped = Vec::new();
    for (i, material) in index.materials.iter().enumerate() {
        match clip_of(card, &index, i, material, profile, recognised, recorded_at_unix_ms) {
            Ok(clip) => clips.push(clip),
            Err(error) => skipped.push((material.original.relative_path.clone(), error)),
        }
    }
    Ok(Selected { clips, skipped, profile: profile.stamp(), recognised })
}

fn clip_of(card: &Card, index: &CameraIndex, i: usize, material: &Material, profile: &Profile, recognised: bool, at: u64) -> Result<CatalogClip, String> {
    let clip_id = format!("clip-{}", qnc_ids::fresh());
    let binding = qnc_sony_index::bind(material, &clip_id, &card.source_uri(), &card.root_path)?;
    let sidecar = match material.sidecar()? {
        Some(related) => {
            let path = card.file(&related.relative_path);
            let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let document_uri = card.document_uri(&related.relative_path, &text)?;
            Some(qnc_sony_clip::Sidecar { relative_path: related.relative_path.clone(), document_uri, text })
        }
        None => None,
    };
    let mut metadata = qnc_sony_clip::read_clip(index, i, &binding, sidecar.as_ref())?.metadata;
    qnc_camera_profile::stamp(&mut metadata, profile, recognised);
    let report = inspect_phase(&metadata, Phase::Camera).map_err(|e| e.to_string())?;
    let completeness = if report.is_complete() { Completeness::Complete } else { Completeness::Partial };
    let snapshot = Snapshot {
        binding: Binding { source_index_uri: SOURCE_INDEX_URI.into(), source_record_id: qnc_ids::fresh(), original_uri: binding.original.media_uri.clone(), proxy_uri: binding.proxy.as_ref().map(|p| p.media_uri.clone()) },
        revision: 1,
        phase: Phase::Camera,
        completeness,
        metadata,
        report,
        recorded_at_unix_ms: at,
    };
    snapshot.validate().map_err(|e| e.to_string())?;
    let name = material.original.relative_path.rsplit('/').next().unwrap_or(&material.original.relative_path).to_string();
    let thumbnail_uri = material.thumbnail().map(|t| qnc_uri::file_uri(&card.source_uri(), &card.card_path(&t.relative_path))).transpose()?;
    Ok(CatalogClip { name, source_uri: card.source_uri(), source_name: format!("{} {}", profile.manufacturer, profile.model).trim().to_string(), serial_number: card.serial.clone(), volume_name: card.volume_name.clone(), thumbnail_uri, media_records_uri: MEDIA_RECORDS_URI.into(), snapshot })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A card built from the fixtures: the index of the real FX6 card and the sidecar of one
    /// clip (the other materials have none and are read from the index alone).
    #[test]
    fn a_card_selects_into_camera_phase_clips_bound_to_its_file_uris() {
        let dir = std::env::temp_dir().join(format!("qnc-card-select-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let root = dir.join("PRIVATE").join("XDROOT");
        std::fs::create_dir_all(root.join("Clip")).unwrap();
        std::fs::write(root.join("MEDIAPRO.XML"), include_str!("../../qnc-sony-index/tests/MEDIAPRO.XML")).unwrap();
        std::fs::write(root.join("Clip").join("Mironik 1483M01.XML"), include_str!("../../qnc-sony-index/tests/Mironik_1483M01.XML")).unwrap();
        let card = Card { disk: dir.clone(), serial: "67870D0C".into(), volume_name: "UNTITLED".into(), root_path: "PRIVATE/XDROOT".into() };
        let (profiles, _) = qnc_camera_profile::read_profiles(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../contracts/cameras/profiles"));
        let selected = select(&card, &profiles, 5).unwrap();
        assert_eq!((selected.profile.as_str(), selected.recognised), ("sony-ilme-fx6v@1", true));
        // Only one material has its sidecar here; a material whose related sidecar is missing
        // is not taken (the card is unsound), the others go on.
        assert_eq!((selected.clips.len(), selected.skipped.len()), (1, 117));
        let clip = &selected.clips[0];
        assert_eq!(clip.name, "Mironik 1483.MXF");
        assert_eq!(clip.snapshot.binding.original_uri, "qnc://local/source/volume-67870d0c/file/PRIVATE/XDROOT/Clip/Mironik%201483%2EMXF");
        assert_eq!(clip.thumbnail_uri.as_deref(), Some("qnc://local/source/volume-67870d0c/file/PRIVATE/XDROOT/Thmbnl/Mironik%201483T01%2EJPG"));
        assert_eq!(clip.snapshot.phase, Phase::Camera);
        assert_eq!(qnc_media_model::single_video(&clip.snapshot.metadata.original).unwrap().exact_frame_count(), Some(362));
        assert!(clip.snapshot.metadata.evidence[2].document_uri.starts_with("qnc://local/artifact/camera-document/") && clip.snapshot.metadata.evidence[2].document_uri.ends_with("/volume-67870d0c/file/PRIVATE/XDROOT/Clip/Mironik%201483M01%2EXML"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod live {
    use super::*;

    /// The real FX6 card on D: when it is in (read only); otherwise the test says so and passes.
    #[test]
    fn the_real_card_selects_every_clip() {
        let disk = Path::new("D:\\");
        if !disk.join("PRIVATE").join("XDROOT").join("MEDIAPRO.XML").is_file() {
            eprintln!("skipped: no card on D:");
            return;
        }
        let card = Card { disk: disk.into(), serial: "67870D0C".into(), volume_name: "UNTITLED".into(), root_path: "PRIVATE/XDROOT".into() };
        let (profiles, _) = qnc_camera_profile::read_profiles(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../contracts/cameras/profiles"));
        let selected = select(&card, &profiles, 5).unwrap();
        assert!(selected.skipped.is_empty(), "{:?}", selected.skipped);
        assert!(selected.clips.len() >= 100 && selected.clips.iter().all(|c| c.snapshot.phase == Phase::Camera));
        // The documents are named by the bytes of the card: the names the old QNC wrote.
        let clip = selected.clips.iter().find(|c| c.name == "Mironik 1483.MXF").unwrap();
        assert_eq!(clip.snapshot.metadata.evidence[0].document_uri, "qnc://local/artifact/camera-document/293e22eee5fede3ab5c18c3d54776320aefcadd89fe0764fd8c2ce129d106120/volume-67870d0c/file/PRIVATE/XDROOT/MEDIAPRO%2EXML");
        assert_eq!(clip.snapshot.metadata.evidence[2].document_uri, "qnc://local/artifact/camera-document/87975909116ecdba51b9a2971999d92bb25abb62fbac67a41aa16c15092880b1/volume-67870d0c/file/PRIVATE/XDROOT/Clip/Mironik%201483M01%2EXML");
    }
}
