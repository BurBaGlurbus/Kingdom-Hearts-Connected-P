//! Static action/animation candidates from installed BD scripts. This does not
//! execute traps, choose a branch, or implement an enemy's combat behavior.
use crate::{
    bar,
    bdscript::{Instruction, Program},
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
};

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct MotionCandidate {
    pub instruction: u32,
    pub trap: u16,
    /// Logical request; a variant MSET does not use this as a raw BAR ordinal.
    pub logical_motion: u32,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Action {
    pub name: Vec<u8>,
    pub registration: u32,
    pub callback: u32,
    pub flags: u32,
    pub motions: Vec<MotionCandidate>,
    pub unresolved_motion_requests: usize,
    pub callback_error: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Inspection {
    pub actions: Vec<Action>,
    pub unresolved_registrations: usize,
}

fn invalid(s: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, s)
}
fn literal(i: &Instruction) -> Option<u32> {
    (i.opcode & 0x3f == 0).then(|| i.operands[0] | i.operands[1] << 16)
}

fn graph(program: &Program<'_>, root: u32) -> io::Result<Vec<Instruction>> {
    let mut pending = vec![root];
    let mut found = BTreeMap::new();
    let mut occupied = BTreeMap::new();
    while let Some(address) = pending.pop() {
        if found.contains_key(&address) {
            continue;
        }
        if found.len() >= 65_536 {
            return Err(invalid("action graph exceeds instruction bound"));
        }
        let i = program.instruction(address)?;
        for word in address..address + i.words {
            if occupied.insert(word, address).is_some() {
                return Err(invalid("action branch overlaps operand"));
            }
        }
        if let Some(target) = i.target {
            pending.push(target);
        }
        if !matches!(i.opcode & 0xffcf, 7 | 0x49 | 0x89) {
            pending.push(address + i.words);
        }
        found.insert(address, i);
    }
    Ok(found.into_values().collect())
}

/// Recover only an unchanged literal stack cell in one straight-line region.
/// Native motion traps take object, motion, blend (frame-start also takes a
/// frame before blend). Dynamic expressions above the motion cell do not make
/// that cell dynamic. Calls, unknown effects and incoming branches stop the
/// slice; no helper execution, control-flow choice or runtime patch is inferred.
fn motion_argument(
    code: &[Instruction],
    at: usize,
    joins: &BTreeSet<u32>,
    above: usize,
) -> Option<u32> {
    let mut next = code[at].address;
    if joins.contains(&next) {
        return None;
    }
    let mut skip = above;
    for i in code[..at].iter().rev().take(64) {
        if i.address + i.words != next {
            return None;
        }
        match i.opcode & 15 {
            0 => {
                if skip == 0 {
                    return literal(i);
                }
                skip -= 1;
            }
            // Store pops a cell; indirect read and unary operations replace
            // the top cell; binary operations consume two and produce one.
            1 => skip = skip.checked_add(1)?,
            3 | 5 => {
                if skip == 0 {
                    return None;
                }
            }
            6 => {
                if skip == 0 {
                    return None;
                }
                skip = skip.checked_add(1)?;
            }
            _ => return None,
        }
        if joins.contains(&i.address) {
            return None;
        }
        next = i.address;
    }
    None
}

/// Accept only the literal compiler pattern: twelve contiguous pushes, then
/// table 1/trap 6. Nonliteral/different registrations stay in the denominator.
/// Motion requests are candidates on reachable direct paths; conditional paths
/// and called helpers may contain several choices and are not selected here.
pub fn inspect(bytes: &[u8]) -> io::Result<Inspection> {
    let program = Program::read(bytes)?;
    let instructions = program.direct_graph()?;
    let mut result = Inspection {
        actions: Vec::new(),
        unresolved_registrations: 0,
    };
    for (index, trap) in instructions.iter().enumerate() {
        if trap.trap() != Some((1, 6)) {
            continue;
        }
        let registration = (|| {
            let window = instructions.get(index.checked_sub(12)?..index)?;
            if !window
                .windows(2)
                .all(|w| w[0].address + w[0].words == w[1].address)
                || window[11].address + window[11].words != trap.address
                || window[0].opcode != 96
                || window[1].opcode != 224
                || !window[2..].iter().all(|i| literal(i).is_some())
            {
                return None;
            }
            let start = 16usize.checked_add(window[1].operands[0] as usize * 2)?;
            let tail = bytes.get(start..)?;
            let length = tail.iter().take(129).position(|&b| b == 0)?;
            if length == 0 || length > 128 {
                return None;
            }
            Some((
                tail[..length].to_vec(),
                literal(&window[2])?,
                literal(&window[3])?,
            ))
        })();
        let Some((name, flags, callback)) = registration else {
            result.unresolved_registrations += 1;
            continue;
        };
        let mut action = Action {
            name,
            registration: trap.address,
            callback,
            flags,
            motions: Vec::new(),
            unresolved_motion_requests: 0,
            callback_error: None,
        };
        match graph(&program, callback) {
            Ok(code) => {
                let mut joins: BTreeSet<_> = code.iter().filter_map(|i| i.target).collect();
                joins.insert(callback);
                for (at, i) in code.iter().enumerate() {
                    let Some((1, trap_id @ (11 | 12 | 13 | 61))) = i.trap() else {
                        continue;
                    };
                    let above = if trap_id == 61 { 2 } else { 1 };
                    let candidate = motion_argument(&code, at, &joins, above);
                    if let Some(logical_motion) = candidate {
                        action.motions.push(MotionCandidate {
                            instruction: i.address,
                            trap: trap_id,
                            logical_motion,
                        });
                    } else {
                        action.unresolved_motion_requests += 1;
                    }
                }
            }
            Err(error) => action.callback_error = Some(error.to_string()),
        }
        result.actions.push(action);
    }
    Ok(result)
}

/// Native MSET admission (T122): the BAR header word at +0xc, bit 0, enables
/// logical*4 plus context-dependent variants. No variant order is guessed.
pub fn uses_motion_variants(bytes: &[u8]) -> io::Result<bool> {
    Ok(bar::read_bar(bytes)?.header_field_12 & 1 != 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(dynamic: bool) -> Vec<u8> {
        let mut body = vec![48, 0];
        if dynamic {
            body.extend([48, 4]);
        } else {
            body.extend([192, 7, 0]);
        }
        body.extend([208, 0, 0, 74, 11, 137]);
        fixture_body(&body)
    }
    fn fixture_body(body: &[u16]) -> Vec<u8> {
        let mut code = vec![96, 0, 224, 0, 192, 0, 0, 192, 0, 0];
        for _ in 0..8 {
            code.extend([192, 0, 0]);
        }
        code.extend([74, 6, 137]);
        let callback = 14 + code.len() as u16;
        code[8] = callback;
        code.extend(body);
        let string_at = 14 + code.len() as u16;
        code[3] = string_at;
        let mut bytes = vec![0; 44];
        bytes[..4].copy_from_slice(b"test");
        bytes[28..32].copy_from_slice(&1u32.to_le_bytes());
        bytes[32..36].copy_from_slice(&14u32.to_le_bytes());
        for word in code {
            bytes.extend(word.to_le_bytes());
        }
        bytes.extend(b"example\0");
        bytes
    }
    #[test]
    fn literal_registration_and_callback_motion() {
        let decoded = inspect(&fixture(false)).unwrap();
        assert_eq!(decoded.unresolved_registrations, 0);
        assert_eq!(decoded.actions.len(), 1);
        assert_eq!(decoded.actions[0].name, b"example");
        assert_eq!(decoded.actions[0].motions[0].logical_motion, 7);
        assert_eq!(decoded.actions[0].unresolved_motion_requests, 0);
    }
    #[test]
    fn dynamic_motion_remains_unknown() {
        let decoded = inspect(&fixture(true)).unwrap();
        assert!(decoded.actions[0].motions.is_empty());
        assert_eq!(decoded.actions[0].unresolved_motion_requests, 1);
    }
    #[test]
    fn literal_motion_survives_dynamic_blend_frame_and_blend_arithmetic() {
        for body in [
            // A blend loaded through an actor pointer.
            vec![48, 0, 192, 7, 0, 48, 0, 3, 36, 74, 12, 137],
            // Dynamic blend multiplied by a literal float.
            vec![48, 0, 192, 7, 0, 48, 4, 208, 0, 0, 150, 74, 11, 137],
            // Frame-start takes both frame and blend above the motion.
            vec![48, 0, 192, 7, 0, 48, 4, 48, 8, 74, 61, 137],
        ] {
            let inspected = inspect(&fixture_body(&body)).unwrap();
            assert_eq!(inspected.actions[0].motions[0].logical_motion, 7);
            assert_eq!(inspected.actions[0].unresolved_motion_requests, 0);
        }
    }
    #[test]
    fn incoming_branches_and_computed_motion_values_remain_unknown() {
        for body in [
            // Two different motion literals converge at the blend load.
            vec![
                48, 0, 48, 4, 71, 5, 192, 7, 0, 7, 3, 192, 8, 0, 48, 8, 74, 11, 137,
            ],
            // The motion itself is a computed value, not an unchanged literal.
            vec![48, 0, 192, 7, 0, 192, 1, 0, 6, 208, 0, 0, 74, 11, 137],
            // A call can consume or replace value-stack cells.
            vec![48, 0, 192, 7, 0, 8, 6, 208, 0, 0, 74, 11, 137, 137],
        ] {
            let inspected = inspect(&fixture_body(&body)).unwrap();
            assert!(inspected.actions[0].callback_error.is_none());
            assert!(inspected.actions[0].motions.is_empty());
            assert_eq!(inspected.actions[0].unresolved_motion_requests, 1);
        }
    }
    #[test]
    fn deep_blend_expression_preserves_the_inspection_budget() {
        let mut body = vec![48, 0, 192, 7, 0, 48, 4];
        // Authored unary float negations leave the blend in one stack cell.
        // Its depth exceeds this inspector's slice budget, so the request
        // remains unknown instead of searching arbitrarily far backwards.
        body.extend(std::iter::repeat_n(149, 65));
        body.extend([74, 11, 137]);
        let inspected = inspect(&fixture_body(&body)).unwrap();
        assert!(inspected.actions[0].callback_error.is_none());
        assert!(inspected.actions[0].motions.is_empty());
        assert_eq!(inspected.actions[0].unresolved_motion_requests, 1);
    }
    #[test]
    fn immediate_operand_subfields_do_not_change_literal_kind() {
        let mut bytes = fixture(false);
        // Integer/float immediate kind uses the low six opcode bits. This
        // generated script uses the other valid subfield spelling.
        for at in (44..bytes.len() - 8).step_by(2) {
            let opcode = u16::from_le_bytes([bytes[at], bytes[at + 1]]);
            if opcode == 192 {
                bytes[at..at + 2].copy_from_slice(&0u16.to_le_bytes());
            }
            if opcode == 208 {
                bytes[at..at + 2].copy_from_slice(&16u16.to_le_bytes());
            }
        }
        let decoded = inspect(&bytes).unwrap();
        assert_eq!(decoded.actions[0].motions[0].logical_motion, 7);
    }
    #[test]
    fn invalid_callback_is_reported_without_losing_action() {
        let mut bytes = fixture(false);
        // The callback literal is data for the registration, not a direct
        // branch: failure belongs to this action rather than the whole script.
        bytes[16 + 22 * 2..16 + 22 * 2 + 2].copy_from_slice(&0xffffu16.to_le_bytes());
        let decoded = inspect(&bytes).unwrap();
        assert_eq!(decoded.actions.len(), 1);
        assert!(decoded.actions[0].callback_error.is_some());
        assert!(decoded.actions[0].motions.is_empty());
    }
    #[test]
    fn malformed_registration_is_retained() {
        let mut bytes = fixture(false);
        // Use a structurally valid nonliteral name push.
        bytes[16 + 16 * 2..16 + 16 * 2 + 2].copy_from_slice(&96u16.to_le_bytes());
        let decoded = inspect(&bytes).unwrap();
        assert!(decoded.actions.is_empty());
        assert_eq!(decoded.unresolved_registrations, 1);
    }
    #[test]
    fn variant_flag_is_source_header_not_filename() {
        let mut bytes = b"BAR\x01".to_vec();
        bytes.extend([0u8; 12]);
        assert!(!uses_motion_variants(&bytes).unwrap());
        bytes[12] = 1;
        assert!(uses_motion_variants(&bytes).unwrap());
        assert!(uses_motion_variants(&bytes[..12]).is_err());
    }
}
