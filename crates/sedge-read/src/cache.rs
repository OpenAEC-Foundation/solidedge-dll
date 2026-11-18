//! The display cache: the triangles Solid Edge saved for showing a model, as triangle strips
//! per face.
//!
//! A cache (`Display/Cache0`) is a stream that holds a compound file of its own: `Info` and one
//! packed stream per level of detail (`L0-T1` the finest). A level is a list of faces, each
//! with its number, perhaps a style, and one mesh: points (f64), normals (f32) and more per
//! vertex. A point of three times the largest f64 ends a strip.
//!
//! Two versions of the layout were seen (the third byte of every tag: 1 or 2, and 1 or 3 for a
//! mesh). They differ in what follows the normals, which is not read, so both are read alike.

use sedge_core::{cfb::CompoundFile, pack, Error};

/// One face of a model: triangle strips.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Face {
    /// The number of the face in its body.
    pub id: u32,
    /// The style of the face, when it has one of its own: a key of the style library.
    pub style: Option<u32>,
    /// The colour of the face, when it has one of its own that is no style (seen in parts made
    /// from imported bodies): red, green, blue and opacity.
    pub color: Option<[u8; 4]>,
    /// Whether the face points at a style in a way that is not understood.
    pub unknown_style: bool,
    /// The points of the strips, one strip after the other.
    pub positions: Vec<[f32; 3]>,
    /// A normal per point.
    pub normals: Vec<[f32; 3]>,
    /// The number of points of each strip.
    pub strips: Vec<u32>,
}

impl Face {
    pub fn triangle_count(&self) -> usize {
        self.strips.iter().map(|&n| (n as usize).saturating_sub(2)).sum()
    }
}

const DAMAGED: Error = Error::Damaged("display cache");

/// The tags of the records of a level: the low half is the kind of record, the high half says
/// `02` and a version.
const LEVEL: u32 = 4;
const FACE: u32 = 3;
const MESH: u32 = 2;

fn kind(tag: u32) -> Option<u32> {
    (tag >> 24 == 2).then_some(tag & 0xFFFF)
}

fn u32_at(data: &[u8], at: usize) -> Result<u32, Error> {
    data.get(at..at.checked_add(4).ok_or(DAMAGED)?).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])).ok_or(DAMAGED)
}

/// The names of the levels of detail in a cache: `L<level>-T<type>`.
fn level_of(name: &str) -> Option<(u32, u32)> {
    let (level, kind) = name.strip_prefix('L')?.split_once("-T")?;
    Some((level.parse().ok()?, kind.parse().ok()?))
}

/// The faces of a cache, from the bytes of its stream: those of its finest level.
pub fn read(stream: &[u8]) -> Result<Vec<Face>, Error> {
    read_in(&CompoundFile::open(stream).map_err(|_| DAMAGED)?, "")
}

/// The faces of a cache that is a storage of a compound file: the finest of the levels in
/// `folder` (`Cache2.1/`, or nothing for the file itself).
pub fn read_in(file: &CompoundFile, folder: &str) -> Result<Vec<Face>, Error> {
    let levels = file.entries().iter().filter_map(|e| Some((level_of(e.path.strip_prefix(folder)?)?, &e.path)));
    let finest = levels.min().ok_or(Error::Damaged("display cache without a level"))?;
    parse(&pack::inflate(&file.stream(finest.1)?)?)
}

