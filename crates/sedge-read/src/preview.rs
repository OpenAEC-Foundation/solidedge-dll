//! The preview of an assembly (the storage `Preview`, in files of newer versions): what Solid
//! Edge saves to show an assembly without opening the files of its parts. It holds every file
//! of the assembly as a numbered "site", for each part a display cache and the styles it uses,
//! and for each assembly the sites it places and where.
//!
//! ```text
//! Preview/Structure/Sites             u32 1, u32, u32 0, u32 count
//!                                     per site: u32 number, u32 0, the path of its file
//! Preview/Structure/IDS               u32 1, 3, 2, 1
//!                                     "Sites", u32 sites, u32 assemblies, u32 parts
//!                                     per site: "Site", u32 number, u32 flags, 6 f64 box, 2 u32
//!                                     "Atts"
//!                                     per assembly: u32 its site, u32 count
//!                                       per placement: "Att", u32 site, u32 number, u32 0,
//!                                                      16 f64 transform, the name
//! Preview/Display/Scene0/Geometries   a compound file: `Cache<site>.1`, a display cache as a
//!                                     storage or as a stream that is a compound file
//! Preview/Display/Scene0/Libraries    a compound file: `Library<site>`, a packed style library
//! ```
//!
//! A string is a u32 count of UTF-16 units and the units. The caches hold a coarser level of
//! detail than the files of the parts do.

use crate::cache::{self, Face};
use crate::styles::{self, Styles};
use sedge_core::{cfb::CompoundFile, pack, Document};
use std::collections::HashMap;

pub const SITES: &str = "Preview/Structure/Sites";
pub const STRUCTURE: &str = "Preview/Structure/IDS";
pub const GEOMETRIES: &str = "Preview/Display/Scene0/Geometries";
pub const LIBRARIES: &str = "Preview/Display/Scene0/Libraries";

/// The keys every library of a preview holds: the default styles of construction and model.
const CONSTRUCTION_DEFAULT: u32 = 0x8000_0020;
const MODEL_DEFAULT: u32 = 0x8000_0027;

/// One place of a site in an assembly.
#[derive(Debug, Clone, PartialEq)]
pub struct Placement {
    /// The site that is placed.
    pub site: u32,
    /// `Bracket.par:2`.
    pub name: String,
    /// A 4 x 4 matrix, row by row, that takes a column vector in the coordinates of the site
    /// to the coordinates of the assembly.
    pub transform: [f64; 16],
}

/// The model of a part as the preview holds it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Model {
    pub faces: Vec<Face>,
    pub styles: Styles,
}

