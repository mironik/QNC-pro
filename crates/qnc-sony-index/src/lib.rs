//! The Sony MediaProfile index of a card (`MEDIAPRO.XML`, XDROOT / M4ROOT / XDCAM): every
//! `Material` with its original, its proxies and the files related to it (the M01 sidecar, the
//! thumbnail), as relative references inside the recording root. The caller gives the text
//! and the URIs; this block reads nothing. A reference that leaves the root, a duplicate
//! identity or an entry it does not understand is an error, never silently dropped.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

pub const NAMESPACE: &str = "http://xmlns.sony.net/pro/metadata/mediaprofile";
/// The names of the index file on a card, by layout.
pub const INDEX_FILE: &str = "MEDIAPRO.XML";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexedMedia {
    pub relative_path: String,
    pub attributes: BTreeMap<String, String>,
    pub locator: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelatedFile {
    pub relative_path: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Material {
    pub original: IndexedMedia,
    pub proxies: Vec<IndexedMedia>,
    pub related: Vec<RelatedFile>,
}

impl Material {
    /// The recording identity (UMID of the original), upper case.
    pub fn umid(&self) -> Result<String, String> {
        self.original.attributes.get("umid").map(|u| u.to_ascii_uppercase()).ok_or_else(|| "missing recording identity".into())
    }

    /// The one sidecar XML related to the material; two is an error.
    pub fn sidecar(&self) -> Result<Option<&RelatedFile>, String> {
        let mut xml = self.related.iter().filter(|r| r.kind == "XML");
        let first = xml.next();
        if xml.next().is_some() {
            return Err("ambiguous camera sidecar".into());
        }
        Ok(first)
    }

    /// The one thumbnail related to the material; two is none.
    pub fn thumbnail(&self) -> Option<&RelatedFile> {
        let mut jpg = self.related.iter().filter(|r| r.kind == "JPG");
        let first = jpg.next()?;
        jpg.next().is_none().then_some(first)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CameraIndex {
    pub document_uri: String,
    pub properties: BTreeMap<String, String>,
    pub materials: Vec<Material>,
}

/// A medium of a material bound to its URI on the card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoundMedia {
    pub relative_path: String,
    pub media_uri: String,
}

/// A clip as the card knows it: its id, its original and proxy bound to URIs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClipBinding {
    pub clip_id: String,
    pub original: BoundMedia,
    pub proxy: Option<BoundMedia>,
}

/// The index read from its text; `document_uri` is the index file's own URI.
pub fn read_index(text: &str, document_uri: &str) -> Result<CameraIndex, String> {
    qnc_uri::validate_resource_uri(document_uri)?;
    let root = qnc_xml::parse(text, "MediaProfile", NAMESPACE)?;
    let contents = root.one("Contents")?.ok_or("missing Contents")?;
    if contents.children.iter().any(|n| n.namespace != NAMESPACE || n.name != "Material") {
        return Err("unsupported Contents entry; entries cannot be silently discarded".into());
    }
    let mut materials = Vec::new();
    let mut media_paths = BTreeSet::new();
    let mut identities = BTreeSet::new();
    for node in contents.children_named("Material") {
        if node.children.iter().any(|n| n.namespace != NAMESPACE || !matches!(n.name.as_str(), "Proxy" | "RelevantInfo")) {
            return Err("unsupported Material relationship".into());
        }
        let original = indexed_media(node)?;
        if !identities.insert(node.required("umid")?.to_ascii_uppercase()) {
            return Err("duplicate Material UMID: unresolved/spanned identity".into());
        }
        let proxies = node.children_named("Proxy").map(indexed_media).collect::<Result<Vec<_>, _>>()?;
        for media in std::iter::once(&original).chain(&proxies) {
            if !media_paths.insert(media.relative_path.clone()) {
                return Err("duplicate or shared original/proxy reference".into());
            }
        }
        let related = node
            .children_named("RelevantInfo")
            .map(|n| Ok(RelatedFile { relative_path: relative_reference(n.required("uri")?)?, kind: n.required("type")?.into() }))
            .collect::<Result<Vec<_>, String>>()?;
        materials.push(Material { original, proxies, related });
    }
    let mut properties = BTreeMap::new();
    if let Some(node) = root.one("Properties")? {
        node.all_attributes(&mut properties);
    }
    Ok(CameraIndex { document_uri: document_uri.into(), properties, materials })
}

fn indexed_media(node: &qnc_xml::Node) -> Result<IndexedMedia, String> {
    Ok(IndexedMedia { relative_path: relative_reference(node.required("uri")?)?, attributes: node.attributes.clone(), locator: node.locator.clone() })
}

/// A camera-local reference (`./Clip/A.MXF`, percent-encoded) as a relative path inside the
/// recording root; anything that could leave the root is refused.
pub fn relative_reference(value: &str) -> Result<String, String> {
    let value = value.strip_prefix("./").unwrap_or(value);
    let mut segments = Vec::new();
    for encoded in value.split('/') {
        let segment = percent_decode(encoded)?;
        if matches!(segment.as_str(), "" | "." | "..") || segment.contains(['/', '\\', ':', '?', '#', '%']) || segment.chars().any(char::is_control) || segment.trim() != segment {
            return Err("camera reference must remain inside its recording root".into());
        }
        segments.push(segment);
    }
    Ok(segments.join("/"))
}

fn percent_decode(encoded: &str) -> Result<String, String> {
    let bytes = encoded.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = bytes.get(i + 1..i + 3).and_then(|h| std::str::from_utf8(h).ok()).and_then(|h| u8::from_str_radix(h, 16).ok());
            let Some(byte) = hex else { return Err("invalid URI escape".into()) };
            out.push(byte);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| "invalid UTF-8 URI".into())
}

/// The binding of a material to the card: file URIs under `root_source_uri` (the card as a
/// source, e.g. `qnc://local/source/volume-67870d0c`), with the recording root `root_path`
/// inside it (`PRIVATE/XDROOT` or empty).
pub fn bind(material: &Material, clip_id: &str, root_source_uri: &str, root_path: &str) -> Result<ClipBinding, String> {
    qnc_uri::valid_id(clip_id)?;
    if material.proxies.len() > 1 {
        return Err("multiple proxy representations are not supported".into());
    }
    let uri = |relative: &str| -> Result<BoundMedia, String> {
        let path = if root_path.is_empty() { relative.to_string() } else { format!("{}/{relative}", root_path.trim_matches('/')) };
        Ok(BoundMedia { relative_path: relative.into(), media_uri: qnc_uri::file_uri(root_source_uri, &path)? })
    };
    Ok(ClipBinding { clip_id: clip_id.into(), original: uri(&material.original.relative_path)?, proxy: material.proxies.first().map(|p| uri(&p.relative_path)).transpose()? })
}

#[cfg(test)]
mod tests {
    use super::*;

    const INDEX: &str = include_str!("../tests/MEDIAPRO.XML");

    #[test]
    fn the_index_of_a_real_fx6_card_reads_whole_and_a_material_binds_to_file_uris() {
        let index = read_index(INDEX, "qnc://local/source/volume-67870d0c/file/PRIVATE/XDROOT/MEDIAPRO%2EXML").unwrap();
        assert_eq!(index.materials.len(), 118);
        assert_eq!(index.properties.get("/MediaProfile[1]/Properties[1]/System[1]/@systemKind").map(String::as_str), Some("ILME-FX6V ver.6.000"));
        let material = index.materials.iter().find(|m| m.original.relative_path == "Clip/Mironik 1483.MXF").unwrap();
        assert_eq!(material.original.attributes.get("dur").map(String::as_str), Some("362"));
        assert_eq!(material.proxies[0].relative_path, "Sub/Mironik 1483S03.MP4");
        assert_eq!(material.sidecar().unwrap().unwrap().relative_path, "Clip/Mironik 1483M01.XML");
        assert_eq!(material.thumbnail().unwrap().relative_path, "Thmbnl/Mironik 1483T01.JPG");
        let binding = bind(material, "clip-1", "qnc://local/source/volume-67870d0c", "PRIVATE/XDROOT").unwrap();
        assert_eq!(binding.original.media_uri, "qnc://local/source/volume-67870d0c/file/PRIVATE/XDROOT/Clip/Mironik%201483%2EMXF");
        assert_eq!(binding.proxy.unwrap().media_uri, "qnc://local/source/volume-67870d0c/file/PRIVATE/XDROOT/Sub/Mironik%201483S03%2EMP4");
        assert!(relative_reference("./Clip/../x").is_err());
        assert_eq!(relative_reference("./Clip/A%20B.MXF").unwrap(), "Clip/A B.MXF");
        let dup = INDEX.replacen("Mironik 1484.MXF", "Mironik 1483.MXF", 1);
        assert!(read_index(&dup, "qnc://local/source/v/file/MEDIAPRO%2EXML").is_err());
    }
}
