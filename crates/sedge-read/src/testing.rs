//! Builders of small Solid Edge files for tests, here and in programs that use this crate: a
//! part with a display cache and styles, an assembly that places files. They write what this
//! crate reads; they are no general writers.

use crate::cache::Face;
use sedge_core::testing::{compound_file, pack};

/// A unit square in the plane z = `origin[2]`, from `origin`, facing +z: one strip of four.
pub fn quad(id: u32, origin: [f32; 3], style: Option<u32>) -> Face {
    let [x, y, z] = origin;
    Face { id, style, color: None, unknown_style: false, positions: vec![[x, y, z], [x + 1.0, y, z], [x, y + 1.0, z], [x + 1.0, y + 1.0, z]], normals: vec![[0.0, 0.0, 1.0]; 4], strips: vec![4] }
}

/// A transform that moves by (x, y, z).
pub fn moved(x: f64, y: f64, z: f64) -> [f64; 16] {
    [1.0, 0.0, 0.0, x, 0.0, 1.0, 0.0, y, 0.0, 0.0, 1.0, z, 0.0, 0.0, 0.0, 1.0]
}

/// One level of detail of a display cache, not packed. `version` 1 or 2: the two layouts.
pub fn level(faces: &[Face], version: u8) -> Vec<u8> {
    let mut out = vec![4, 0, version, 2];
    for face in faces {
        // a style of the library, or a colour: four bits each of opacity, blue, green and red
        let colour = face.color.map(|c| c.map(|v| u32::from(v) / 17)).map(|[r, g, b, a]| 0x4006_0000 | a << 12 | b << 8 | g << 4 | r);
        let style: Vec<u8> = face.style.or(colour).map(|word| word.to_le_bytes().to_vec()).unwrap_or_default();
        out.extend([3, 0, version, 2]);
        out.extend((61 + style.len() as u32).to_le_bytes());
        out.extend(0x46u32.to_le_bytes());
        out.extend(face.id.to_le_bytes());
        out.extend(&style);
        out.extend(1u32.to_le_bytes());
        // the box of a face is saved empty
        out.extend([1e30f64, 1e30, 1e30, -1e30, -1e30, -1e30].iter().flat_map(|v| v.to_le_bytes()));
        // the vertices: the strips with an end mark between them
        let mut vertices: Vec<Option<usize>> = Vec::new();
        let mut first = 0;
        for (k, &length) in face.strips.iter().enumerate() {
            if k > 0 {
                vertices.push(None);
            }
            vertices.extend((first..first + length as usize).map(Some));
            first += length as usize;
        }
        let mut mesh = Vec::new();
        mesh.extend((vertices.len() as u32).to_le_bytes());
        mesh.extend((face.strips.len().saturating_sub(1) as u32).to_le_bytes());
        mesh.extend([0x52, 0x03, 0xA0, 0x28]);
        for vertex in &vertices {
            let point = vertex.map_or([f64::MAX; 3], |v| face.positions[v].map(f64::from));
            mesh.extend(point.iter().flat_map(|v| v.to_le_bytes()));
        }
        for vertex in &vertices {
            // the slot of an end mark holds whatever was in memory
            let normal = vertex.map_or([f32::NAN, 7.0, -3.0], |v| face.normals[v]);
            mesh.extend(normal.iter().flat_map(|v| v.to_le_bytes()));
        }
        // texture coordinates, a flag and (in the second layout) a number per vertex, a tail
        mesh.resize(mesh.len() + vertices.len() * if version == 1 { 9 } else { 13 }, 0x23);
        mesh.resize(mesh.len() + if version == 1 { 0 } else { 51 }, 0);
        mesh.push(0xED);
        out.extend([2, 0, if version == 1 { 1 } else { 3 }, 2]);
        out.extend((mesh.len() as u32).to_le_bytes());
        out.extend(mesh);
        out.push(0xED);
    }
    out.push(0xED);
    out
}

/// The stream of a display cache: a compound file with `Info` and these levels
/// (`("L0-T1", faces)`), packed.
pub fn cache(levels: &[(&str, &[Face])], version: u8) -> Vec<u8> {
    let packed: Vec<(&str, Vec<u8>)> = levels.iter().map(|(name, faces)| (*name, pack(&level(faces, version), 65536))).collect();
    let info = pack(&[5, 0, version, 2, 0x55, 0, 0, 0], 65536);
    let mut streams: Vec<(&str, &[u8])> = vec![("Info", &info)];
    streams.extend(packed.iter().map(|(name, bytes)| (*name, bytes.as_slice())));
    compound_file(&streams)
}