/// The faces of one level, from its inflated bytes.
pub fn parse(level: &[u8]) -> Result<Vec<Face>, Error> {
    if kind(u32_at(level, 0)?) != Some(LEVEL) {
        return Err(DAMAGED);
    }
    let (mut at, mut faces) = (4, Vec::new());
    // a face needs at least its two headers; the level ends with one closing byte
    while at + 16 <= level.len() {
        // the face: its size, then a kind, its number, perhaps style words, a count and a box
        let size = u32_at(level, at + 4)? as usize;
        if kind(u32_at(level, at)?) != Some(FACE) || !(61..=1024).contains(&size) || size % 4 != 1 {
            return Err(DAMAGED);
        }
        let head = level.get(at + 8..at + 8 + size - 1).ok_or(DAMAGED)?;
        let mut face = Face { id: u32_at(head, 4)?, ..Face::default() };
        for word in head[8..head.len() - 52].chunks_exact(4).map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]])) {
            match word {
                0 => {}
                // the keys of the style library all have this top byte
                key if key >> 24 == 0x80 => face.style = Some(key),
                // a colour: four bits each of opacity, blue, green and red
                word if word >> 16 == 0x4006 => face.color = Some([word, word >> 4, word >> 8, word >> 12].map(|v| (v & 0xF) as u8 * 17)),
                _ => face.unknown_style = true,
            }
        }
        at += 8 + size - 1;
        // its mesh: the size of what follows, the number of vertices (strip ends included), the
        // number of strips, flags, then an array per vertex
        let size = u32_at(level, at + 4)? as usize;
        if kind(u32_at(level, at)?) != Some(MESH) {
            return Err(DAMAGED);
        }
        let mesh = level.get(at + 8..(at + 8).checked_add(size).ok_or(DAMAGED)?).ok_or(DAMAGED)?;
        let count = u32_at(mesh, 0)? as usize;
        let (points, normals) = (12, count.checked_mul(24).ok_or(DAMAGED)? + 12);
        if count.checked_mul(36).is_none_or(|bytes| bytes + 12 > mesh.len()) {
            return Err(DAMAGED);
        }
        let mut length = 0;
        for k in 0..count {
            let p = &mesh[points + 24 * k..points + 24 * k + 24];
            let point = [0, 8, 16].map(|o| f64::from_le_bytes([p[o], p[o + 1], p[o + 2], p[o + 3], p[o + 4], p[o + 5], p[o + 6], p[o + 7]]));
            if point[0] == f64::MAX {
                face.strips.push(length);
                length = 0;
                continue;
            }
            let n = &mesh[normals + 12 * k..normals + 12 * k + 12];
            let normal = [0, 4, 8].map(|o| f32::from_le_bytes([n[o], n[o + 1], n[o + 2], n[o + 3]]));
            // a point that is no number would spoil the bounds of the whole model
            let point = point.map(|v| if v.is_finite() { v as f32 } else { 0.0 });
            face.positions.push(point);
            face.normals.push(normal.map(|v| if v.is_finite() { v } else { 0.0 }));
            length += 1;
        }
        face.strips.push(length);
        face.strips.retain(|&n| n > 0);
        faces.push(face);
        // one byte closes the face
        at += 8 + size + 1;
    }
    Ok(faces)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{self, quad};

    #[test]
    fn a_level_gives_its_faces_and_strips() {
        let mut second = quad(7, [0.0, 0.0, 1.0], Some(0x8000_0024));
        second.positions.extend([[5.0, 0.0, 0.0], [6.0, 0.0, 0.0], [5.0, 1.0, 0.0]]);
        second.normals.extend([[0.0, 0.0, 1.0]; 3]);
        second.strips.push(3);
        let faces = vec![quad(3, [0.0, 0.0, 0.0], None), second];
        for version in [1, 2] {
            assert_eq!(parse(&testing::level(&faces, version)).unwrap(), faces, "version {version}");
        }
        assert_eq!((faces[0].triangle_count(), faces[1].triangle_count()), (2, 3));
    }

    #[test]
    fn the_finest_level_of_a_cache_is_read() {
        let (fine, coarse) = (vec![quad(1, [0.0; 3], None), quad(2, [0.0, 0.0, 1.0], None)], vec![quad(1, [0.0; 3], None)]);
        let stream = testing::cache(&[("L2-T1", &coarse), ("L0-T1", &fine), ("L1-T1", &coarse)], 2);
        assert_eq!(read(&stream).unwrap(), fine);
        // a cache saved with one coarser level only
        assert_eq!(read(&testing::cache(&[("L1-T1", &coarse)], 2)).unwrap(), coarse);
        assert_eq!(read(&testing::cache(&[], 2)), Err(Error::Damaged("display cache without a level")));
        assert_eq!(read(b"no compound file"), Err(DAMAGED));
    }

    #[test]
    fn a_face_that_points_elsewhere_for_its_style_says_so() {
        let mut level = testing::level(&[quad(1, [0.0; 3], Some(0x8000_0024))], 2);
        // the style word of the face: at the face header (4 + 8) plus kind and number
        level[20..24].copy_from_slice(&0x1234_5678u32.to_le_bytes());
        let faces = parse(&level).unwrap();
        assert_eq!((faces[0].style, faces[0].color, faces[0].unknown_style), (None, None, true));
    }

    #[test]
    fn a_face_may_carry_its_colour_itself() {
        let mut level = testing::level(&[quad(1, [0.0; 3], Some(0x8000_0024))], 2);
        // opacity F, blue 0, green E, red E: yellow
        level[20..24].copy_from_slice(&0x4006_F0EEu32.to_le_bytes());
        let faces = parse(&level).unwrap();
        assert_eq!((faces[0].style, faces[0].color, faces[0].unknown_style), (None, Some([238, 238, 0, 255]), false));
    }

    #[test]
    fn a_damaged_level_is_an_error_not_a_panic() {
        let good = testing::level(&[quad(1, [0.0; 3], None), quad(2, [0.0, 0.0, 1.0], Some(0x8000_0001))], 2);
        assert_eq!(parse(&good).unwrap().len(), 2);
        for cut in 0..good.len() {
            // a cut inside a record is an error; a cut between two faces leaves the first, and
            // the bytes that close the last face and the level are not missed, and
            // the bytes that close the last face and the level are not missed
            if let Ok(faces) = parse(&good[..cut]) {
                assert!(faces.len() < 2 || cut + 2 >= good.len(), "cut at {cut}");
            }
        }
        // a mesh that says it has more vertices than it has bytes
        let mut lying = good.clone();
        let at = 4 + 8 + 60 + 8;
        lying[at..at + 4].copy_from_slice(&1_000_000u32.to_le_bytes());
        assert_eq!(parse(&lying), Err(DAMAGED));
        lying[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(parse(&lying), Err(DAMAGED));
        assert_eq!(parse(&[9, 9, 9, 9, 9]), Err(DAMAGED));
    }
}
