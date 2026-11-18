//! Container layer of the Solid Edge reader.
//!
//! A Solid Edge part (`.par`), sheet metal part (`.psm`) or assembly (`.asm`) is a compound
//! file: a small file system of named streams in storages ([`cfb`]). Some streams are packed
//! ([`pack`]), and some hold a compound file of their own. [`Document`] opens a file, tells
//! whether it is a Solid Edge document, and hands out its streams.
//!
//! Everything works on bytes in memory: no file system, no `unsafe`.

#![forbid(unsafe_code)]

pub mod cfb;
pub mod pack;
pub mod testing;

use std::fmt;

/// Why a file could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The bytes are not a compound file: no Solid Edge document. Other programs use the
    /// extensions `.par` and `.asm` too; their files end here.
    NotACompoundFile,
    /// A compound file, but without the streams of a Solid Edge document.
    NotSolidEdge,
    /// The compound file is damaged or cut short.
    Damaged(&'static str),
    /// A packed stream does not inflate.
    Compression(&'static str),
    /// The file has no stream of this name.
    MissingStream(String),
    /// The file holds nothing that can be drawn.
    NoGeometry(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotACompoundFile => write!(f, "not a Solid Edge file (no compound file; other programs use this extension too)"),
            Error::NotSolidEdge => write!(f, "not a Solid Edge file (a compound file of another program)"),
            Error::Damaged(what) => write!(f, "damaged compound file: {what}"),
            Error::Compression(what) => write!(f, "packed stream does not inflate: {what}"),
            Error::MissingStream(name) => write!(f, "no stream {name}"),
            Error::NoGeometry(what) => write!(f, "nothing to draw: {what}"),
        }
    }
}

impl std::error::Error for Error {}

/// The stream every Solid Edge document has: the roots of its object store.
pub const ROOTS: &str = "PSMroots";

/// Whether the bytes start as a compound file does.
pub fn is_compound_file(data: &[u8]) -> bool {
    data.len() >= 8 && data[..8] == cfb::MAGIC
}

/// An opened Solid Edge file: its streams by path (`Display/Styles/Library0`).
pub struct Document {
    file: cfb::CompoundFile,
}

impl Document {
    /// Opens the bytes of a `.par`, `.psm` or `.asm` file. A compound file of another program
    /// is refused.
    pub fn open(data: &[u8]) -> Result<Document, Error> {
        let file = cfb::CompoundFile::open(data)?;
        // the object store of the document, and one of the streams only Solid Edge writes next
        // to it: the lightweight data of a model, its display data, its bodies, its parts
        let own = |e: &cfb::Entry| ["PartsLiteData", "Attachments"].contains(&e.path.as_str()) || e.path.starts_with("Display/") || e.path.starts_with("PARASOLID/");
        if !file.has(ROOTS) || !file.entries().iter().any(own) {
            return Err(Error::NotSolidEdge);
        }
        Ok(Document { file })
    }

    /// The streams of the file: path (storages separated by `/`) and size in bytes as stored.
    pub fn streams(&self) -> impl Iterator<Item = (&str, u64)> {
        self.file.entries().iter().map(|e| (e.path.as_str(), e.size))
    }

    /// Whether the file has a stream of this path.
    pub fn has(&self, path: &str) -> bool {
        self.file.has(path)
    }

    /// The bytes of a stream as stored.
    pub fn raw(&self, path: &str) -> Result<Vec<u8>, Error> {
        self.file.stream(path)
    }

    /// The content of a packed stream.
    pub fn packed(&self, path: &str) -> Result<Vec<u8>, Error> {
        pack::inflate(&self.file.stream(path)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_document_gives_plain_and_packed_streams() {
        let text = b"tessellation ".repeat(700);
        let packed = testing::pack(&text, 4096);
        let file = testing::compound_file(&[(ROOTS, b"root"), ("Display/Styles/Library0", &packed), ("PartsLiteData", &[7u8; 5000])]);
        let document = Document::open(&file).unwrap();
        let mut names: Vec<_> = document.streams().map(|(name, size)| (name.to_string(), size)).collect();
        names.sort();
        assert_eq!(names, [("Display/Styles/Library0".to_string(), packed.len() as u64), ("PSMroots".to_string(), 4), ("PartsLiteData".to_string(), 5000)]);
        assert_eq!(document.packed("Display/Styles/Library0").unwrap(), text);
        assert_eq!(document.raw("PSMroots").unwrap(), b"root");
        assert!(document.has("PartsLiteData") && !document.has("Display"));
        assert_eq!(document.raw("Display/Nothing").err(), Some(Error::MissingStream("Display/Nothing".into())));
    }

    #[test]
    fn other_files_are_refused() {
        assert!(matches!(Document::open(b"ISO-10303-21; not this"), Err(Error::NotACompoundFile)));
        assert!(matches!(Document::open(&[]), Err(Error::NotACompoundFile)));
        // a compound file of another program
        let other = testing::compound_file(&[("Contents/DisplayLists", b"x"), ("Header2", b"y")]);
        assert!(matches!(Document::open(&other), Err(Error::NotSolidEdge)));
        let half = testing::compound_file(&[(ROOTS, b"root"), ("WordDocument", b"y")]);
        assert!(matches!(Document::open(&half), Err(Error::NotSolidEdge)));
        assert!(is_compound_file(&other) && !is_compound_file(b"PK\x03\x04"));
    }
}
