use super::*;
use crate::model::IDENTITY;
use crate::testing::{assembly, assembly_with_preview, moved, part, quad, Site};

const STEEL: u32 = 0x8000_003E;
const RED: u32 = 0x8000_002C;
const GLASS: u32 = 0x8000_0078;
const STYLES: [(u32, &str, [f32; 3], f32); 3] = [(STEEL, "Steel", [0.33, 0.34, 0.35], 1.0), (RED, "Red", [0.8, 0.0, 0.0], 1.0), (GLASS, "Glass", [0.44, 0.6, 0.74], 0.5)];

/// A steel part of these faces.
fn part_of(faces: &[cache::Face]) -> Vec<u8> {
    part(faces, &STYLES, STEEL, &[])
}

/// A part of three faces: two in the style of the model, one red.
fn plate() -> Vec<u8> {
    part(&[quad(1, [0.0, 0.0, 0.0], None), quad(2, [0.0, 0.0, 1.0], Some(RED)), quad(3, [2.0, 0.0, 0.0], None)], &STYLES, STEEL, &[])
}

#[test]
fn a_part_gives_a_mesh_per_style() {
    let scene = read(&plate()).unwrap();
    assert_eq!((scene.info.kind, scene.info.components, scene.info.faces, scene.info.unresolved_colors, scene.info.other_caches), (Kind::Part, 0, 3, 0, 0));
    assert!(scene.info.complete && scene.info.missing.is_empty());
    assert_eq!(scene.meshes.len(), 2);
    let (steel, red) = (&scene.meshes[0], &scene.meshes[1]);
    assert_eq!((steel.name.as_str(), steel.style.as_str(), steel.color, steel.opacity, steel.triangle_count()), ("Model", "Steel", [0.33, 0.34, 0.35], 1.0, 4));
    assert_eq!((red.style.as_str(), red.color, red.triangle_count(), red.instance), ("Red", [0.8, 0.0, 0.0], 2, 0));
    assert_eq!(steel.transform, IDENTITY);
    // the finest level is read: the coarser one of the builder holds one face
    assert_eq!(steel.positions.len(), 8 * 3);
    // counter-clockwise seen from the side the normals point to
    for mesh in &scene.meshes {
        for t in mesh.indices.chunks_exact(3) {
            let [a, b, c] = [t[0], t[1], t[2]].map(|v| [0, 1, 2].map(|k| mesh.positions[v as usize * 3 + k]));
            assert!(cross(subtract(b, a), subtract(c, a))[2] > 0.0);
        }
        assert!(mesh.normals.chunks_exact(3).all(|n| n == [0.0, 0.0, 1.0]));
    }
}

#[test]
fn a_transparent_style_keeps_its_opacity() {
    let scene = read(&part(&[quad(1, [0.0; 3], None)], &STYLES, GLASS, &[])).unwrap();
    assert_eq!((scene.meshes[0].style.as_str(), scene.meshes[0].opacity), ("Glass", 0.5));
}

#[test]
fn a_style_that_is_not_found_falls_back_to_that_of_the_model() {
    let scene = read(&part(&[quad(1, [0.0; 3], Some(0x8000_0FFF)), quad(2, [0.0, 0.0, 1.0], None)], &STYLES, STEEL, &[])).unwrap();
    assert_eq!((scene.meshes.len(), scene.meshes[0].style.as_str(), scene.info.unresolved_colors), (1, "Steel", 1));
    // no style library at all: the default colour
    let scene = read(&part(&[quad(1, [0.0; 3], None)], &[], STEEL, &[])).unwrap();
    assert_eq!((scene.meshes[0].color, scene.meshes[0].style.as_str()), (model::DEFAULT_COLOR, ""));
}

