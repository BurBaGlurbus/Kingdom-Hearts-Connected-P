//! Bounded enemy-script inspection. Word addresses are relative to byte 16.
//! T106: native interpreter/address helper inspected with Ghidra. This module
//! decodes bytes only; it does not implement traps or the PC runtime patches.
use serde::Serialize;
use std::{collections::BTreeMap, io};

fn invalid(s: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, s)
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Trigger {
    pub id: i32,
    pub address: u32,
}

pub struct Program<'a> {
    bytes: &'a [u8],
    pub name: &'a [u8],
    pub work_bytes: u32,
    pub stack_bytes: u32,
    pub temp_bytes: u32,
    pub triggers: Vec<Trigger>,
    pub code_start: u32,
}

/// Raw operands stay unsigned bit patterns, including float immediates.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Instruction {
    pub address: u32,
    pub opcode: u16,
    pub words: u32,
    pub operands: Vec<u32>,
    pub target: Option<u32>,
}

impl Instruction {
    pub fn trap(&self) -> Option<(u16, u16)> {
        (self.opcode & 15 == 10).then(|| (self.opcode >> 6, self.operands[0] as u16))
    }

    fn falls_through(&self) -> bool {
        // Halt resumes at the next instruction. Exit and return do not.
        !matches!(self.opcode & 0xffcf, 0x0007 | 0x0049 | 0x0089)
    }
}

impl<'a> Program<'a> {
    pub fn read(bytes: &'a [u8]) -> io::Result<Self> {
        if bytes.len() < 36 || bytes.len() > 4 * 1024 * 1024 {
            return Err(invalid("bdscript length outside inspection bounds"));
        }
        let word = |at: usize| -> io::Result<u32> {
            let b = bytes
                .get(at..at + 4)
                .ok_or_else(|| invalid("truncated bdscript header"))?;
            Ok(u32::from_le_bytes(b.try_into().unwrap()))
        };
        let mut triggers = Vec::new();
        let mut at = 28;
        loop {
            let id = word(at)? as i32;
            let address = word(at + 4)?;
            at += 8;
            if id == 0 && address == 0 {
                break;
            }
            if triggers.len() >= 256 {
                return Err(invalid("too many bdscript triggers"));
            }
            triggers.push(Trigger { id, address });
        }
        let p = Self {
            bytes,
            name: &bytes[..bytes[..16].iter().position(|&b| b == 0).unwrap_or(16)],
            work_bytes: word(16)?,
            stack_bytes: word(20)?,
            temp_bytes: word(24)?,
            triggers,
            code_start: ((at - 16) / 2) as u32,
        };
        if [p.work_bytes, p.stack_bytes, p.temp_bytes]
            .iter()
            .any(|&v| v > 4 * 1024 * 1024)
        {
            return Err(invalid("bdscript allocation outside inspection bounds"));
        }
        for t in &p.triggers {
            p.code_word(t.address)?;
        }
        Ok(p)
    }

    fn code_word(&self, address: u32) -> io::Result<u16> {
        if address < self.code_start {
            return Err(invalid("bdscript code address overlaps header"));
        }
        let at = 16usize
            .checked_add(address as usize * 2)
            .ok_or_else(|| invalid("address overflow"))?;
        let b = self
            .bytes
            .get(at..at + 2)
            .ok_or_else(|| invalid("bdscript code outside payload"))?;
        Ok(u16::from_le_bytes(b.try_into().unwrap()))
    }