/// A style library, not packed: face styles with key, name, diffuse colour and opacity.
pub fn style_library(styles: &[(u32, &str, [f32; 3], f32)]) -> Vec<u8> {
    let mut out = Vec::new();
    // a view style first: another kind of record, with floats where a face style has colours
    for (kind, key, name, color, opacity) in std::iter::once((3u32, 0x8000_0008u32, "Default", [1.0f32; 3], 1.0f32)).chain(styles.iter().map(|s| (5, s.0, s.1, s.2, s.3))) {
        out.extend([7, 0, 2, 2]);
        out.extend(5u32.to_le_bytes());
        out.extend(kind.to_le_bytes());
        out.extend(key.to_le_bytes());
        let name: Vec<u16> = name.encode_utf16().collect();
        out.extend((name.len() as u32).to_le_bytes());
        out.extend(name.iter().flat_map(|u| u.to_le_bytes()));
        out.extend([0, 0, 0, 0, 0, 0, 0x80, 0x3F, 0, 0, 0x80, 0x3F, 1, 0, 0xFF, 0xFF]);
        let [r, g, b] = color;
        // ambient, diffuse, specular, emission, one more; shininess, transparency, two more
        let floats = [r / 2.0, g / 2.0, b / 2.0, 1.0, r, g, b, 1.0, 0.9, 0.9, 0.9, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.8, 1.0 - opacity, 0.1, 1.0];
        out.extend(floats.iter().flat_map(|v| v.to_le_bytes()));
        out.resize(out.len() + 300, 0);
        out.push(0xED);
    }
    out
}

/// The stream of the default styles of a document whose model has the style `model`.
pub fn default_styles(model: u32) -> Vec<u8> {
    [1, model, 0x8000_0020].iter().flat_map(|v: &u32| v.to_le_bytes()).collect()
}

fn moniker(path: &str, up: u16) -> Vec<u8> {
    let mut out = vec![0x03, 0x03, 0, 0, 0, 0, 0, 0, 0xC0, 0, 0, 0, 0, 0, 0, 0x46];
    out.extend(up.to_le_bytes());
    let narrow: Vec<u8> = path.chars().map(|c| if c.is_ascii() { c as u8 } else { b'?' }).chain([0]).collect();
    out.extend((narrow.len() as u32).to_le_bytes());
    out.extend(narrow);
    out.extend([0xFF, 0xFF, 0xAD, 0xDE]);
    out.extend([0; 20]);
    if path.is_ascii() {
        out.extend(0u32.to_le_bytes());
    } else {
        let wide: Vec<u8> = path.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        out.extend((wide.len() as u32 + 6).to_le_bytes());
        out.extend((wide.len() as u32).to_le_bytes());
        out.extend(3u16.to_le_bytes());
        out.extend(wide);
    }
    out
}

/// A document an assembly places: its full path, its path from the folder of the assembly and
/// its placements (name, transform).
pub type Placed<'a> = (&'a str, &'a str, &'a [(&'a str, [f64; 16])]);

/// The `Attachments` stream of an assembly that places these documents. `version` 5 or 7.
pub fn attachments(documents: &[Placed], version: u32) -> Vec<u8> {
    let placed = documents.iter().map(|d| d.2.len() as u32).sum::<u32>();
    let mut out = vec![0xB0, 0x9D, 0xD8, 0xDE, 0xB6, 0x45, 0xCE, 0x11, 0xB3, 0x07, 0x08, 0x00, 0x36, 0x3A, 0x1E, 0x02];
    for word in [version, documents.len() as u32, placed, documents.len() as u32, placed, 0, 0, 0] {
        out.extend(word.to_le_bytes());
    }
    for (k, (full, relative, placements)) in documents.iter().enumerate() {
        out.extend(moniker(full, 0));
        out.extend(1u32.to_le_bytes());
        out.extend(moniker(relative, 1));
        for word in [placements.len() as u32, 8, 342 + k as u32, 0xA000_0016] {
            out.extend(word.to_le_bytes());
        }
        for (name, transform) in placements.iter() {
            // column by column
            for c in 0..4 {
                for r in 0..4 {
                    out.extend(transform[r * 4 + c].to_le_bytes());
                }
            }
            out.extend([0u8; 48]);
            let mut rest = Vec::new();
            let wide: Vec<u8> = name.encode_utf16().chain([0]).flat_map(|u| u.to_le_bytes()).collect();
            rest.extend((wide.len() as u32).to_le_bytes());
            rest.extend(wide);
            rest.extend([0x23; 28]);
            if version >= 7 {
                rest.extend([0; 4]);
            }
            out.extend((rest.len() as u32).to_le_bytes());
            out.extend(rest);
        }
    }
    // what follows the documents: a list that is not read
    out.extend([0, 0, 0, 0, 5, 0, 0, 0]);
    out
}

/// The streams every document has next to its model.
const ROOTS: (&str, &[u8]) = ("PSMroots", b"root\0\0\0\0");

/// A part (or sheet metal part) file: its model as faces, its styles (key, name, colour,
/// opacity) and the key of the style of the model. `others` are further display caches
/// (`"Display/Cache2"`), each one unit square.
pub fn part(faces: &[Face], styles: &[(u32, &str, [f32; 3], f32)], model: u32, others: &[&str]) -> Vec<u8> {
    let model_cache = cache(&[("L0-T1", faces), ("L1-T1", &faces[..faces.len().min(1)])], 2);
    let other_cache = cache(&[("L0-T1", &[quad(1, [50.0, 50.0, 50.0], None)])], 2);
    let (library, defaults) = (pack(&style_library(styles), 65536), default_styles(model));
    let mut streams: Vec<(&str, &[u8])> = vec![ROOTS, ("PartsLiteData", &[7, 0, 0, 0]), ("Display/Styles/Library0", &library), ("Display/Styles/DefaultStyles", &defaults)];
    if !faces.is_empty() {
        streams.push(("Display/Cache0", &model_cache));
    }
    streams.extend(others.iter().map(|name| (*name, other_cache.as_slice())));
    compound_file(&streams)
}

/// An assembly file that places these documents (see [`attachments`]).
pub fn assembly(documents: &[Placed]) -> Vec<u8> {
    assembly_with_preview(documents, &[])
}

/// A file of the preview of an assembly: its number, its path, the faces and styles of its
/// model (a part) and the sites it places with their names and transforms (an assembly).
/// `storage` says how its display cache is kept: as a storage, or as a stream that is a
/// compound file.
pub struct Site<'a> {
    pub number: u32,
    pub path: &'a str,
    pub faces: &'a [Face],
    pub styles: &'a [(u32, &'a str, [f32; 3], f32)],
    pub places: &'a [(u32, &'a str, [f64; 16])],
    pub storage: bool,
}

