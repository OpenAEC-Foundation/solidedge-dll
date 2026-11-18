//! The placed parts of an assembly (`Attachments`): every document the assembly refers to, and
//! each place it is put.
//!
//! ```text
//! 16 bytes   an identifier
//! u32        version (5 and 7 seen)
//! u32, u32   documents and placements of this assembly
//! u32, u32   the same with those of its sub-assemblies counted in
//! 12 bytes   zero
//! per document:
//!   the file as a file moniker: its full path when the assembly was saved
//!   u32 1
//!   the file as a file moniker: its path from the folder of the assembly
//!   u32 placements, u32 8, u32 a number, u32 flags
//!   per placement:
//!     16 f64  the transform, column by column
//!     6 f64   the box of the placed part
//!     u32     size of the rest
//!     u32 bytes of the name, the name (UTF-16, `Bracket.par:2`), attributes
//! ```
//!
//! A file moniker is laid out as Microsoft documents it ([MS-OSHARED], FileMoniker): a path in
//! the code page of the writer, and the same in UTF-16 when the code page cannot say it. What
//! follows the last document is not read.

/// A document an assembly refers to.
#[derive(Debug, Clone, PartialEq)]
pub struct Reference {
    /// Its file name, without folders: `Bracket.par`.
    pub file: String,
    /// Each place it is put.
    pub placements: Vec<Placement>,
}

/// One place of a document in an assembly.
#[derive(Debug, Clone, PartialEq)]
pub struct Placement {
    /// `Bracket.par:2`.
    pub name: String,
    /// A 4 x 4 matrix, row by row, that takes a column vector in the coordinates of the part
    /// to the coordinates of the assembly.
    pub transform: [f64; 16],
}

/// The documents of an assembly, and whether the list was read to its end.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Attachments {
    pub references: Vec<Reference>,
    pub complete: bool,
}

const FILE_MONIKER: [u8; 16] = [0x03, 0x03, 0, 0, 0, 0, 0, 0, 0xC0, 0, 0, 0, 0, 0, 0, 0x46];

struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        let bytes = self.data.get(self.at..self.at.checked_add(count)?)?;
        self.at += count;
        Some(bytes)
    }

    fn u16(&mut self) -> Option<u16> {
        self.take(2).map(|b| u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Option<u32> {
        self.take(4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn f64(&mut self) -> Option<f64> {
        self.take(8).map(|b| f64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]))
    }

    fn utf16(&mut self, bytes: usize) -> Option<String> {
        let units: Vec<u16> = self.take(bytes)?.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        Some(String::from_utf16_lossy(&units).trim_end_matches('\0').to_string())
    }

    /// A file moniker: its path, from the UTF-16 form when it has one. The other form is in a
    /// code page that the file does not name; it is read as Latin-1, which is right for ASCII.
    fn moniker(&mut self) -> Option<String> {
        if self.take(16)? != FILE_MONIKER {
            return None;
        }
        self.u16()?;
        let length = self.u32()? as usize;
        let narrow: String = self.take(length)?.iter().take_while(|&&b| b != 0).map(|&b| b as char).collect();
        // the length of the server part and a version, then twenty bytes kept free
        self.take(24)?;
        if self.u32()? == 0 {
            return Some(narrow);
        }
        let bytes = self.u32()? as usize;
        self.u16()?;
        self.utf16(bytes)
    }
}

/// The last name of a path with `\` or `/` between its folders.
fn file_name(path: &str) -> &str {
    path.rsplit(['\\', '/']).next().unwrap_or(path)
}

fn document(reader: &mut Reader) -> Option<Reference> {
    let full = reader.moniker()?;
    reader.u32()?;
    let relative = reader.moniker()?;
    // the path from the folder of the assembly names the file as it is on disk; the full path
    // may be the short (8.3) form of it
    let file = if file_name(&relative).is_empty() { file_name(&full) } else { file_name(&relative) }.to_string();
    let count = reader.u32()? as usize;
    reader.take(12)?;
    // a placement is at least its transform, box and sizes
    if count > (reader.data.len() - reader.at) / 184 {
        return None;
    }
    let mut placements = Vec::with_capacity(count);
    for _ in 0..count {
        let mut stored = [0.0; 16];
        for value in &mut stored {
            *value = reader.f64()?;
        }
        reader.take(48)?;
        let size = reader.u32()? as usize;
        let mut rest = Reader { data: reader.take(size)?, at: 0 };
        let bytes = rest.u32()? as usize;
        let name = rest.utf16(bytes)?;
        if stored.iter().any(|v| !v.is_finite()) {
            return None;
        }
        // stored column by column
        let mut transform = [0.0; 16];
        for r in 0..4 {
            for c in 0..4 {
                transform[r * 4 + c] = stored[c * 4 + r];
            }
        }
        placements.push(Placement { name, transform });
    }
    Some(Reference { file, placements })
}

/// The documents of an assembly, from the bytes of its `Attachments` stream. A list that
/// cannot be read to its end gives the documents before that point.
pub fn parse(stream: &[u8]) -> Attachments {
    let mut reader = Reader { data: stream, at: 20 };
    let Some(count) = reader.u32() else { return Attachments::default() };
    reader.at = 48;
    let mut out = Attachments { references: Vec::new(), complete: true };
    for _ in 0..count {
        match document(&mut reader) {
            Some(reference) => out.references.push(reference),
            None => {
                out.complete = false;
                break;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::IDENTITY;
    use crate::testing::{self, moved};

    #[test]
    fn documents_and_their_places_are_read() {
        let turned = [0.0, -1.0, 0.0, 0.5, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 0.0, 0.0, 0.0, 1.0];
        for version in [5, 7] {
            let stream = testing::attachments(
                &[
                    ("C:\\Work\\PROJEC~1\\Wheel.par", "Wheel.par", &[("Wheel.par:1", IDENTITY), ("Wheel.par:2", turned)]),
                    ("C:\\Work\\Größe\\Träger 1.psm", "sub\\Träger 1.psm", &[("Träger 1.psm:1", moved(1.0, 2.0, 3.0))]),
                ],
                version,
            );
            let read = parse(&stream);
            assert!(read.complete, "version {version}");
            assert_eq!(read.references.len(), 2);
            assert_eq!(read.references[0].file, "Wheel.par");
            assert_eq!(read.references[0].placements, [Placement { name: "Wheel.par:1".into(), transform: IDENTITY }, Placement { name: "Wheel.par:2".into(), transform: turned }]);
            assert_eq!((read.references[1].file.as_str(), read.references[1].placements[0].transform[3]), ("Träger 1.psm", 1.0));
        }
    }

    #[test]
    fn a_list_cut_short_gives_what_came_before() {
        let stream = testing::attachments(&[("C:\\a\\One.par", "One.par", &[("One.par:1", IDENTITY)]), ("C:\\a\\Two.par", "Two.par", &[("Two.par:1", IDENTITY)])], 7);
        // the last eight bytes belong to what follows the documents
        for cut in 0..stream.len() - 8 {
            let read = parse(&stream[..cut]);
            assert!(!read.complete, "cut at {cut}");
            assert!(read.references.len() < 2);
        }
        assert_eq!(parse(&[]), Attachments::default());
        // a document that says it is placed more often than there are bytes
        let mut lying = stream.clone();
        let count = lying.windows(8).rposition(|w| w == b"One.par\0").unwrap() + 8 + 24 + 4;
        lying[count..count + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(parse(&lying), Attachments { references: Vec::new(), complete: false });
    }

    #[test]
    fn an_assembly_without_parts_is_complete() {
        assert_eq!(parse(&testing::attachments(&[], 7)), Attachments { references: Vec::new(), complete: true });
    }
}
