//! Reader of Solid Edge parts (`.par`), sheet metal parts (`.psm`) and assemblies (`.asm`).
//!
//! What is read is the display cache of a part: the triangles Solid Edge itself saved for
//! showing the model ([`cache`]), in the colours of its face styles ([`styles`]). An assembly
//! names the files of its parts and where each is placed ([`attachments`]), so
//! [`read_assembly`] asks the caller for the bytes of those files, by file name. When files are
//! not given, an assembly of a newer version is drawn from the preview it holds ([`preview`]):
//! all its parts, more coarsely. The exact geometry (the Parasolid bodies), the feature tree,
//! sketches and dimensions are not read.
//!
//! Nothing here touches the file system, so the reader also runs in the browser.

#![forbid(unsafe_code)]

pub mod attachments;
pub mod cache;
pub mod model;
pub mod preview;
pub mod styles;
pub mod testing;

use cache::Face;
use model::{Kind, Mesh, Scene};
pub use sedge_core::Error;
use sedge_core::{is_compound_file, Document};
use std::collections::HashMap;
use std::rc::Rc;

/// The display caches of a part are numbered: `Display/Cache0`, `Display/Cache2`. The one with
/// the lowest number is the model; the others hold construction bodies and surfaces, and the
/// flat pattern of a sheet metal part.
pub const CACHE: &str = "Display/Cache";
/// The style library of a document, packed.
pub const STYLE_LIBRARY: &str = "Display/Styles/Library0";
/// Which styles of the library the model takes.
pub const DEFAULT_STYLES: &str = "Display/Styles/DefaultStyles";
/// The placed parts of an assembly.
pub const ATTACHMENTS: &str = "Attachments";

/// Assemblies inside assemblies deeper than this are not followed.
const MAX_DEPTH: usize = 24;

/// Whether the bytes are a Solid Edge document: a compound file with the streams Solid Edge
/// writes. Files of other programs with the same extension (`.par`, `.asm`) are not.
pub fn is_solid_edge(data: &[u8]) -> bool {
    is_compound_file(data) && Document::open(data).is_ok()
}

/// Reads a part, or an assembly without the files of its parts: from its preview when it has
/// one (`Scene::info.preview`), else there is nothing to draw.
pub fn read(data: &[u8]) -> Result<Scene, Error> {
    read_assembly(data, &mut |_| None)
}

/// Reads a part or an assembly. `fetch` is asked for the bytes of the file of a placed part or
/// sub-assembly, by its file name (`Bracket.par`, without folders); it answers `None` when it
/// does not have the file. Each file is asked for once. When a file is not given and the
/// assembly holds a preview that draws more of its parts, the preview is drawn instead.
pub fn read_assembly(data: &[u8], fetch: &mut dyn FnMut(&str) -> Option<Vec<u8>>) -> Result<Scene, Error> {
    let top = file(data)?;
    let mut scene = Scene::default();
    scene.info.complete = top.complete;
    scene.info.faces = top.faces;
    scene.info.unresolved_colors = top.unresolved_colors;
    scene.info.other_caches = top.other_caches;
    for shape in &top.shapes {
        scene.meshes.push(Mesh { name: "Model".to_string(), ..shape.mesh() });
    }
    if top.assembly {
        scene.info.kind = Kind::Assembly;
        let mut placer = Placer { fetch, files: HashMap::new(), scene: &mut scene, instance: u32::from(!top.shapes.is_empty()), open: Vec::new() };
        placer.place(&top, &model::IDENTITY, "");
        if !scene.info.missing.is_empty() || scene.meshes.is_empty() {
            if let Some(drawn) = from_preview(data).filter(|drawn| drawn.info.components > scene.info.components) {
                scene = drawn;
            }
        }
    }
    if scene.meshes.is_empty() {
        return Err(Error::NoGeometry(match (top.assembly, top.children.is_empty()) {
            (false, _) => "the file holds no model with a saved tessellation (an empty part, or sketches only)",
            (true, true) => "the assembly places no parts",
            (true, false) => "none of the parts of the assembly was found",
        }));
    }
    Ok(scene)
}

/// What a face is painted with.
#[derive(Clone, Copy, Default, PartialEq)]
enum Paint {
    /// A style of the library, by its key.
    Style(u32),
    /// A colour of its own.
    Color([u8; 4]),
    /// Nothing that was found: the default colour.
    #[default]
    Plain,
}

/// The triangles of one style or colour of a model.
#[derive(Default)]
struct Shape {
    paint: Paint,
    style: String,
    color: [f32; 3],
    opacity: f32,
    positions: Vec<f32>,
    normals: Vec<f32>,
    indices: Vec<u32>,
}

