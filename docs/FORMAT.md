# What is known about the format

Everything here was worked out from files (see the clean-room rules in the README). "Seen" means
seen in the sample set: 257 files from nine public sources, 213 parts, 4 sheet metal parts and
40 assemblies, saved by at least six versions of the program. All numbers are little endian.

## Container

A part, sheet metal part or assembly is a compound file ([MS-CFB]): storages and streams. The
streams seen:

| Stream | What |
|---|---|
| `Display/Cache<n>` | A display cache: the tessellation of one body for display; read (below) |
| `Display/Styles/Library0` | The style library, packed; read (below) |
| `Display/Styles/DefaultStyles` | Which styles the model and construction take; read (below) |
| `Display/DFAColors`, `Display/Animation/Info` | Not read |
| `PARASOLID/STREAM<n>.P_B`, `.D_B` | The bodies: Parasolid partition and delta transmit files, binary, not compressed; not read |
| `PARASOLID/SKETCH.P_B` | Sketch geometry, the same way; not read |
| `Attachments` | Assemblies: the placed files and where; read (below) |
| `Preview/...` | Assemblies of newer versions: every part with a tessellation and its place; read (below) |
| `JSite<n>/Ole`, `JSitesList`, `JReverseSitesList`, `IOT/Default`, `JAsmRefAttr` | Assemblies: the links to the placed files; not read |
| `PSMroots`, `PSMclustertable`, `PSMcluster<n>`, `PSMsegmenttable`, `PSMspacemap/...`, `PSMtypetable`, `PSMuserroots` | The document as a store of objects: features, relations, everything else; not read |
| `PartsLiteData` | Named views and variables; not read |
| `BuildVersions`, `DocVersion2`, `Versions`, `GeometricVersionHistory` | Version stamps |
| `SummaryInformation` and other property sets | Properties, and a picture of the model as it was saved; not read |

A file is taken for a Solid Edge document when it has `PSMroots` and one of `PartsLiteData`,
`Attachments`, `Display/...`, `PARASOLID/...`. Every sample has.

A stream may itself hold a compound file (a display cache does). The last sector of such an
inner file may be cut short, in the middle of the mini stream; the streams before the cut are
whole.

## Packed streams

The levels of a display cache and the style libraries are packed:

```text
repeated:  u32 size of the content of the block (at most 0x10000)
           u32 size of the compressed bytes
           the compressed bytes: a zlib stream
then:      u32 0, u32 0
           ED ED ED ED
```

## The display cache

`Display/Cache<n>` is a compound file with these streams:

| Stream | What |
|---|---|
| `Info` | Packed: a tag, a size, the box of the body (6 f64) and more; not read |
| `L0-T1`, `L1-T1`, `L2-T1` | Packed: the faces at a level of detail, 0 the finest. Not every cache has every level |

A part has one cache per displayed body. The cache with the lowest number is taken for the
model: `Cache0` in 184 of the 208 parts with a cache, `Cache1` in 23 (many of them mirrored
copies of another part) and `Cache11` in one. The pictures five of the files carry show that
body. The other caches, where they were looked at, were a construction body (a blank before its
teeth were cut), construction surfaces (numbers from 101) and the flat pattern of a sheet metal
part (`Cache3`, the same faces laid elsewhere). Nothing in the cache says which it is; that is
in the object store.

A level:

```text
u32        tag 04 00 vv 02
per face:
  u32      tag 03 00 vv 02
  u32      size of the face header, its closing byte included: 61, 65 or 69
  u32      0x46
  u32      number of the face
  [u32]    with size 65: the style of the face
  [u32 0, u32]  with size 69: the style of the face
  u32      1
  6 f64    a box, saved empty (1e30 three times, -1e30 three times)
  u32      tag 02 00 ww 02
  u32      size of the mesh: all that follows up to the closing byte of the face
  u32      n: vertices, strip ends included
  u32      strip ends
  4 bytes  flags (52 03 A0 28 and others); not understood, the layout does not depend on them
  n x 3 f64   points, in metres
  n x 3 f32   normals
  n x 2 f32   two numbers between 0 and 1: texture coordinates, it seems; not read
  n x u8      a flag per vertex; not read
  n x u32     a number per vertex (only with ww = 3); not read
  tail        1 byte (ww = 1) or 52 bytes (ww = 3), the last one ED
  ED          closes the face
ED         closes the level
```

`vv` is 1 or 2 and `ww` 1 or 3: an older and a newer layout, alike up to the normals.

The points of a face are triangle strips. A point of three times the largest f64 ends a strip;
its slots in the other arrays hold whatever was in memory. The triangles of a strip alternate
in direction, the first counter-clockwise seen from outside. With that rule, in the three parts
this was counted for, every triangle agrees with its normals and the strips of a solid close it
(every edge used twice); read as fans they do neither. The reader still turns a triangle that
disagrees with its normals.

