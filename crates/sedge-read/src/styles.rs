//! The styles of a document: the face styles of its library (`Display/Styles/Library0`, packed)
//! and which of them the model takes when a face has none of its own
//! (`Display/Styles/DefaultStyles`).
//!
//! The library is a row of records, each closed by a byte `ED`. A record has no size and the
//! records differ in length (a style may name textures), so they are found by their start: the
//! tag, a word, the kind, a key with the top byte `80`, and the name. Of a face style the
//! diffuse colour and the transparency are read.

use std::collections::HashMap;

/// A face style.
#[derive(Debug, Clone, PartialEq)]
pub struct Style {
    pub name: String,
    /// The diffuse colour, sRGB 0..1.
    pub color: [f32; 3],
    /// 1 for opaque.
    pub opacity: f32,
}

/// The face styles of a document by key, and the keys of its defaults.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Styles {
    pub by_key: HashMap<u32, Style>,
    /// The style of the model: of every face without a style of its own.
    pub model: Option<u32>,
    /// The style of construction bodies and surfaces.
    pub construction: Option<u32>,
}

/// The kinds of record that are face styles; the other kind seen (3) is a view style.
const FACE_STYLES: [u32; 2] = [5, 10];
/// After the name: sixteen bytes, then five colours of four floats (ambient, diffuse, specular,
/// emission and one more), then shininess, transparency, reflectivity and refraction.
const DIFFUSE: usize = 16 + 16;
const TRANSPARENCY: usize = 16 + 84;
const BODY: usize = 16 + 96;

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    data.get(at..at.checked_add(4)?).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn f32_at(data: &[u8], at: usize) -> Option<f32> {
    u32_at(data, at).map(f32::from_bits)
}

/// The style whose record starts at `at`, with its key.
fn style_at(library: &[u8], at: usize) -> Option<(u32, Style)> {
    let tag = library.get(at..at + 4)?;
    if tag[0] != 7 || tag[1] != 0 || tag[3] != 2 || !FACE_STYLES.contains(&u32_at(library, at + 8)?) {
        return None;
    }
    let (key, length) = (u32_at(library, at + 12)?, u32_at(library, at + 16)? as usize);
    if key >> 24 != 0x80 || length > 255 {
        return None;
    }
    let body = at + 20 + 2 * length;
    let name: Vec<u16> = library.get(at + 20..body)?.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    let name = String::from_utf16(&name).ok()?;
    let unit = |v: f32| (0.0..=1.0).contains(&v);
    let color = [0, 4, 8].map(|o| f32_at(library, body + DIFFUSE + o));
    let color = [color[0]?, color[1]?, color[2]?];
    let transparency = f32_at(library, body + TRANSPARENCY)?;
    library.get(body + BODY - 1)?;
    (color.iter().all(|&v| unit(v)) && unit(transparency)).then_some((key, Style { name, color, opacity: 1.0 - transparency }))
}

/// The face styles of a library, from its inflated bytes, and the defaults of the document,
/// from the bytes of its stream (a count, then the keys of the model and construction styles).
pub fn parse(library: &[u8], defaults: &[u8]) -> Styles {
    let mut styles = Styles { model: u32_at(defaults, 4).filter(|k| k >> 24 == 0x80), construction: u32_at(defaults, 8).filter(|k| k >> 24 == 0x80), ..Styles::default() };
    let mut at = 0;
    while at + 20 < library.len() {
        // a record starts the library or follows the closing byte of the one before
        let found = (at == 0 || library[at - 1] == 0xED).then(|| style_at(library, at)).flatten();
        match found {
            Some((key, style)) => {
                at += 20 + 2 * style.name.encode_utf16().count() + BODY;
                styles.by_key.entry(key).or_insert(style);
            }
            None => at += 1,
        }
    }
    styles
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;

    #[test]
    fn face_styles_come_with_colour_and_opacity() {
        let library = testing::style_library(&[
            (0x8000_0027, "Model Default", [0.51, 0.53, 0.55], 1.0),
            (0x8000_002D, "Rojo (transparente)", [0.84, 0.0, 0.0], 0.375),
            (0x8000_006B, "Алюминий", [0.82, 0.82, 0.82], 1.0),
        ]);
        let styles = parse(&library, &testing::default_styles(0x8000_006B));
        assert_eq!(styles.by_key.len(), 3);
        assert_eq!(styles.by_key[&0x8000_002D], Style { name: "Rojo (transparente)".into(), color: [0.84, 0.0, 0.0], opacity: 0.375 });
        assert_eq!(styles.by_key[&0x8000_006B].name, "Алюминий");
        assert_eq!((styles.model, styles.construction), (Some(0x8000_006B), Some(0x8000_0020)));
    }

    #[test]
    fn other_bytes_give_no_styles() {
        assert_eq!(parse(&[], &[]), Styles::default());
        assert!(parse(&[0xED; 500], &[1, 0, 0, 0]).by_key.is_empty());
        // a record cut short, and bytes that look like a start but hold no colours
        let library = testing::style_library(&[(0x8000_0001, "Steel", [0.3, 0.3, 0.3], 1.0)]);
        for cut in 0..library.len() {
            let _ = parse(&library[..cut], &[]);
        }
        let mut wrong = library.clone();
        let diffuse = wrong.windows(4).position(|w| w == 0.3f32.to_le_bytes()).unwrap();
        wrong[diffuse..diffuse + 4].copy_from_slice(&7.5f32.to_le_bytes());
        assert!(parse(&wrong, &[]).by_key.is_empty());
    }
}