#[test]
fn a_face_with_a_colour_of_its_own_is_a_mesh_of_that_colour() {
    let mut yellow = quad(2, [0.0, 0.0, 1.0], None);
    yellow.color = Some([238, 238, 0, 255]);
    let scene = read(&part_of(&[quad(1, [0.0; 3], None), yellow])).unwrap();
    assert_eq!(scene.meshes.len(), 2);
    assert_eq!((scene.meshes[1].color, scene.meshes[1].opacity, scene.meshes[1].style.as_str()), ([238.0 / 255.0, 238.0 / 255.0, 0.0], 1.0, ""));
    assert_eq!(scene.info.unresolved_colors, 0);
}

#[test]
fn only_the_model_is_drawn_not_the_construction_bodies() {
    let scene = read(&part(&[quad(1, [0.0; 3], None)], &STYLES, STEEL, &["Display/Cache2", "Display/Cache101"])).unwrap();
    assert_eq!((scene.triangle_count(), scene.info.other_caches), (2, 2));
    assert!(scene.meshes.iter().flat_map(|m| &m.positions).all(|&v| v < 10.0));
    // a part saved without cache 0 (a mirrored copy): the lowest number is its model
    let scene = read(&part(&[], &STYLES, STEEL, &["Display/Cache11", "Display/Cache2"])).unwrap();
    assert_eq!((scene.triangle_count(), scene.info.other_caches), (2, 1));
    // a part without any cache has no model
    let error = read(&part(&[], &STYLES, STEEL, &[])).unwrap_err();
    assert!(matches!(error, Error::NoGeometry(_)), "{error}");
    assert!(error.to_string().contains("sketches only"), "{error}");
}

