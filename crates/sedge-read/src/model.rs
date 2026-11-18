//! The model a file is read to: triangle meshes, each with its name, colour, place and
//! properties.
//!
//! Lengths are metres and the axes are those of the file: Solid Edge draws Z up.

/// What a file holds.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scene {
    pub meshes: Vec<Mesh>,
    pub info: Info,
}

/// What kind of document a file is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Kind {
    /// A part or a sheet metal part: a model of its own.
    #[default]
    Part,
    /// An assembly: parts and other assemblies, placed.
    Assembly,
}

/// What was found in a file, and what was left out.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Info {
    pub kind: Kind,
    /// The placed parts of an assembly that were drawn, those of its sub-assemblies included
    /// (0 for a part).
    pub components: usize,
    /// The faces read (those of each part once, however often it is placed).
    pub faces: usize,
    /// The files of placed parts and sub-assemblies the caller did not give, or that hold
    /// nothing to draw: they are missing from the meshes. One entry per placement.
    pub missing: Vec<String>,
    /// Faces whose style is a reference that could not be followed: drawn in the style of
    /// their part.
    pub unresolved_colors: usize,
    /// Display caches of the files read that were not drawn: construction bodies and surfaces,
    /// the flat pattern of a sheet metal part.
    pub other_caches: usize,
    /// Whether the meshes come from the preview the assembly holds, because files of its parts
    /// were not given: every part is drawn, in a coarser tessellation than its own file has.
    pub preview: bool,
    /// Whether the list of placed parts of an assembly was read to its end. False when the
    /// list is damaged or laid out in a way not seen before: what was read before that point
    /// is drawn.
    pub complete: bool,
}

/// The colour of faces whose style is not found: the grey of the style a new part gets.
pub const DEFAULT_COLOR: [f32; 3] = [0.51, 0.53, 0.55];

/// The identity transform.
pub const IDENTITY: [f64; 16] = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];

/// Triangles of one style: of the model of a part, or of a placed part of an assembly.
#[derive(Debug, Clone, PartialEq)]
pub struct Mesh {
    /// The name of the placed part (`Bracket.par:2`), or `Model` in a part.
    pub name: String,
    /// xyz per vertex, in the coordinates of the part.
    pub positions: Vec<f32>,
    /// xyz per vertex, unit length, as the file has them.
    pub normals: Vec<f32>,
    /// Triangle list, counter-clockwise seen from outside.
    pub indices: Vec<u32>,
    /// sRGB, 0..1: the diffuse colour of the style.
    pub color: [f32; 3],
    /// 1 for an opaque style, less for a transparent one.
    pub opacity: f32,
    /// The name of the style (`Aluminum`), empty when the style was not found.
    pub style: String,
    /// Where the part is placed in the assembly: a 4 x 4 matrix, row by row, that takes a
    /// column vector in the coordinates of the part to the coordinates of the file. The
    /// identity in a part.
    pub transform: [f64; 16],
    /// Name and value: `Component` (the path through the assemblies), `Part`, `File`.
    pub props: Vec<(String, String)>,
    /// The placed part these triangles belong to: the meshes of one placed part (one per
    /// style) share this number. 0 in a part.
    pub instance: u32,
}

impl Default for Mesh {
    fn default() -> Self {
        Mesh {
            name: String::new(),
            positions: Vec::new(),
            normals: Vec::new(),
            indices: Vec::new(),
            color: DEFAULT_COLOR,
            opacity: 1.0,
            style: String::new(),
            transform: IDENTITY,
            props: Vec::new(),
            instance: 0,
        }
    }
}

impl Mesh {
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// The positions in the coordinates of the file: `transform` applied.
    pub fn world_positions(&self) -> Vec<f32> {
        let m = &self.transform;
        self.positions
            .chunks_exact(3)
            .flat_map(|p| {
                let (x, y, z) = (f64::from(p[0]), f64::from(p[1]), f64::from(p[2]));
                [0, 4, 8].map(|r| (m[r] * x + m[r + 1] * y + m[r + 2] * z + m[r + 3]) as f32)
            })
            .collect()
    }

    /// The normals in the coordinates of the file: turned with `transform`, unit length.
    pub fn world_normals(&self) -> Vec<f32> {
        let m = &self.transform;
        self.normals
            .chunks_exact(3)
            .flat_map(|n| {
                let (x, y, z) = (f64::from(n[0]), f64::from(n[1]), f64::from(n[2]));
                let v = [0, 4, 8].map(|r| m[r] * x + m[r + 1] * y + m[r + 2] * z);
                let length = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
                v.map(|c| if length > 0.0 { (c / length) as f32 } else { 0.0 })
            })
            .collect()
    }
}

impl Scene {
    pub fn triangle_count(&self) -> usize {
        self.meshes.iter().map(Mesh::triangle_count).sum()
    }
}

/// The product of two transforms: first `b`, then `a`.
pub fn multiply(a: &[f64; 16], b: &[f64; 16]) -> [f64; 16] {
    let mut out = [0.0; 16];
    for r in 0..4 {
        for c in 0..4 {
            out[r * 4 + c] = (0..4).map(|k| a[r * 4 + k] * b[k * 4 + c]).sum();
        }
    }
    out
}