/// The preview of an assembly.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Preview {
    /// The site of the assembly itself.
    pub root: u32,
    /// The file name of each site, without folders, as the preview spells it (lower case).
    pub files: HashMap<u32, String>,
    /// What each assembly among the sites places.
    pub placements: HashMap<u32, Vec<Placement>>,
    /// The model of each part among the sites.
    pub models: HashMap<u32, Model>,
}

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

    fn u32(&mut self) -> Option<u32> {
        self.take(4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn string(&mut self) -> Option<String> {
        let units = self.u32()? as usize;
        let units: Vec<u16> = self.take(units.checked_mul(2)?)?.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        Some(String::from_utf16_lossy(&units))
    }

    fn expect(&mut self, word: &str) -> Option<()> {
        (self.string()? == word).then_some(())
    }
}

/// The file names of the sites, from the bytes of the stream of sites.
fn files(stream: &[u8]) -> Option<HashMap<u32, String>> {
    let mut reader = Reader { data: stream, at: 12 };
    let count = reader.u32()? as usize;
    let mut out = HashMap::new();
    for _ in 0..count.min(stream.len() / 12) {
        let site = reader.u32()?;
        reader.u32()?;
        let path = reader.string()?;
        out.insert(site, path.rsplit(['\\', '/']).next().unwrap_or(&path).to_string());
    }
    Some(out)
}

/// The site of the assembly itself and the placements of every assembly, from the bytes of the
/// stream of the structure.
fn structure(stream: &[u8]) -> Option<(u32, HashMap<u32, Vec<Placement>>)> {
    let mut reader = Reader { data: stream, at: 16 };
    reader.expect("Sites")?;
    let sites = reader.u32()? as usize;
    reader.take(8)?;
    let mut root = None;
    for _ in 0..sites.min(stream.len() / 72) {
        reader.expect("Site")?;
        // the first site is the assembly that was saved
        root.get_or_insert(reader.u32()?);
        reader.take(4 + 48 + 8)?;
    }
    reader.expect("Atts")?;
    let mut placements = HashMap::new();
    while reader.at < stream.len() {
        let (site, count) = (reader.u32()?, reader.u32()? as usize);
        let mut list = Vec::new();
        for _ in 0..count.min(stream.len() / 152) {
            reader.expect("Att")?;
            let child = reader.u32()?;
            reader.take(8)?;
            let mut stored = [0.0; 16];
            for value in &mut stored {
                *value = reader.take(8).map(|b| f64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])).filter(|v| v.is_finite())?;
            }
            // stored column by column
            let mut transform = [0.0; 16];
            for r in 0..4 {
                for c in 0..4 {
                    transform[r * 4 + c] = stored[c * 4 + r];
                }
            }
            list.push(Placement { site: child, name: reader.string()?, transform });
        }
        placements.insert(site, list);
    }
    Some((root?, placements))
}

/// The number of the site of `Cache12.1` or `Library12`.
fn site_of(name: &str, prefix: &str) -> Option<u32> {
    let number = name.strip_prefix(prefix)?;
    number.split_once('.').map_or(number, |(site, _)| site).parse().ok()
}

/// The preview of an assembly, when it has one that can be read.
pub fn read(document: &Document) -> Option<Preview> {
    let (root, placements) = structure(&document.raw(STRUCTURE).ok()?)?;
    let mut preview = Preview { root, files: files(&document.raw(SITES).ok()?)?, placements, models: HashMap::new() };
    let geometries = CompoundFile::open(&document.raw(GEOMETRIES).ok()?).ok()?;
    // a cache is a stream that holds a compound file, or a storage with the same streams
    let mut caches: Vec<(u32, &str)> = geometries.entries().iter().filter_map(|e| Some((site_of(e.path.split('/').next()?, "Cache")?, e.path.split('/').next()?))).collect();
    caches.sort();
    caches.dedup();
    for (site, name) in caches {
        let faces = match geometries.stream(name) {
            Ok(stream) => cache::read(&stream),
            Err(_) => cache::read_in(&geometries, &format!("{name}/")),
        };
        // the first cache of a site is its model
        if let Ok(faces) = faces {
            preview.models.entry(site).or_insert(Model { faces, styles: Styles::default() });
        }
    }
    if let Some(libraries) = document.raw(LIBRARIES).ok().and_then(|bytes| CompoundFile::open(&bytes).ok()) {
        for entry in libraries.entries() {
            let Some(model) = site_of(&entry.path, "Library").and_then(|site| preview.models.get_mut(&site)) else { continue };
            let Ok(library) = libraries.stream(&entry.path).and_then(|s| pack::inflate(&s)) else { continue };
            model.styles = styles::parse(&library, &[]);
            // The library holds the styles the part uses, and does not say which of them is the
            // style of the model. It is the one no face names, when there is exactly one such
            // style besides the two defaults; else the default of a model.
            let mut unused = model.styles.by_key.keys().copied().filter(|key| ![CONSTRUCTION_DEFAULT, MODEL_DEFAULT].contains(key) && !model.faces.iter().any(|f| f.style == Some(*key)));
            model.styles.model = match (unused.next(), unused.next()) {
                (Some(key), None) => Some(key),
                _ => Some(MODEL_DEFAULT),
            };
        }
    }
    Some(preview)
}