fn string(text: &str) -> Vec<u8> {
    let units: Vec<u16> = text.encode_utf16().collect();
    (units.len() as u32).to_le_bytes().into_iter().chain(units.iter().flat_map(|u| u.to_le_bytes())).collect()
}

/// An assembly file that places these documents and holds a preview of these sites; the first
/// site is the assembly itself.
pub fn assembly_with_preview(documents: &[Placed], sites: &[Site]) -> Vec<u8> {
    let list = attachments(documents, 7);
    let mut streams: Vec<(String, Vec<u8>)> = Vec::new();
    if !sites.is_empty() {
        let words = |values: &[u32]| values.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<u8>>();
        let mut names = words(&[1, sites.len() as u32 + 1, 0, sites.len() as u32]);
        let assemblies = sites.iter().filter(|s| !s.places.is_empty()).count() as u32;
        let mut structure = words(&[1, 3, 2, 1]);
        structure.extend(string("Sites"));
        structure.extend(words(&[sites.len() as u32, assemblies, sites.len() as u32 - assemblies]));
        for site in sites {
            names.extend(words(&[site.number, 0]));
            names.extend(string(&site.path.to_lowercase()));
            structure.extend(string("Site"));
            structure.extend(words(&[site.number, if site.places.is_empty() { 137 } else { 0x0040_0002 }]));
            structure.extend([0u8; 48]);
            structure.extend(words(&[0x1234_5678, u32::MAX]));
        }
        structure.extend(string("Atts"));
        for site in sites.iter().filter(|s| !s.places.is_empty()) {
            structure.extend(words(&[site.number, site.places.len() as u32]));
            for (k, (child, name, transform)) in site.places.iter().enumerate() {
                structure.extend(string("Att"));
                structure.extend(words(&[*child, 354 + k as u32, 0]));
                // column by column
                for c in 0..4 {
                    for r in 0..4 {
                        structure.extend(transform[r * 4 + c].to_le_bytes());
                    }
                }
                structure.extend(string(name));
            }
        }
        let mut caches: Vec<(String, Vec<u8>)> = vec![("Summary".into(), vec![0; 12])];
        let mut libraries = caches.clone();
        for site in sites.iter().filter(|s| !s.faces.is_empty()) {
            if site.storage {
                caches.push((format!("Cache{}.1/Info", site.number), pack(&[5, 0, 2, 2], 65536)));
                caches.push((format!("Cache{}.1/L1-T1", site.number), pack(&level(site.faces, 2), 65536)));
            } else {
                caches.push((format!("Cache{}.1", site.number), cache(&[("L1-T1", site.faces)], 2)));
            }
            libraries.push((format!("Library{}", site.number), pack(&style_library(site.styles), 65536)));
        }
        let file = |entries: &[(String, Vec<u8>)]| compound_file(&entries.iter().map(|(name, bytes)| (name.as_str(), bytes.as_slice())).collect::<Vec<_>>());
        streams.push(("Preview/Structure/Sites".into(), names));
        streams.push(("Preview/Structure/IDS".into(), structure));
        streams.push(("Preview/Display/Scene0/Geometries".into(), file(&caches)));
        streams.push(("Preview/Display/Scene0/Libraries".into(), file(&libraries)));
    }
    let mut all: Vec<(&str, &[u8])> = vec![ROOTS, ("PartsLiteData", &[7, 0, 0, 0]), ("Attachments", &list)];
    all.extend(streams.iter().map(|(name, bytes)| (name.as_str(), bytes.as_slice())));
    compound_file(&all)
}