#[test]
fn an_assembly_places_the_files_it_names() {
    let wheel = part(&[quad(1, [0.0; 3], None)], &STYLES, RED, &[]);
    let axle = assembly(&[("C:\\Work\\Wheel.par", "Wheel.par", &[("Wheel.par:1", moved(0.0, 5.0, 0.0)), ("Wheel.par:2", moved(0.0, -5.0, 0.0))])]);
    let turned = [0.0, -1.0, 0.0, 100.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
    let car = assembly(&[
        ("C:\\Work\\PLATE~1.PAR", "Plate one.par", &[("Plate one.par:1", IDENTITY)]),
        ("C:\\Work\\Axle.asm", "Axle.asm", &[("Axle.asm:1", moved(10.0, 0.0, 0.0)), ("Axle.asm:2", turned)]),
        ("C:\\Work\\Lost.par", "..\\other\\Lost.par", &[("Lost.par:1", IDENTITY), ("Lost.par:2", IDENTITY)]),
    ]);
    let mut asked = Vec::new();
    let mut fetch = |name: &str| {
        asked.push(name.to_string());
        match name {
            "Plate one.par" => Some(plate()),
            "Wheel.par" => Some(wheel.clone()),
            "Axle.asm" => Some(axle.clone()),
            _ => None,
        }
    };
    let scene = read_assembly(&car, &mut fetch).unwrap();
    assert_eq!(asked, ["Plate one.par", "Axle.asm", "Wheel.par", "Lost.par"]);
    assert_eq!((scene.info.kind, scene.info.components, scene.info.faces), (Kind::Assembly, 5, 4));
    assert_eq!(scene.info.missing, ["Lost.par", "Lost.par"]);
    assert!(scene.info.complete);
    let names: Vec<(&str, u32, &str)> = scene.meshes.iter().map(|m| (m.name.as_str(), m.instance, m.style.as_str())).collect();
    assert_eq!(names, [("Plate one.par:1", 0, "Steel"), ("Plate one.par:1", 0, "Red"), ("Wheel.par:1", 1, "Red"), ("Wheel.par:2", 2, "Red"), ("Wheel.par:1", 3, "Red"), ("Wheel.par:2", 4, "Red")]);
    assert_eq!(scene.meshes[0].props, [("Component".to_string(), "Plate one.par:1".to_string()), ("Part".to_string(), "Plate one".to_string()), ("File".to_string(), "Plate one.par".to_string())]);
    assert_eq!(scene.meshes[3].props[0].1, "Axle.asm:1/Wheel.par:2");
    // the place of a wheel is that of its axle, then its own
    assert_eq!(&scene.meshes[2].world_positions()[..3], [10.0, 5.0, 0.0]);
    assert_eq!(&scene.meshes[3].world_positions()[..3], [10.0, -5.0, 0.0]);
    // the second axle is turned a quarter about z and moved: (0, 5, 0) goes to (-5, 0, 0) + 100
    assert_eq!(&scene.meshes[4].world_positions()[..3], [95.0, 0.0, 0.0]);
    assert_eq!(&scene.meshes[4].world_normals()[..3], [0.0, 0.0, 1.0]);
    assert_eq!(&scene.meshes[4].world_positions()[3..6], [95.0, 1.0, 0.0]);
}

#[test]
fn an_assembly_without_its_parts_says_so() {
    let car = assembly(&[("C:\\Work\\Wheel.par", "Wheel.par", &[("Wheel.par:1", IDENTITY)])]);
    let error = read(&car).unwrap_err();
    assert_eq!(error, Error::NoGeometry("none of the parts of the assembly was found"));
    assert_eq!(read(&assembly(&[])).unwrap_err(), Error::NoGeometry("the assembly places no parts"));
    // a file that is given but is no Solid Edge file counts as missing
    let scene = read_assembly(&assembly(&[("C:\\a\\Wheel.par", "Wheel.par", &[("Wheel.par:1", IDENTITY)]), ("C:\\a\\Plate.par", "Plate.par", &[("Plate.par:1", IDENTITY)])]), &mut |name| {
        if name == "Plate.par" {
            Some(plate())
        } else {
            Some(b"not a part".to_vec())
        }
    })
    .unwrap();
    assert_eq!((scene.info.missing.as_slice(), scene.info.components), (["Wheel.par".to_string()].as_slice(), 1));
}

/// A car of a plate and an axle of two wheels, with a preview that holds them all.
fn car_with_preview() -> Vec<u8> {
    let (plate, wheel) = ([quad(1, [0.0; 3], None), quad(2, [0.0, 0.0, 1.0], Some(RED))], [quad(1, [0.0; 3], None)]);
    // the library of a part in a preview: the two defaults and the styles the part uses
    let defaults = [(0x8000_0020, "Construction Default", [0.5, 0.6, 0.7], 1.0), (0x8000_0027, "Model Default", [0.51, 0.53, 0.55], 1.0)];
    let of_plate = [defaults[0], defaults[1], STYLES[0], STYLES[1]];
    let sites = [
        Site { number: 1, path: "C:\\Work\\Car.asm", faces: &[], styles: &[], places: &[(3, "Plate one.par:1", IDENTITY), (2, "Axle.asm:1", moved(10.0, 0.0, 0.0))], storage: false },
        Site { number: 2, path: "C:\\Work\\Axle.asm", faces: &[], styles: &[], places: &[(4, "Wheel.par:1", moved(0.0, 5.0, 0.0)), (4, "Wheel.par:2", moved(0.0, -5.0, 0.0))], storage: false },
        Site { number: 3, path: "C:\\Work\\Plate one.par", faces: &plate, styles: &of_plate, places: &[], storage: true },
        Site { number: 4, path: "C:\\Work\\Wheel.par", faces: &wheel, styles: &defaults, places: &[], storage: false },
    ];
    assembly_with_preview(&[("C:\\Work\\Plate one.par", "Plate one.par", &[("Plate one.par:1", IDENTITY)]), ("C:\\Work\\Axle.asm", "Axle.asm", &[("Axle.asm:1", moved(10.0, 0.0, 0.0))])], &sites)
}

#[test]
fn an_assembly_without_its_files_is_drawn_from_its_preview() {
    let scene = read(&car_with_preview()).unwrap();
    assert!(scene.info.preview && scene.info.complete);
    assert_eq!((scene.info.kind, scene.info.components, scene.info.faces, scene.info.missing.len()), (Kind::Assembly, 3, 3, 0));
    let names: Vec<(&str, u32, &str)> = scene.meshes.iter().map(|m| (m.name.as_str(), m.instance, m.style.as_str())).collect();
    // the style of the plate is the one of its library that no face names
    assert_eq!(names, [("Plate one.par:1", 0, "Steel"), ("Plate one.par:1", 0, "Red"), ("Wheel.par:1", 1, "Model Default"), ("Wheel.par:2", 2, "Model Default")]);
    assert_eq!(scene.meshes[3].props, [("Component".to_string(), "Axle.asm:1/Wheel.par:2".to_string()), ("Part".to_string(), "Wheel".to_string()), ("File".to_string(), "Wheel.par".to_string())]);
    assert_eq!(&scene.meshes[3].world_positions()[..3], [10.0, -5.0, 0.0]);
    assert_eq!(scene.meshes[0].triangle_count(), 2);
}

#[test]
fn the_files_of_the_parts_go_before_the_preview() {
    // every file given: the parts are drawn from their own files, in their finest level
    let wheel = part(&[quad(1, [0.0; 3], None), quad(2, [3.0, 0.0, 0.0], None)], &STYLES, RED, &[]);
    let axle = assembly(&[("C:\\Work\\Wheel.par", "Wheel.par", &[("Wheel.par:1", moved(0.0, 5.0, 0.0)), ("Wheel.par:2", moved(0.0, -5.0, 0.0))])]);
    let mut all = |name: &str| match name {
        "Plate one.par" => Some(plate()),
        "Wheel.par" => Some(wheel.clone()),
        "Axle.asm" => Some(axle.clone()),
        _ => None,
    };
    let scene = read_assembly(&car_with_preview(), &mut all).unwrap();
    assert!(!scene.info.preview);
    assert_eq!((scene.info.components, scene.meshes[2].style.as_str(), scene.meshes[2].triangle_count()), (3, "Red", 4));
    // one file not given: the preview draws more of the assembly
    let scene = read_assembly(&car_with_preview(), &mut |name| if name == "Plate one.par" { Some(plate()) } else { None }).unwrap();
    assert!(scene.info.preview);
    assert_eq!(scene.info.components, 3);
}

#[test]
fn an_assembly_that_places_itself_ends() {
    let knot = assembly(&[("C:\\a\\Knot.asm", "Knot.asm", &[("Knot.asm:1", moved(1.0, 0.0, 0.0))]), ("C:\\a\\Plate.par", "Plate.par", &[("Plate.par:1", IDENTITY)])]);
    let scene = read_assembly(&knot, &mut |name| if name == "Knot.asm" { Some(knot.clone()) } else { Some(plate()) }).unwrap();
    // the plate of the assembly, and that of the one copy of itself that is followed
    assert_eq!(scene.info.components, 2);
    assert_eq!(scene.meshes.iter().map(|m| m.props[0].1.as_str()).collect::<Vec<_>>(), ["Knot.asm:1/Plate.par:1", "Knot.asm:1/Plate.par:1", "Plate.par:1", "Plate.par:1"]);
}

#[test]
fn files_of_other_programs_are_refused() {
    // other programs use the same extensions: assembler source, parameter files, archives
    for other in [&b"; boot.asm\n        mov ax, 0x07C0\n"[..], b"ISO-10303-21;", b"PK\x03\x04", b""] {
        assert!(!is_solid_edge(other));
        assert_eq!(read(other), Err(Error::NotACompoundFile));
    }
    let word = sedge_core::testing::compound_file(&[("WordDocument", b"text"), ("1Table", b"x")]);
    assert!(!is_solid_edge(&word));
    assert_eq!(read(&word), Err(Error::NotSolidEdge));
    assert!(read(&word).unwrap_err().to_string().starts_with("not a Solid Edge file"));
    assert!(is_solid_edge(&plate()) && is_solid_edge(&assembly(&[])));
}

#[test]
fn a_file_cut_short_is_an_error_not_a_panic() {
    let file = plate();
    for cut in (0..file.len()).step_by(97) {
        let _ = read(&file[..cut]);
    }
    let mut noise = file.clone();
    for k in (600..noise.len()).step_by(41) {
        noise[k] ^= 0x5A;
    }
    let _ = read(&noise);
}
