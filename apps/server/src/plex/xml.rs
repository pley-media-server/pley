//! Response tree of the Plex API, written as XML (the default) or as JSON
//! (`Accept: application/json`) in the shapes Plex Media Server uses.

use std::fmt::Write;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Str(String),
    Int(i64),
    Float(f64),
    Bool(bool),
}

impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::Str(v.to_owned())
    }
}

impl From<String> for Value {
    fn from(v: String) -> Self {
        Value::Str(v)
    }
}

impl From<&String> for Value {
    fn from(v: &String) -> Self {
        Value::Str(v.clone())
    }
}

impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Value::Int(v)
    }
}

impl From<i32> for Value {
    fn from(v: i32) -> Self {
        Value::Int(v.into())
    }
}

impl From<u32> for Value {
    fn from(v: u32) -> Self {
        Value::Int(v.into())
    }
}

impl From<u64> for Value {
    fn from(v: u64) -> Self {
        Value::Int(v.min(i64::MAX as u64) as i64)
    }
}

impl From<usize> for Value {
    fn from(v: usize) -> Self {
        Value::Int(v.min(i64::MAX as usize) as i64)
    }
}

impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Value::Float(v)
    }
}

impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Value::Bool(v)
    }
}

/// One element: a tag, attributes in insertion order and child elements.
#[derive(Debug, Clone)]
pub struct Node {
    pub tag: &'static str,
    attrs: Vec<(&'static str, Value)>,
    children: Vec<Node>,
    /// JSON key of the array this element is listed in. Library items are
    /// `Video`/`Directory`/`Track` in XML but all go into `Metadata` in JSON.
    group: &'static str,
    /// JSON: written as a lone object instead of an array (`User`, `Player`).
    single: bool,
}

impl Node {
    pub fn new(tag: &'static str) -> Self {
        Self { tag, attrs: Vec::new(), children: Vec::new(), group: tag, single: false }
    }

    /// A library item: listed under `Metadata` in JSON.
    pub fn item(tag: &'static str) -> Self {
        Self { group: "Metadata", ..Self::new(tag) }
    }

    /// The `MediaContainer` root.
    pub fn container() -> Self {
        Self::new("MediaContainer")
    }

    pub fn single(mut self) -> Self {
        self.single = true;
        self
    }

    pub fn attr(mut self, key: &'static str, value: impl Into<Value>) -> Self {
        self.attrs.push((key, value.into()));
        self
    }

    /// Adds the attribute only when `value` is present.
    pub fn opt<V: Into<Value>>(mut self, key: &'static str, value: Option<V>) -> Self {
        if let Some(value) = value {
            self.attrs.push((key, value.into()));
        }
        self
    }

    /// Sets an attribute, replacing an existing one with the same name.
    pub fn set(&mut self, key: &'static str, value: impl Into<Value>) {
        let value = value.into();
        match self.attrs.iter_mut().find(|(k, _)| *k == key) {
            Some(slot) => slot.1 = value,
            None => self.attrs.push((key, value)),
        }
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.attrs.iter().find(|(k, _)| *k == key).map(|(_, v)| v)
    }

    pub fn child(mut self, child: Node) -> Self {
        self.children.push(child);
        self
    }

    pub fn push(&mut self, child: Node) {
        self.children.push(child);
    }

    pub fn extend(&mut self, children: impl IntoIterator<Item = Node>) {
        self.children.extend(children);
    }

    /// Leaves files' and folders' names (`Part`'s `file`, `Location`'s
    /// `path`), not where the server keeps them.
    pub fn names_only(&mut self) {
        let key = match self.tag {
            "Part" => Some("file"),
            "Location" => Some("path"),
            _ => None,
        };
        if let Some(key) = key
            && let Some(Value::Str(path)) = self.get(key)
        {
            let name = std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned());
            self.set(key, name.unwrap_or_default());
        }
        for child in &mut self.children {
            child.names_only();
        }
    }

    pub fn children(&self) -> &[Node] {
        &self.children
    }

    pub fn children_mut(&mut self) -> &mut Vec<Node> {
        &mut self.children
    }

    pub fn to_xml(&self) -> String {
        let mut out = String::with_capacity(512 + self.children.len() * 512);
        out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        self.write_xml(&mut out);
        out.push('\n');
        out
    }

    fn write_xml(&self, out: &mut String) {
        out.push('<');
        out.push_str(self.tag);
        for (key, value) in &self.attrs {
            out.push(' ');
            out.push_str(key);
            out.push_str("=\"");
            match value {
                Value::Str(s) => escape_xml(out, s),
                Value::Int(i) => {
                    let _ = write!(out, "{i}");
                }
                Value::Float(f) => write_float(out, *f),
                Value::Bool(b) => out.push(if *b { '1' } else { '0' }),
            }
            out.push('"');
        }
        if self.children.is_empty() {
            out.push_str(" />");
            return;
        }
        out.push('>');
        for child in &self.children {
            child.write_xml(out);
        }
        out.push_str("</");
        out.push_str(self.tag);
        out.push('>');
    }