impl Shape {
    fn mesh(&self) -> Mesh {
        Mesh { positions: self.positions.clone(), normals: self.normals.clone(), indices: self.indices.clone(), color: self.color, opacity: self.opacity, style: self.style.clone(), ..Mesh::default() }
    }
}

/// A placed file of an assembly.
struct Child {
    name: String,
    file: String,
    transform: [f64; 16],
}

/// What one file holds: its model by style, and the files it places.
struct File {
    assembly: bool,
    shapes: Vec<Shape>,
    children: Vec<Child>,
    faces: usize,
    unresolved_colors: usize,
    other_caches: usize,
    complete: bool,
}

/// Reads one file, without the files it places.
fn file(data: &[u8]) -> Result<File, Error> {
    let document = Document::open(data)?;
    let mut out = File { assembly: document.has(ATTACHMENTS), shapes: Vec::new(), children: Vec::new(), faces: 0, unresolved_colors: 0, other_caches: 0, complete: true };
    let mut caches: Vec<(u32, &str)> = document.streams().filter_map(|(path, _)| Some((path.strip_prefix(CACHE)?.parse().ok()?, path))).collect();
    caches.sort();
    out.other_caches = caches.len().saturating_sub(1);
    if let Some((_, model)) = caches.first() {
        let faces = cache::read(&document.raw(model)?)?;
        // a document without styles is drawn in the default colour
        let library = document.packed(STYLE_LIBRARY).unwrap_or_default();
        let styles = styles::parse(&library, &document.raw(DEFAULT_STYLES).unwrap_or_default());
        out.faces = faces.len();
        out.unresolved_colors = faces.iter().filter(|f| f.unknown_style || f.style.is_some_and(|key| !styles.by_key.contains_key(&key))).count();
        out.shapes = shapes(&faces, &styles);
    }
    if out.assembly {
        let list = attachments::parse(&document.raw(ATTACHMENTS)?);
        out.complete = list.complete;
        for reference in list.references {
            out.children.extend(reference.placements.into_iter().map(|p| Child { name: p.name, file: reference.file.clone(), transform: p.transform }));
        }
    }
    Ok(out)
}

/// Walks an assembly and puts the meshes of its placed parts in the scene.
struct Placer<'a> {
    fetch: &'a mut dyn FnMut(&str) -> Option<Vec<u8>>,
    /// The files asked for so far, by file name in lower case.
    files: HashMap<String, Option<Rc<File>>>,
    scene: &'a mut Scene,
    instance: u32,
    /// The assemblies being walked, against an assembly that places itself.
    open: Vec<String>,
}

impl Placer<'_> {
    fn place(&mut self, assembly: &File, transform: &[f64; 16], path: &str) {
        for child in &assembly.children {
            let key = child.file.to_lowercase();
            if !self.files.contains_key(&key) {
                let read = (self.fetch)(&child.file).and_then(|bytes| file(&bytes).ok()).map(Rc::new);
                if let Some(read) = &read {
                    self.scene.info.faces += read.faces;
                    self.scene.info.unresolved_colors += read.unresolved_colors;
                    self.scene.info.other_caches += read.other_caches;
                    self.scene.info.complete &= read.complete;
                }
                self.files.insert(key.clone(), read);
            }
            let Some(part) = self.files[&key].clone().filter(|p| !p.shapes.is_empty() || !p.children.is_empty()) else {
                self.scene.info.missing.push(child.file.clone());
                continue;
            };
            let placed = model::multiply(transform, &child.transform);
            let component = if path.is_empty() { child.name.clone() } else { format!("{path}/{}", child.name) };
            if !part.shapes.is_empty() {
                let stem = child.file.rsplit_once('.').map_or(child.file.as_str(), |(stem, _)| stem);
                let props = vec![("Component".to_string(), component.clone()), ("Part".to_string(), stem.to_string()), ("File".to_string(), child.file.clone())];
                for shape in &part.shapes {
                    self.scene.meshes.push(Mesh { name: child.name.clone(), transform: placed, props: props.clone(), instance: self.instance, ..shape.mesh() });
                }
                self.instance += 1;
                self.scene.info.components += 1;
            }
            if !part.children.is_empty() && self.open.len() < MAX_DEPTH && !self.open.contains(&key) {
                self.open.push(key);
                self.place(&part, &placed, &component);
                self.open.pop();
            }
        }
    }
}

