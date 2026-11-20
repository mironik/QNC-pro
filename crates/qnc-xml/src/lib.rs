//! A camera's XML document as a tree of elements. Every element carries a locator
//! (`/Contents[1]/Material[3]`) so a fact can say where it was read; the text is bounded, must
//! be UTF-8, and a DTD or entity declaration is refused before parsing. The rules of the old
//! QNC are kept to the letter so the locators in saved records read here.

use std::collections::BTreeMap;

use xml::reader::{ParserConfig, XmlEvent};

pub const MAX_XML_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug)]
pub struct Node {
    pub name: String,
    pub namespace: String,
    pub attributes: BTreeMap<String, String>,
    pub children: Vec<Node>,
    pub locator: String,
    child_counts: BTreeMap<(String, String), usize>,
}

impl Node {
    /// The children of this name in this element's namespace.
    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Node> {
        self.children.iter().filter(move |n| n.namespace == self.namespace && n.name == name)
    }

    /// The one child of this name; two of them is an error.
    pub fn one(&self, name: &str) -> Result<Option<&Node>, String> {
        let mut children = self.children.iter().filter(|n| n.namespace == self.namespace && n.name == name);
        let first = children.next();
        if children.next().is_some() {
            return Err(format!("duplicate {name} in {}", self.locator));
        }
        Ok(first)
    }

    /// The element down a path of single children.
    pub fn at(&self, path: &[&str]) -> Result<Option<&Node>, String> {
        let mut node = self;
        for name in path {
            let Some(child) = node.one(name)? else { return Ok(None) };
            node = child;
        }
        Ok(Some(node))
    }

    /// An attribute that must be there and not blank.
    pub fn required(&self, key: &str) -> Result<&str, String> {
        self.attributes.get(key).map(String::as_str).filter(|s| !s.trim().is_empty()).ok_or_else(|| format!("missing {key} in {}", self.locator))
    }

    /// Every attribute of this element and its descendants, keyed `<locator>/@<name>`.
    pub fn all_attributes(&self, output: &mut BTreeMap<String, String>) {
        for (key, value) in &self.attributes {
            output.insert(format!("{}/@{key}", self.locator), value.clone());
        }
        for child in &self.children {
            child.all_attributes(output);
        }
    }
}

/// The document as a tree; its root must be `root_name` in `namespace`.
pub fn parse(text: &str, root_name: &str, namespace: &str) -> Result<Node, String> {
    if text.len() > MAX_XML_BYTES || text.contains('\0') {
        return Err("XML exceeds limit or is not UTF-8 text".into());
    }
    if text.contains("<!DOCTYPE") || text.contains("<!ENTITY") {
        return Err("DTD and entity declarations are forbidden".into());
    }
    let reader = ParserConfig::new()
        .trim_whitespace(true)
        .allow_multiple_root_elements(false)
        .max_attributes(64)
        .max_attribute_length(8192)
        .max_name_length(512)
        .max_data_length(MAX_XML_BYTES)
        .max_entity_expansion_depth(0)
        .create_reader(text.as_bytes());
    let mut stack: Vec<Node> = Vec::new();
    let mut root = None;
    let mut count = 0;
    for event in reader {
        match event.map_err(|e| format!("invalid XML: {e}"))? {
            XmlEvent::StartDocument { encoding, .. } if !encoding.eq_ignore_ascii_case("UTF-8") => {
                return Err("only UTF-8 XML is supported".into());
            }
            XmlEvent::StartElement { name, attributes, .. } => {
                count += 1;
                if count > 100_000 || stack.len() >= 64 {
                    return Err("XML node/depth limit exceeded".into());
                }
                let ns = name.namespace.unwrap_or_default();
                let ordinal = stack.last_mut().map_or(1, |parent| {
                    let count = parent.child_counts.entry((ns.clone(), name.local_name.clone())).or_default();
                    *count += 1;
                    *count
                });
                let prefix = stack.last().map_or("", |parent| parent.locator.as_str());
                let local_name = if ns == namespace { name.local_name.clone() } else { format!("{{{ns}}}{}", name.local_name) };
                let locator = format!("{prefix}/{local_name}[{ordinal}]");
                let attributes = attributes
                    .into_iter()
                    .map(|a| {
                        let key = match a.name.namespace {
                            Some(ns) if !ns.is_empty() => format!("{{{ns}}}{}", a.name.local_name),
                            _ => a.name.local_name,
                        };
                        (key, a.value)
                    })
                    .collect();
                stack.push(Node { name: name.local_name, namespace: ns, attributes, children: Vec::new(), locator, child_counts: BTreeMap::new() });
            }
            XmlEvent::EndElement { .. } => {
                let node = stack.pop().ok_or("unbalanced XML")?;
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(node);
                } else if root.replace(node).is_some() {
                    return Err("multiple XML roots".into());
                }
            }
            _ => {}
        }
    }
    let root = root.ok_or("missing XML root")?;
    if !stack.is_empty() || root.name != root_name || root.namespace != namespace {
        return Err("unsupported XML root or namespace".into());
    }
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_document_reads_into_locators_and_a_dtd_or_another_root_is_refused() {
        let text = r#"<?xml version="1.0" encoding="UTF-8"?><Root xmlns="urn:x"><A k="1"/><A k="2"><B/></A></Root>"#;
        let root = parse(text, "Root", "urn:x").unwrap();
        assert_eq!(root.children_named("A").count(), 2);
        assert_eq!(root.children[1].locator, "/A[2]");
        assert_eq!(root.at(&["A"]).unwrap_err(), "duplicate A in /Root[1]");
        assert_eq!(root.children[0].required("k").unwrap(), "1");
        assert!(root.children[0].required("z").is_err());
        let mut all = BTreeMap::new();
        root.all_attributes(&mut all);
        assert_eq!(all.get("/A[2]/@k").map(String::as_str), Some("2"));
        assert!(parse(text, "Other", "urn:x").is_err());
        assert!(parse("<!DOCTYPE x><Root xmlns=\"urn:x\"/>", "Root", "urn:x").is_err());
    }
}
