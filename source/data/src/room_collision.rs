//! COCT v1 room records, adapted from pinned OpenKH Coct.cs (NOTICE).
//! Raw flags, planes and float bits stay separate from collision response rules.
use serde::Serialize;
use std::io;

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Node {
    pub children: [i16; 8],
    pub bounds: [i16; 6],
    pub mesh_range: [u16; 2],
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct CollisionMesh {
    pub bounds: [i16; 6],
    pub polygon_range: [u16; 2],
    pub visibility: u8,
    pub group: u8,
    pub padding: i16,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Polygon {
    pub ground: u8,
    pub floor_level: u8,
    pub vertices: [i16; 4],
    pub plane: i16,
    pub bounding_box: i16,
    pub attribute: i16,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct RoomCollision {
    pub version: i32,
    pub depth: i32,
    pub kind: i32,
    pub table_spans: [[i32; 2]; 8],
    pub nodes: Vec<Node>,
    pub meshes: Vec<CollisionMesh>,
    pub polygons: Vec<Polygon>,
    pub vertex_bits: Vec<[u32; 4]>,
    pub plane_bits: Vec<[u32; 4]>,
    pub bounding_boxes: Vec<[i16; 6]>,
    pub attributes: Vec<u32>,
}

impl RoomCollision {
    /// Pinned MapStudio polygon display topology, in original coordinates.
    /// Keeps polygon identity; octree membership and response flags are separate.
    pub fn triangle_indices(&self) -> io::Result<Vec<(usize, [u32; 3])>> {
        let mut triangles = Vec::new();
        for (ordinal, polygon) in self.polygons.iter().enumerate() {
            let [a, b, c, d] = polygon.vertices;
            if ![a, b, c]
                .into_iter()
                .all(|v| index(v, self.vertex_bits.len()))
                || (d != -1 && !index(d, self.vertex_bits.len()))
            {
                return Err(invalid("collision display vertex outside declared table"));
            }
            triangles.push((ordinal, [a as u32, b as u32, c as u32]));
            if d >= 0 {
                triangles.push((ordinal, [a as u32, c as u32, d as u32]));
            }
        }
        Ok(triangles)
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn words16<const N: usize>(bytes: &[u8]) -> [i16; N] {
    std::array::from_fn(|i| i16::from_le_bytes(bytes[i * 2..i * 2 + 2].try_into().unwrap()))
}
fn words32<const N: usize>(bytes: &[u8]) -> [u32; N] {
    std::array::from_fn(|i| u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap()))
}
fn range(pair: [u16; 2], count: usize) -> bool {
    pair[0] <= pair[1] && pair[1] as usize <= count
}
fn index(value: i16, count: usize) -> bool {
    value >= 0 && (value as usize) < count
}

pub fn read(bytes: &[u8]) -> io::Result<RoomCollision> {
    if bytes.len() < 80 || bytes.len() > 16 * 1024 * 1024 || &bytes[..4] != b"COCT" {
        return Err(invalid("bounded COCT header required"));
    }
    let header = words32::<20>(bytes);
    if header[1] != 1 {
        return Err(invalid("unqualified COCT version"));
    }
    let table_spans = std::array::from_fn(|i| [header[4 + i * 2] as i32, header[5 + i * 2] as i32]);
    let strides = [1, 32, 20, 16, 16, 16, 12, 4];
    let mut tables = Vec::with_capacity(8);
    for (table_index, (span, stride)) in table_spans.iter().zip(strides).enumerate() {
        let offset = usize::try_from(span[0]).map_err(|_| invalid("negative COCT table offset"))?;
        let size = usize::try_from(span[1]).map_err(|_| invalid("negative COCT table size"))?;
        if size % stride != 0
            || size / stride > 262144
            || (table_index != 0 && size != 0 && offset < 80)
        {
            return Err(invalid("COCT table record boundary/count invalid"));
        }
        let end = offset
            .checked_add(size)
            .ok_or_else(|| invalid("COCT span overflow"))?;
        tables.push(
            bytes
                .get(offset..end)
                .ok_or_else(|| invalid("COCT table outside payload"))?,
        );
    }
    let vertex_bits = tables[4]
        .as_chunks::<16>()
        .0
        .iter()
        .map(|b| words32::<4>(b))
        .collect::<Vec<_>>();
    let plane_bits = tables[5]
        .as_chunks::<16>()
        .0
        .iter()
        .map(|b| words32::<4>(b))
        .collect::<Vec<_>>();
    let bounding_boxes = tables[6]
        .as_chunks::<12>()
        .0
        .iter()
        .map(|b| words16::<6>(b))
        .collect::<Vec<_>>();
    let attributes = tables[7]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| words32::<1>(b)[0])
        .collect::<Vec<_>>();
    let polygons = tables[3]
        .as_chunks::<16>()
        .0
        .iter()
        .map(|b| Polygon {
            ground: b[0],
            floor_level: b[1],
            vertices: words16::<4>(&b[2..]),
            plane: words16::<1>(&b[10..])[0],
            bounding_box: words16::<1>(&b[12..])[0],
            attribute: words16::<1>(&b[14..])[0],
        })
        .collect::<Vec<_>>();
    for p in &polygons {
        if !p.vertices[..3].iter().all(|&v| index(v, vertex_bits.len()))
            || (p.vertices[3] != -1 && !index(p.vertices[3], vertex_bits.len()))
            || !index(p.plane, plane_bits.len())
            || !index(p.attribute, attributes.len())
            || (p.bounding_box >= 0 && !index(p.bounding_box, bounding_boxes.len()))
        {
            return Err(invalid("COCT polygon reference outside declared tables"));
        }
    }
    let meshes = tables[2]
        .as_chunks::<20>()
        .0
        .iter()
        .map(|b| CollisionMesh {
            bounds: words16::<6>(b),
            polygon_range: [
                words16::<1>(&b[12..])[0] as u16,
                words16::<1>(&b[14..])[0] as u16,
            ],
            visibility: b[16],
            group: b[17],
            padding: words16::<1>(&b[18..])[0],
        })
        .collect::<Vec<_>>();
    let nodes = tables[1]
        .as_chunks::<32>()
        .0
        .iter()
        .map(|b| Node {
            children: words16::<8>(b),
            bounds: words16::<6>(&b[16..]),
            mesh_range: [
                words16::<1>(&b[28..])[0] as u16,
                words16::<1>(&b[30..])[0] as u16,
            ],
        })
        .collect::<Vec<_>>();
    if meshes
        .iter()
        .any(|m| !range(m.polygon_range, polygons.len()))
        || nodes.iter().any(|n| {
            !range(n.mesh_range, meshes.len())
                || n.children
                    .iter()
                    .any(|&c| c != -1 && !index(c, nodes.len()))
        })
    {
        return Err(invalid("COCT node/mesh reference outside declared tables"));
    }
    Ok(RoomCollision {
        version: header[1] as i32,
        depth: header[2] as i32,
        kind: header[3] as i32,
        table_spans,
        nodes,
        meshes,
        polygons,
        vertex_bits,
        plane_bits,
        bounding_boxes,
        attributes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Vec<u8> {
        let mut b = vec![0; 80 + 32 + 20 + 16 + 48 + 16 + 12 + 4];
        b[..4].copy_from_slice(b"COCT");
        b[4..8].copy_from_slice(&1u32.to_le_bytes());
        let mut cursor = 80;
        for (i, size) in [0u32, 32, 20, 16, 48, 16, 12, 4].into_iter().enumerate() {
            b[16 + i * 8..20 + i * 8].copy_from_slice(&(cursor as u32).to_le_bytes());
            b[20 + i * 8..24 + i * 8].copy_from_slice(&size.to_le_bytes());
            cursor += size as usize;
        }
        b[80..96].fill(255);
        b[110..112].copy_from_slice(&1u16.to_le_bytes());
        b[126..128].copy_from_slice(&1u16.to_le_bytes());
        for (i, value) in [0i16, 1, 2, -1, 0, -1, 0].into_iter().enumerate() {
            b[134 + i * 2..136 + i * 2].copy_from_slice(&value.to_le_bytes());
        }
        b[148..152].copy_from_slice(&0x80000000u32.to_le_bytes());
        b[224..228].copy_from_slice(&0xfedcba98u32.to_le_bytes());
        b
    }
    #[test]
    fn preserves_triangle_sentinel_float_bits_and_raw_attributes() {
        let r = read(&fixture()).unwrap();
        assert_eq!(r.nodes[0].mesh_range, [0, 1]);
        assert_eq!(r.meshes[0].polygon_range, [0, 1]);
        assert_eq!(r.polygons[0].vertices, [0, 1, 2, -1]);
        assert_eq!(r.polygons[0].bounding_box, -1);
        assert_eq!(r.vertex_bits[0][0], 0x80000000);
        assert_eq!(r.attributes[0], 0xfedcba98);
        assert_eq!(r.triangle_indices().unwrap(), [(0, [0, 1, 2])]);
        let mut quad = r;
        quad.polygons[0].vertices[3] = 1;
        assert_eq!(
            quad.triangle_indices().unwrap(),
            [(0, [0, 1, 2]), (0, [0, 2, 1])]
        );
        quad.polygons[0].vertices[3] = 3;
        assert!(quad.triangle_indices().is_err());
        let mut header_entry = fixture();
        header_entry[16..20].copy_from_slice(&0u32.to_le_bytes());
        header_entry[20..24].copy_from_slice(&80u32.to_le_bytes());
        assert_eq!(read(&header_entry).unwrap().table_spans[0], [0, 80]);
    }
    #[test]
    fn rejects_truncated_spans_versions_and_invalid_graph_references() {
        let good = fixture();
        for len in [0, 4, 79, 147, 227] {
            assert!(read(&good[..len]).is_err());
        }
        for (offset, bytes) in [
            (4, 2u32.to_le_bytes().to_vec()),
            (28, 1u32.to_le_bytes().to_vec()),
            (40, 9999u32.to_le_bytes().to_vec()),
            (134, 3i16.to_le_bytes().to_vec()),
            (80, 1i16.to_le_bytes().to_vec()),
            (126, 2u16.to_le_bytes().to_vec()),
        ] {
            let mut bad = good.clone();
            bad[offset..offset + bytes.len()].copy_from_slice(&bytes);
            assert!(read(&bad).is_err(), "offset {offset}");
        }
    }
}