/// An assembly drawn from the preview it holds, when it has one.
fn from_preview(data: &[u8]) -> Option<Scene> {
    let preview = preview::read(&Document::open(data).ok()?)?;
    let mut scene = Scene::default();
    scene.info.kind = Kind::Assembly;
    scene.info.preview = true;
    scene.info.complete = true;
    let mut models = HashMap::new();
    for (site, model) in &preview.models {
        scene.info.faces += model.faces.len();
        scene.info.unresolved_colors += model.faces.iter().filter(|f| f.unknown_style || f.style.is_some_and(|key| !model.styles.by_key.contains_key(&key))).count();
        models.insert(*site, shapes(&model.faces, &model.styles));
    }
    let mut instance = 0;
    // the sites to place: site, transform, path of the component, depth
    let mut todo = vec![(preview.root, model::IDENTITY, String::new(), 0usize)];
    while let Some((site, transform, path, depth)) = todo.pop() {
        let mut inner = Vec::new();
        for placement in preview.placements.get(&site).into_iter().flatten() {
            let placed = model::multiply(&transform, &placement.transform);
            let component = if path.is_empty() { placement.name.clone() } else { format!("{path}/{}", placement.name) };
            // `Bracket.par:2` is the second placement of the file `Bracket.par`
            let file = placement.name.rsplit_once(':').map(|(file, _)| file.to_string()).or(preview.files.get(&placement.site).cloned()).unwrap_or_default();
            let (shapes, children) = (models.get(&placement.site).filter(|s| !s.is_empty()), preview.placements.contains_key(&placement.site));
            if let Some(shapes) = shapes {
                let stem = file.rsplit_once('.').map_or(file.as_str(), |(stem, _)| stem);
                let props = vec![("Component".to_string(), component.clone()), ("Part".to_string(), stem.to_string()), ("File".to_string(), file.clone())];
                for shape in shapes {
                    scene.meshes.push(Mesh { name: placement.name.clone(), transform: placed, props: props.clone(), instance, ..shape.mesh() });
                }
                instance += 1;
                scene.info.components += 1;
            } else if !children {
                scene.info.missing.push(file);
            }
            if children && depth < MAX_DEPTH {
                inner.push((placement.site, placed, component, depth + 1));
            }
        }
        // the stack gives the last first
        todo.extend(inner.into_iter().rev());
    }
    (!scene.meshes.is_empty()).then_some(scene)
}

fn subtract(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// The faces of a model as triangle lists, one per style or colour, in the order they are first
/// met. A face without a style or colour of its own, or with a style that is not in the
/// library, takes the style of the model.
fn shapes(faces: &[Face], styles: &styles::Styles) -> Vec<Shape> {
    let mut out: Vec<Shape> = Vec::new();
    for face in faces {
        let known = |key: &u32| styles.by_key.contains_key(key);
        let paint = match (face.style.filter(known), face.color, styles.model.filter(known)) {
            (Some(key), _, _) | (None, None, Some(key)) => Paint::Style(key),
            (None, Some(color), _) => Paint::Color(color),
            (None, None, None) => Paint::Plain,
        };
        let shape = match out.iter().position(|s| s.paint == paint) {
            Some(k) => &mut out[k],
            None => {
                let (style, color, opacity) = match paint {
                    Paint::Style(key) => (styles.by_key[&key].name.clone(), styles.by_key[&key].color, styles.by_key[&key].opacity),
                    Paint::Color([r, g, b, a]) => (String::new(), [r, g, b].map(|v| f32::from(v) / 255.0), f32::from(a) / 255.0),
                    Paint::Plain => (String::new(), model::DEFAULT_COLOR, 1.0),
                };
                out.push(Shape { paint, style, color, opacity, ..Shape::default() });
                out.last_mut().expect("just added")
            }
        };
        let base = (shape.positions.len() / 3) as u32;
        shape.positions.extend(face.positions.iter().flatten());
        shape.normals.extend(face.normals.iter().flatten());
        let mut first = 0usize;
        for &length in &face.strips {
            let length = length as usize;
            for k in 0..length.saturating_sub(2) {
                // every second triangle of a strip runs the other way round
                let mut corners = if k % 2 == 0 { [first + k, first + k + 1, first + k + 2] } else { [first + k + 1, first + k, first + k + 2] };
                let [a, b, c] = corners.map(|v| face.positions[v]);
                let normal = cross(subtract(b, a), subtract(c, a));
                if normal == [0.0; 3] {
                    continue;
                }
                // the normals of the file say which side is out
                let outward = corners.iter().fold([0.0; 3], |sum, &v| [sum[0] + face.normals[v][0], sum[1] + face.normals[v][1], sum[2] + face.normals[v][2]]);
                if dot(normal, outward) < 0.0 {
                    corners.swap(1, 2);
                }
                shape.indices.extend(corners.map(|v| base + v as u32));
            }
            first += length;
        }
    }
    out.retain(|s| !s.indices.is_empty());
    out
}

#[cfg(test)]
mod tests;