The style word of a face is one of:

- a key of the style library (top byte `80`): `0x8000002C`;
- a colour, in parts made from imported bodies: `0x4006` in the upper half, then four bits each
  of opacity, blue, green and red: `0x4006F0EE` is yellow. Checked against the pictures in three
  files. (In a preview such a word is also the key of a library record named after it.)

A face without the word takes the style of the model.

## Styles

`Display/Styles/Library0`, packed, is a row of records, each closed by `ED`. A record has no
size, and records differ in length, so they are found by their start:

```text
u32        tag 07 00 vv 02
u32        5
u32        kind: 5 and 10 are face styles, 3 a view style, 6 (in a preview) a colour by number
u32        key, top byte 80
u32        length of the name in UTF-16 units, then the name
16 bytes   00 00 00 00 (flags), f32 1, f32 1, 01 00 FF FF
4 f32      ambient colour, red green blue and 1
4 f32      diffuse colour: the colour that is read
4 f32      specular colour
4 f32      emission
4 f32      one more colour, mostly 0 0 0 1
f32        shininess
f32        transparency: 0 opaque; read
f32        reflectivity
f32        refraction
...        texture names and more; not read
```

The keys are the same in every file for the styles that come with the program (`0x80000027`
"Model Default", `0x80000020` "Construction Default", `0x8000003E` "Steel"); the names are in the
language of the installation.

`Display/Styles/DefaultStyles`: u32 1, the key of the style of the model, the key of the style
of construction.

## Placed files of an assembly

`Attachments`:

```text
16 bytes   B0 9D D8 DE B6 45 CE 11 B3 07 08 00 36 3A 1E 02
u32        version: 5 and 7 seen
u32, u32   files and placements of this assembly
u32, u32   the same, those of its sub-assemblies counted in
12 bytes   zero
per file:
  a file moniker ([MS-OSHARED] 2.3.7.8): the full path of the file when the assembly was saved
  u32 1
  a file moniker: the path from the folder of the assembly (`\Wheel.par`, `sub\Wheel.par`)
  u32      placements of this file
  u32 8, u32 a number (that of its `JSite<n>` storage), u32 flags
  per placement:
    16 f64   the transform, column by column: the translation is the 13th to 15th value
    6 f64    the box of the placed part in the assembly
    u32      size of the rest
    u32      bytes of the name, the name in UTF-16 with a closing zero: `Wheel.par:2`
    ...      attributes (`References` and a number); not read
then       a list of numbers and of names of pattern items; not read
```

The narrow path of a moniker is in the code page of the writer and often the short (8.3) form;
the UTF-16 path, present when the narrow one cannot say it, is the long form. The file name is
taken from the second moniker.

An assembly holds no tessellation of its parts outside its preview. A sub-assembly is a placed
file like a part; its own `Attachments` places its parts. Files are often renamed after an
assembly was saved (a space for a hyphen); the reader asks for the name as written and leaves
finding the file to the caller.

## The preview of an assembly

Assemblies with `Attachments` version 7 mostly have a storage `Preview`. It numbers every file
of the assembly, sub-assemblies and their parts included, as a "site"; the assembly itself is
the first.

`Preview/Structure/Sites`: u32 1, u32 (the count plus one), u32 0, u32 count; per site its
number (u32), u32 0 and the path of its file as a string, in lower case. A string here is a u32
count of UTF-16 units and the units.

`Preview/Structure/IDS`:

```text
u32 1, 3, 2, 1
"Sites", u32 sites, u32 assemblies, u32 parts
per site:      "Site", u32 number, u32 flags, 6 f64 box of the part (empty for an assembly),
               u32, u32 FFFFFFFF
"Atts"
per assembly:  u32 its site, u32 placements
  per placement: "Att", u32 the site that is placed, u32 a number, u32 0,
                 16 f64 the transform (as in `Attachments`), the name (`Wheel.par:2`)
```

`Preview/Display/Scene0/Geometries` is a compound file with, per part, `Cache<site>.1`: a
display cache as above, kept either as a storage (`Cache12.1/Info`, `Cache12.1/L1-T1`) or as a
stream that is a compound file. Its one level is mostly `L1`. `Cache1`, seen twice, is not
understood and not read.

`Preview/Display/Scene0/Libraries` is a compound file with `Library<site>` per part: a packed
style library that holds the two default styles and the styles the part uses. It does not say
which is the style of the model. The reader takes the one style, besides the two defaults, that
no face names, when there is exactly one; else "Model Default". So a part with a style of its
own and faces of two other styles is drawn in the default grey where its faces have no style.

`Preview/Structure/IOT` lists the numbers of the placements; not read.

## Not found

- Which parts of an assembly are hidden or suppressed, and the colours an assembly gives a
  placed part.
- Which display cache is the model, other than by its number.
- The names of bodies, and the relation between the faces of a cache (their numbers) and the
  faces of the Parasolid body.
