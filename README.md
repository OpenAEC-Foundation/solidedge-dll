# solidedge-dll

Native library that reads Solid Edge parts (`.par`), sheet metal parts (`.psm`) and assemblies
(`.asm`) without Solid Edge, for use by
[open-bim-suite](https://github.com/OpenAEC-Foundation/open-bim-suite) and other tools.

**Status:** pre-alpha. Parts and sheet metal parts open as coloured triangle meshes,
read from the display cache Solid Edge saves with the model; assemblies place the files of their
parts, or are drawn from the preview they hold. Of the 257 files of the sample set 247 open: all
208 parts and sheet metal parts that hold a model (9 more hold sketches only) and 39 of the 40
assemblies (1 places no parts).

## Scope

- Input: `.par`, `.psm` and `.asm` files. They are compound files (the "OLE2" container); a file
  with one of these extensions that is not a Solid Edge document (other programs use `.par` and
  `.asm` too) is refused with a message that says so: `sedge_read::is_solid_edge(&[u8])`.
- Output: triangle meshes, each with a name, a colour, an opacity, the name of its style, its
  place in the assembly and properties (component path, part, file), exposed through Rust and a
  command line tool. Lengths are metres, Z is up.
- `sedge_read::read(&[u8])` works on bytes, so the reader also runs in the browser (wasm). An
  assembly asks the caller for the files of its parts and sub-assemblies, by file name:
  `sedge_read::read_assembly(&[u8], fetch)`.

### What is read

- The display cache of the model (`Display/Cache<n>`, the one with the lowest number): the
  triangle strips, with normals, of every face, in the finest level of detail the file holds.
- The colour of each face: its own style or colour, or the style of the part. Of a style the
  diffuse colour, the transparency and the name.
- In an assembly (`Attachments`): every placed part and sub-assembly with its place, the path
  of the component through the sub-assemblies, and the name and file of its part. A file placed
  many times is read once.
- The preview of an assembly (`Preview`, written by newer versions): the same parts and places
  with a coarser tessellation of every part, held by the assembly itself. It is drawn when
  files of parts are not given and it shows more of the assembly than the files that were.

### What is not read

- The exact geometry (the Parasolid bodies in `PARASOLID/*.P_B`), so no curved surfaces finer
  than Solid Edge tessellated them, no edges, no measurements on exact faces. A part saved
  without a display cache is not drawn; none was seen that holds a body.
- The other display caches of a part: construction bodies and surfaces, the flat pattern of a
  sheet metal part. Which cache is the model is taken from its number (the lowest), not from
  the document; the pictures that five of the files carry agree with it.
- Hidden and suppressed parts of an assembly are not told apart: what the assembly places is
  drawn. Colours an assembly gives to a placed part are not read: a part has its own colours.
- An assembly of an older version (no preview) whose part files are not given: nothing to draw.
  Parts whose file is not given are listed, not drawn.
- In a preview the style of a part is guessed where the part has faces of several styles (see
  [docs/FORMAT.md](docs/FORMAT.md)).
- Textures and the other properties of a style (shininess, reflection): only colour and
  transparency.
- Family-of-parts members and other configurations than the one saved; the feature tree,
  sketches, dimensions, relations, properties, drafts (`.dft`) and weldments (`.pwd`).

## Crates

| Crate | Role |
|---|---|
| `sedge-core` | Container layer: the compound file, its streams, the packed streams |
| `sedge-read` | Display cache, styles, placed parts and preview to meshes: `read`, `read_assembly`, the `model` |
| `sedge-cli` | `sedge` command: `info` (what a file holds), `obj` (its meshes as OBJ), `unpack` (its streams) |

See [docs/FORMAT.md](docs/FORMAT.md) for what is known about the format.

```text
cargo run -p sedge-cli -- info Part.par
cargo run -p sedge-cli -- obj Assembly.asm out.obj
```

## Clean-room rules

The reader is built only from black-box analysis of files.

- Never open, disassemble, or decompile Solid Edge, its add-ins or other third-party binaries;
  no memory dumps.
- Never consult third-party parsers of these files, SDK headers that describe the file format,
  or leaked or licensed format documentation.
- Contributors who have seen such material do not write reader code.
- Public documents that are not about the Solid Edge format are used and named where they are:
  Microsoft's description of the compound file ([MS-CFB]) and of the file moniker
  ([MS-OSHARED]), and the zlib format (RFC 1950).

## Test data

Never commit `.par`, `.psm`, `.asm`, `.pwd` or `.dft` files, or anything derived from them
(exports, unpacked streams, screenshots). Sample files live outside the repository; `.gitignore`
refuses the file types. Unit tests write their own bytes (`sedge_read::testing` builds small
parts and assemblies). The corpus test runs over a folder of samples when `SEDGE_SAMPLES` names
it, and says how many of the files open:

```text
SEDGE_SAMPLES=C:\sedge-samples cargo test -p sedge-read --test corpus -- --nocapture
```

## Legal

Solid Edge and Parasolid are trademarks or registered trademarks of Siemens Industry Software
Inc. This project is not affiliated with or endorsed by it. Before any public release: legal
review and a patent freedom-to-operate check.
