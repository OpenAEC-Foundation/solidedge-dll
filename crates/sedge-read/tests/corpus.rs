//! Runs over the sample set: every `.par`, `.psm` and `.asm` under `SEDGE_SAMPLES`, and says
//! how many of them open. Sample files are kept out of the repository (see the README);
//! without `SEDGE_SAMPLES` the test passes without doing anything.
//!
//! ```text
//! SEDGE_SAMPLES=C:\sedge-samples cargo test -p sedge-read --test corpus -- --nocapture
//! ```
//!
//! A file "opens" when it gives triangles. A part that holds no model (sketches only) and an
//! assembly that has neither a preview nor any of its parts in the sample set cannot open; they
//! are counted apart and do not fail the test. Any other error does. The assemblies are also
//! read on their own, as a browser gets them: that works for those that hold a preview.

use sedge_read::Error;
use std::path::{Path, PathBuf};

fn collect(folder: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(folder) else { return };
    for path in entries.flatten().map(|e| e.path()) {
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|e| ["par", "psm", "asm"].iter().any(|x| e.eq_ignore_ascii_case(x))) {
            out.push(path);
        }
    }
}

/// A name without what people change when they rename files: case, spaces and punctuation.
fn loose(name: &str) -> String {
    name.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

#[test]
fn the_samples_open() {
    let Some(folder) = std::env::var_os("SEDGE_SAMPLES") else { return };
    let mut files = Vec::new();
    collect(Path::new(&folder), &mut files);
    files.sort();
    assert!(!files.is_empty(), "no .par, .psm or .asm under SEDGE_SAMPLES");
    let (mut opened, mut empty, mut without_parts, mut failed) = (0, 0, 0, 0);
    let (mut triangles, mut faces, mut unresolved, mut missing, mut placed, mut incomplete) = (0, 0, 0, 0, 0, 0);
    let (mut previews, mut assemblies, mut alone) = (0, 0, 0);
    for file in &files {
        let name = file.file_name().unwrap_or_default().to_string_lossy();
        let data = std::fs::read(file).expect("a sample file");
        // the files an assembly places: from its own folder, else from the folders of the same
        // source, by exact name, else by a name that differs in punctuation only
        let source = file.strip_prefix(&folder).ok().and_then(|p| p.components().next()).map(|c| Path::new(&folder).join(c)).unwrap_or_default();
        let mut fetch = |part: &str| {
            let (wanted, nearly) = (part.to_lowercase(), loose(part));
            let named = |f: &&PathBuf, exact: bool| f.file_name().is_some_and(|n| if exact { n.to_string_lossy().to_lowercase() == wanted } else { loose(&n.to_string_lossy()) == nearly });
            let found = files
                .iter()
                .find(|f| f.parent() == file.parent() && named(f, true))
                .or_else(|| files.iter().find(|f| f.starts_with(&source) && named(f, true)))
                .or_else(|| files.iter().find(|f| f.starts_with(&source) && named(f, false)))?;
            std::fs::read(found).ok()
        };
        if file.extension().is_some_and(|e| e.eq_ignore_ascii_case("asm")) {
            assemblies += 1;
            alone += usize::from(sedge_read::read(&data).is_ok());
        }
        match sedge_read::read_assembly(&data, &mut fetch) {
            Ok(scene) => {
                opened += 1;
                previews += usize::from(scene.info.preview);
                triangles += scene.triangle_count();
                faces += scene.info.faces;
                unresolved += scene.info.unresolved_colors;
                missing += scene.info.missing.len();
                placed += scene.info.components;
                incomplete += usize::from(!scene.info.complete);
                println!(
                    "ok    {name}: {} meshes, {} triangles, {} placed parts, {} missing, {} styles not followed{}{}",
                    scene.meshes.len(),
                    scene.triangle_count(),
                    scene.info.components,
                    scene.info.missing.len(),
                    scene.info.unresolved_colors,
                    if scene.info.preview { ", from its preview" } else { "" },
                    if scene.info.complete { "" } else { ", list of parts not read to its end" }
                );
                for mesh in &scene.meshes {
                    assert_eq!(mesh.positions.len(), mesh.normals.len(), "{name}");
                    assert!(mesh.indices.iter().all(|&i| (i as usize) < mesh.positions.len() / 3), "{name}");
                    assert!(mesh.positions.iter().chain(&mesh.transform.map(|v| v as f32)).all(|v| v.is_finite()), "{name}");
                    assert!(mesh.color.iter().all(|v| (0.0..=1.0).contains(v)) && (0.0..=1.0).contains(&mesh.opacity), "{name}");
                }
            }
            Err(Error::NoGeometry(what)) if what.contains("sketches only") => {
                empty += 1;
                println!("empty {name}: {what}");
            }
            Err(Error::NoGeometry(what)) => {
                without_parts += 1;
                println!("alone {name}: {what}");
            }
            Err(error) => {
                failed += 1;
                println!("FAIL  {name}: {error}");
            }
        }
    }
    println!("{opened} of {} files open; {empty} parts hold no model; {without_parts} assemblies have no preview and none of their parts in the sample set; {failed} fail", files.len());
    println!("{previews} assemblies drawn from their preview for want of files; {alone} of {assemblies} assemblies open on their own");
    println!("{triangles} triangles; {faces} faces, {unresolved} of them with a style that could not be followed; {placed} placed parts drawn, {missing} placements without their file; {incomplete} lists of parts not read to their end");
    assert_eq!(failed, 0, "not every sample is read");
}
