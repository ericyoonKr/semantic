use std::io::BufRead;

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use crate::error::ParseError;
use crate::guard::{Deadline, Member};

const CHECK_EVERY: u32 = 1024;

pub(crate) trait Guarded {
    fn violation(&self) -> Option<ParseError>;
}

impl Guarded for &[u8] {
    fn violation(&self) -> Option<ParseError> {
        None
    }
}

impl Guarded for Member<'_> {
    fn violation(&self) -> Option<ParseError> {
        Member::violation(self)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Tag {
    pub(crate) name: String,
    attrs: Vec<(String, String)>,
}

impl Tag {
    pub(crate) fn attr(&self, key: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    pub(crate) fn attr_u32(&self, key: &str) -> Option<u32> {
        self.attr(key).and_then(|v| v.trim().parse().ok())
    }

    pub(crate) fn on(&self, key: &str) -> bool {
        !matches!(self.attr(key), Some("0" | "false" | "off"))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Node {
    Open(Tag),
    Close(String),
    Text(String),
    Eof,
}

pub(crate) struct XmlReader<R> {
    reader: Reader<R>,
    buf: Vec<u8>,
    pending_close: Option<String>,
    deadline: Deadline,
    events: u32,
}

impl<R: BufRead + Guarded> XmlReader<R> {
    pub(crate) fn new(source: R, deadline: Deadline) -> Self {
        Self {
            reader: Reader::from_reader(source),
            buf: Vec::new(),
            pending_close: None,
            deadline,
            events: 0,
        }
    }

    pub(crate) fn next(&mut self) -> Result<Node, ParseError> {
        if let Some(name) = self.pending_close.take() {
            return Ok(Node::Close(name));
        }
        loop {
            self.events = self.events.wrapping_add(1);
            if self.events.is_multiple_of(CHECK_EVERY) {
                self.deadline.check()?;
            }
            self.buf.clear();
            let node = match self.reader.read_event_into(&mut self.buf) {
                Err(_) => None,
                Ok(Event::Start(e)) => Some(tag(&e).map(Node::Open)),
                Ok(Event::Empty(e)) => Some(tag(&e).map(|t| {
                    self.pending_close = Some(t.name.clone());
                    Node::Open(t)
                })),
                Ok(Event::End(e)) => Some(utf8(e.name().as_ref()).map(Node::Close)),
                Ok(Event::Text(t)) => Some(
                    t.unescape()
                        .map(|s| Node::Text(s.into_owned()))
                        .map_err(|_| ParseError::Corrupted),
                ),
                Ok(Event::CData(c)) => Some(utf8(&c.into_inner()).map(Node::Text)),
                Ok(Event::DocType(_)) => Some(Err(ParseError::Corrupted)),
                Ok(Event::Eof) => Some(Ok(Node::Eof)),
                Ok(_) => continue,
            };
            return match node {
                Some(result) => result,
                None => Err(self.reader.get_ref().violation().unwrap_or(ParseError::Corrupted)),
            };
        }
    }

    pub(crate) fn skip(&mut self) -> Result<(), ParseError> {
        let mut depth: usize = 1;
        loop {
            match self.next()? {
                Node::Open(_) => depth += 1,
                Node::Close(_) => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(());
                    }
                }
                Node::Text(_) => {}
                Node::Eof => return Err(ParseError::Corrupted),
            }
        }
    }
}

fn tag(e: &BytesStart<'_>) -> Result<Tag, ParseError> {
    let name = utf8(e.name().as_ref())?;
    let mut attrs = Vec::new();
    for attr in e.attributes() {
        let attr = attr.map_err(|_| ParseError::Corrupted)?;
        let key = utf8(attr.key.as_ref())?;
        let value = attr.unescape_value().map_err(|_| ParseError::Corrupted)?;
        attrs.push((key, value.into_owned()));
    }
    Ok(Tag { name, attrs })
}

fn utf8(bytes: &[u8]) -> Result<String, ParseError> {
    String::from_utf8(bytes.to_vec()).map_err(|_| ParseError::Corrupted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guard::GuardLimits;

    fn drain(xml: &str) -> Result<Vec<Node>, ParseError> {
        let deadline = Deadline::start(&GuardLimits::default());
        let mut reader = XmlReader::new(xml.as_bytes(), deadline);
        let mut out = Vec::new();
        loop {
            match reader.next()? {
                Node::Eof => return Ok(out),
                n => out.push(n),
            }
        }
    }

    #[test]
    fn doctype_with_entity_is_rejected() {
        let xml = "<?xml version=\"1.0\"?><!DOCTYPE r [<!ENTITY x \"aaaa\">]><r>t</r>";
        assert_eq!(drain(xml), Err(ParseError::Corrupted));
    }

    #[test]
    fn plain_xml_passes_and_empty_tags_close() {
        let nodes = drain("<a k=\"v\"><b/>t&amp;u</a>").unwrap_or_default();
        assert_eq!(nodes.len(), 5);
        assert_eq!(nodes[3], Node::Text("t&u".to_string()));
    }
}