    pub fn instruction(&self, address: u32) -> io::Result<Instruction> {
        let opcode = self.code_word(address)?;
        let class = opcode & 15;
        let mode = (opcode >> 4) & 3;
        let sub = opcode >> 6;
        let words = match class {
            0 if mode <= 1 => 3,
            0 | 1 if sub <= 3 => 2,
            2 if sub <= 3 => 3,
            3 => 2,
            4 => 1,
            5 if (mode == 0 && matches!(sub, 0 | 2..=11))
                || (mode == 1 && matches!(sub, 1 | 2 | 5..=11)) =>
            {
                1
            }
            6 if (mode == 0 && sub <= 11) || (mode == 1 && sub <= 4) => 1,
            7 if sub <= 2 => 2,
            8 | 10 => 2,
            9 if matches!(sub, 0..=3 | 5..=9) => 1,
            11 => 3,
            _ => return Err(invalid("unsupported bdscript opcode")),
        };
        let mut operands = Vec::new();
        for i in 1..words {
            operands.push(
                self.code_word(
                    address
                        .checked_add(i)
                        .ok_or_else(|| invalid("address overflow"))?,
                )? as u32,
            );
        }
        let target = if matches!(class, 7 | 8 | 11) {
            let relative = if class == 11 {
                (operands[0] | operands[1] << 16) as i32
            } else {
                operands[0] as u16 as i16 as i32
            };
            // Native PC is a 32-bit instruction index: do not wrap to u16.
            let t = i64::from(address) + i64::from(words) + i64::from(relative);
            let t = u32::try_from(t).map_err(|_| invalid("bdscript branch address overflow"))?;
            self.code_word(t)?;
            Some(t)
        } else {
            None
        };
        Ok(Instruction {
            address,
            opcode,
            words,
            operands,
            target,
        })
    }

    /// Follows trigger roots and direct branches/calls only. Act callbacks
    /// passed as integers to traps are deliberately not guessed as code.
    pub fn direct_graph(&self) -> io::Result<Vec<Instruction>> {
        let mut pending: Vec<_> = self.triggers.iter().map(|t| t.address).collect();
        let mut instructions = BTreeMap::<u32, Instruction>::new();
        let mut occupied = BTreeMap::new();
        while let Some(address) = pending.pop() {
            if instructions.contains_key(&address) {
                continue;
            }
            let instruction = self.instruction(address)?;
            for word in address..address + instruction.words {
                if occupied.insert(word, address).is_some() {
                    return Err(invalid("bdscript branch overlaps instruction operand"));
                }
            }
            if let Some(target) = instruction.target {
                pending.push(target);
            }
            if instruction.falls_through() {
                pending.push(address + instruction.words);
            }
            instructions.insert(address, instruction);
        }
        Ok(instructions.into_values().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(code: &[u16]) -> Vec<u8> {
        let mut b = vec![0; 44];
        b[..4].copy_from_slice(b"test");
        b[28..32].copy_from_slice(&1i32.to_le_bytes());
        b[32..36].copy_from_slice(&14u32.to_le_bytes());
        for w in code {
            b.extend(w.to_le_bytes());
        }
        b
    }
    #[test]
    fn walks_branches_calls_and_resume_without_decoding_dead_data() {
        // conditional at 14 -> 20; fallthrough calls 21; exit at 18.
        let b = fixture(&[0x47, 4, 0x88, 3, 0x49, 0xffff, 0x49, 9, 0x89]);
        let p = Program::read(&b).unwrap();
        let graph = p.direct_graph().unwrap();
        assert_eq!(
            graph.iter().map(|i| i.address).collect::<Vec<_>>(),
            [14, 16, 18, 20, 21, 22]
        );
    }
    #[test]
    fn rejects_truncation_bad_targets_and_operand_overlap() {
        for code in [&[0u16][..], &[7, 0xfff0], &[0x47, 0xffff, 0x49], &[0xffff]] {
            let b = fixture(code);
            assert!(Program::read(&b).unwrap().direct_graph().is_err());
        }
        assert!(Program::read(&[0; 28]).is_err());
    }
    #[test]
    fn preserves_float_bits_and_unsigned_trap_index() {
        let b = fixture(&[0x10, 0, 0x8000, 0x8a, 0xffff, 0x49]);
        let g = Program::read(&b).unwrap().direct_graph().unwrap();
        assert_eq!(g[0].operands, [0, 0x8000]);
        assert_eq!(g[1].trap(), Some((2, 65535)));
    }
    #[test]
    fn signed_long_call_and_backwards_loop_are_bounded() {
        let b = fixture(&[0x8b, 1, 0, 0x49, 7, 0xfffe]);
        let g = Program::read(&b).unwrap().direct_graph().unwrap();
        assert_eq!(g.len(), 3);
        assert_eq!(g[2].target, Some(18));
    }
}
