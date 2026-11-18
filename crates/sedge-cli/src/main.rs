//! `sedge`: what a Solid Edge file holds, and its meshes as OBJ for checking in another viewer.
//!
//! ```text
//! sedge info <file>              streams, the display caches, the placed parts
//! sedge obj <file> <out.obj>     the meshes, placed, as Wavefront OBJ (with <out>.mtl)
//! sedge unpack <file> <folder>   every stream as a file, packed ones inflated
//! ```
//!
//! For an assembly, the files of its parts are looked for next to it.

use sedge_core::{cfb::CompoundFile, pack, Document};
use sedge_read::model::{Kind, Scene};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let arguments: Vec<&str> = arguments.iter().map(String::as_str).collect();
    let result = match arguments[..] {
        ["info", file] => info(Path::new(file)),
        ["obj", file, out] => obj(Path::new(file), Path::new(out)),
        ["unpack", file, folder] => unpack(Path::new(file), Path::new(folder)),
        _ => Err("usage: sedge info <file> | sedge obj <file> <out.obj> | sedge unpack <file> <folder>".to_string()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn bytes(file: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(file).map_err(|e| format!("{}: {e}", file.display()))
}

/// Reads a file; the files an assembly places are taken from its folder, whatever the case of
/// their names.
fn scene(file: &Path) -> Result<Scene, String> {
    let data = bytes(file)?;
    let folder = file.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut fetch = |name: &str| {
        let wanted = name.to_lowercase();
        let found: PathBuf = std::fs::read_dir(if folder.as_os_str().is_empty() { Path::new(".") } else { &folder })
            .ok()?
            .flatten()
            .map(|e| e.path())
            .find(|p| p.file_name().is_some_and(|n| n.to_string_lossy().to_lowercase() == wanted))?;
        std::fs::read(found).ok()
    };
    sedge_read::read_assembly(&data, &mut fetch).map_err(|e| format!("{}: {e}", file.display()))
}

fn info(file: &Path) -> Result<(), String> {
    let data = bytes(file)?;
    let document = Document::open(&data).map_err(|e| format!("{}: {e}", file.display()))?;
    println!("{} ({} bytes)", file.display(), data.len());
    println!("streams:");
    for (path, size) in document.streams() {
        println!("  {path:<56} {size:>10}");
        if !path.starts_with(sedge_read::CACHE) {
            continue;
        }
        // a display cache: its levels and what the finest holds
        let Ok(raw) = document.raw(path) else { continue };
        if let Ok(inner) = CompoundFile::open(&raw) {
            for entry in inner.entries() {
                let inflated = inner.stream(&entry.path).ok().and_then(|s| pack::inflate(&s).ok()).map_or(0, |c| c.len());
                println!("    {:<54} {:>10}  inflates to {inflated}", entry.path, entry.size);
            }
        }
        match sedge_read::cache::read(&raw) {
            Ok(faces) => println!(
                "    finest level: {} faces, {} triangles, {} with a style of their own",
                faces.len(),
                faces.iter().map(|f| f.triangle_count()).sum::<usize>(),
                faces.iter().filter(|f| f.style.is_some()).count()
            ),
            Err(error) => println!("    not read: {error}"),
        }
    }
    if document.has(sedge_read::ATTACHMENTS) {
        let list = sedge_read::attachments::parse(&document.raw(sedge_read::ATTACHMENTS).map_err(|e| e.to_string())?);
        println!("placed files{}:", if list.complete { "" } else { " (list not read to its end)" });
        for reference in &list.references {
            println!("  {}", reference.file);
            for placement in &reference.placements {
                let m = &placement.transform;
                println!("    {}  at ({:.4}, {:.4}, {:.4})", placement.name, m[3], m[7], m[11]);
            }
        }
    }
    match scene(file) {
        Ok(scene) => {
            let kind = if scene.info.kind == Kind::Assembly { "assembly" } else { "part" };
            println!(
                "read as {kind}{}: {} meshes, {} triangles, {} faces, {} placed parts, {} placements without their file, {} styles not followed, {} other caches not drawn",
                if scene.info.preview { " from its preview" } else { "" },
                scene.meshes.len(),
                scene.triangle_count(),
                scene.info.faces,
                scene.info.components,
                scene.info.missing.len(),
                scene.info.unresolved_colors,
                scene.info.other_caches
            );
            let (mut low, mut high) = ([f32::MAX; 3], [f32::MIN; 3]);
            for point in scene.meshes.iter().flat_map(|m| m.world_positions()).collect::<Vec<_>>().chunks_exact(3) {
                for axis in 0..3 {
                    (low[axis], high[axis]) = (low[axis].min(point[axis]), high[axis].max(point[axis]));
                }
            }
            println!("bounds (m): {low:?} to {high:?}");
            let mut styles: Vec<(&str, [f32; 3], f32)> = scene.meshes.iter().map(|m| (m.style.as_str(), m.color, m.opacity)).collect();
            styles.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            styles.dedup();
            println!("styles: {styles:?}");
        }
        Err(message) => println!("not read: {message}"),
    }
    Ok(())
}

fn obj(file: &Path, out: &Path) -> Result<(), String> {
    let scene = scene(file)?;
    let material_file = out.with_extension("mtl");
    let (mut text, mut materials, mut colors) = (String::new(), String::new(), Vec::<[u8; 3]>::new());
    let _ = writeln!(text, "mtllib {}", material_file.file_name().map(|n| n.to_string_lossy()).unwrap_or_default());
    let mut offset = 1;
    for mesh in &scene.meshes {
        let color = mesh.color.map(|v| (v * 255.0).round() as u8);
        let material = colors.iter().position(|c| *c == color).unwrap_or_else(|| {
            let _ = writeln!(materials, "newmtl color{}\nKd {} {} {}", colors.len(), mesh.color[0], mesh.color[1], mesh.color[2]);
            colors.push(color);
            colors.len() - 1
        });
        let _ = writeln!(text, "o {}\nusemtl color{material}", mesh.name.replace(' ', "_"));
        let (positions, normals) = (mesh.world_positions(), mesh.world_normals());
        positions.chunks_exact(3).for_each(|p| {
            let _ = writeln!(text, "v {} {} {}", p[0], p[1], p[2]);
        });
        normals.chunks_exact(3).for_each(|n| {
            let _ = writeln!(text, "vn {} {} {}", n[0], n[1], n[2]);
        });
        for t in mesh.indices.chunks_exact(3) {
            let [a, b, c] = [t[0], t[1], t[2]].map(|v| v as usize + offset);
            let _ = writeln!(text, "f {a}//{a} {b}//{b} {c}//{c}");
        }
        offset += positions.len() / 3;
    }
    std::fs::write(out, text).map_err(|e| format!("{}: {e}", out.display()))?;
    std::fs::write(&material_file, materials).map_err(|e| format!("{}: {e}", material_file.display()))?;
    println!("{}: {} meshes, {} triangles", out.display(), scene.meshes.len(), scene.triangle_count());
    Ok(())
}

fn unpack(file: &Path, folder: &Path) -> Result<(), String> {
    let data = bytes(file)?;
    let document = Document::open(&data).map_err(|e| format!("{}: {e}", file.display()))?;
    for (path, _) in document.streams() {
        // stream names are data: keep them inside the folder
        let name: String = path.chars().map(|c| if c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | ' ') { c } else { '_' }).collect();
        let raw = document.raw(path).map_err(|e| format!("{path}: {e}"))?;
        let content = pack::inflate(&raw).ok().filter(|c| !c.is_empty()).unwrap_or(raw);
        std::fs::create_dir_all(folder).and_then(|()| std::fs::write(folder.join(&name), content)).map_err(|e| format!("{}: {e}", folder.join(&name).display()))?;
    }
    Ok(())
}