    /// `{"<tag>": {...}}`, e.g. `{"MediaContainer": {"size": 1, "Metadata": [...]}}`.
    pub fn to_json(&self) -> String {
        let mut out = String::with_capacity(512 + self.children.len() * 512);
        out.push('{');
        escape_json(&mut out, self.tag);
        out.push(':');
        self.write_json(&mut out);
        out.push('}');
        out
    }

    fn write_json(&self, out: &mut String) {
        out.push('{');
        let mut first = true;
        let mut comma = |out: &mut String| {
            if !first {
                out.push(',');
            }
            first = false;
        };
        for (key, value) in &self.attrs {
            comma(out);
            escape_json(out, key);
            out.push(':');
            match value {
                Value::Str(s) => escape_json(out, s),
                Value::Int(i) => {
                    let _ = write!(out, "{i}");
                }
                Value::Float(f) if f.is_finite() => write_float(out, *f),
                Value::Float(_) => out.push_str("null"),
                Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            }
        }
        // Children grouped by JSON key, in order of first appearance.
        let mut groups: Vec<(&'static str, bool, Vec<&Node>)> = Vec::new();
        for child in &self.children {
            match groups.iter_mut().find(|(name, ..)| *name == child.group) {
                Some(group) => group.2.push(child),
                None => groups.push((child.group, child.single, vec![child])),
            }
        }
        for (name, single, nodes) in groups {
            comma(out);
            escape_json(out, name);
            out.push(':');
            if single && nodes.len() == 1 {
                nodes[0].write_json(out);
            } else {
                out.push('[');
                for (i, node) in nodes.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    node.write_json(out);
                }
                out.push(']');
            }
        }
        out.push('}');
    }
}

fn write_float(out: &mut String, value: f64) {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        let _ = write!(out, "{}", value as i64);
    } else {
        let _ = write!(out, "{value}");
    }
}

/// Attribute value escaping. Characters XML 1.0 cannot represent are dropped.
fn escape_xml(out: &mut String, value: &str) {
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            '\t' => out.push_str("&#9;"),
            c if (c as u32) < 0x20 || c == '\u{FFFE}' || c == '\u{FFFF}' => {}
            c => out.push(c),
        }
    }
}

fn escape_json(out: &mut String, value: &str) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Node {
        Node::container()
            .attr("size", 2)
            .attr("title1", "Movies & <More> \"quoted\"\nline")
            .child(
                Node::item("Video")
                    .attr("ratingKey", 1)
                    .attr("rating", 7.5)
                    .attr("duration", 6.0)
                    .opt("year", None::<i64>)
                    .child(Node::new("Genre").attr("tag", "Drama"))
                    .child(Node::new("Genre").attr("tag", "Crime")),
            )
            .child(Node::item("Directory").attr("ratingKey", 2).attr("allowSync", false))
            .child(Node::new("User").single().attr("id", 1))
    }

    #[test]
    fn writes_escaped_xml() {
        let xml = sample().to_xml();
        assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<MediaContainer size=\"2\""));
        assert!(xml.contains("title1=\"Movies &amp; &lt;More&gt; &quot;quoted&quot;&#10;line\""));
        assert!(xml.contains("<Video ratingKey=\"1\" rating=\"7.5\" duration=\"6\"><Genre tag=\"Drama\" />"));
        assert!(xml.contains("<Directory ratingKey=\"2\" allowSync=\"0\" />"));
        assert!(!xml.contains("year"));
    }

    #[test]
    fn writes_plex_json() {
        let json: serde_json::Value = serde_json::from_str(&sample().to_json()).unwrap();
        let mc = &json["MediaContainer"];
        assert_eq!(mc["size"], 2);
        assert_eq!(mc["title1"], "Movies & <More> \"quoted\"\nline");
        let items = mc["Metadata"].as_array().unwrap();
        assert_eq!(items.len(), 2, "Video and Directory items share the Metadata array");
        assert_eq!(items[0]["Genre"][1]["tag"], "Crime");
        assert_eq!(items[1]["allowSync"], false);
        assert_eq!(mc["User"]["id"], 1, "single elements are objects");
    }

    #[test]
    fn drops_invalid_xml_characters() {
        let xml = Node::new("A").attr("v", "a\u{1}b").to_xml();
        assert!(xml.contains("v=\"ab\""));
    }
}
