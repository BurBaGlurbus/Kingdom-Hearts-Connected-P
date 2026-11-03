//! Explicit public-reader CPU profile; not a PS2 hardware emulator.
//! OpenKH VifUnpacker.cs at 7a3b945c538d32c6a285128c98aefba093f52ceb.
//! Retains its first-Run, zero-TOPS, mask, sign and vector-fill behavior.
use std::io;

pub const MEMORY_BYTES: usize = 16 * 1024;
pub const PROFILE: &str = "openkh_vif_first_run_zero_tops_v1";

pub struct Unpacked {
    pub memory: [u8; MEMORY_BYTES],
    pub program_counter: usize,
    pub stop_state: &'static str,
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn read(code: &[u8], pc: &mut usize, width: usize, signed: bool) -> io::Result<u32> {
    let bytes = code
        .get(*pc..*pc + width)
        .ok_or_else(|| invalid("truncated VIF operand"))?;
    *pc += width;
    Ok(match (width, signed) {
        (1, true) => bytes[0] as i8 as i32 as u32,
        (1, false) => bytes[0] as u32,
        (2, true) => i16::from_le_bytes(bytes.try_into().unwrap()) as i32 as u32,
        (2, false) => u16::from_le_bytes(bytes.try_into().unwrap()) as u32,
        (4, _) => u32::from_le_bytes(bytes.try_into().unwrap()),
        _ => unreachable!(),
    })
}

pub fn unpack_first_run(code: &[u8]) -> io::Result<Unpacked> {
    let mut unpacker = Unpacker::new(code)?;
    unpacker.run()?;
    Ok(unpacker.output)
}

/// Stateful public-reader profile: memory and mask survive microprogram stops.
/// This observes uploaded data; it does not execute VU microprograms.
pub struct Unpacker<'a> {
    code: &'a [u8],
    pub output: Unpacked,
    mask: u32,
    commands: usize,
}

impl<'a> Unpacker<'a> {
    pub fn new(code: &'a [u8]) -> io::Result<Self> {
        if code.len() > 1024 * 1024 || !code.len().is_multiple_of(4) {
            return Err(invalid("VIF code exceeds 1 MiB or is not word aligned"));
        }
        Ok(Self {
            code,
            mask: 0,
            commands: 0,
            output: Unpacked {
                memory: [0; MEMORY_BYTES],
                program_counter: 0,
                stop_state: "end",
            },
        })
    }

    pub fn run(&mut self) -> io::Result<&'static str> {
        let code = self.code;
        let output = &mut self.output;
        output.stop_state = "end";
        while output.program_counter < code.len() {
            self.commands += 1;
            if self.commands > 65536 {
                return Err(invalid("VIF exceeds 65536 commands"));
            }
            let word = read(code, &mut output.program_counter, 4, false)?;
            let cmd = (word >> 24) & 0x7f;
            if cmd & 0x60 != 0x60 {
                match cmd {
                    0 | 1 => {} // public STCYCL stores the cycle but does not use it
                    0x14 | 0x17 => {
                        output.stop_state = "microprogram";
                        return Ok(output.stop_state);
                    }
                    0x20 => self.mask = read(code, &mut output.program_counter, 4, false)?,
                    0x30 | 0x31 => {
                        // Public row/column registers are stored but never read back.
                        for _ in 0..4 {
                            read(code, &mut output.program_counter, 4, false)?;
                        }
                    }
                    _ => return Err(invalid("unqualified VIF command for explicit profile")),
                }
                continue;
            }
            let count = ((word >> 16) & 0xff) as usize;
            let vn = ((word >> 26) & 3) as usize;
            let width = match (word >> 24) & 3 {
                0 => 4,
                1 | 3 => 2, // literal public reader's VL=3 route; not a packed-color claim
                2 => 1,
                _ => unreachable!(),
            };
            let signed = word & 0x400 != 0; // literal public reader polarity
            let masked = word & (1 << 28) != 0;
            let mut destination = (word as usize & 0x1ff) * 16;
            for vector in 0..count {
                let lane_mask = if masked {
                    (self.mask >> ((vector & 3) * 8)) as u8
                } else {
                    0
                };
                let mut values = [0; 4];
                // Public scalar/V2 reads operands even when every lane is masked.
                if vn == 0 {
                    values.fill(read(code, &mut output.program_counter, width, signed)?);
                } else if vn == 1 {
                    values[0] = read(code, &mut output.program_counter, width, signed)?;
                    values[1] = read(code, &mut output.program_counter, width, signed)?;
                    values[2] = values[0];
                    values[3] = values[1];
                }
                for (lane, value) in values.iter_mut().enumerate() {
                    let writes = (lane_mask >> (lane * 2)) & 3 == 0;
                    if writes {
                        if vn >= 2 {
                            if vn == 2 && lane == 3 {
                                let mut peek = output.program_counter;
                                *value = read(code, &mut peek, width, signed)?;
                            } else {
                                *value = read(code, &mut output.program_counter, width, signed)?;
                            }
                        }
                        output
                            .memory
                            .get_mut(destination..destination + 4)
                            .ok_or_else(|| invalid("VIF write exceeds 16 KiB memory"))?
                            .copy_from_slice(&value.to_le_bytes());
                    }
                    destination += 4;
                }
            }
            output.program_counter = (output.program_counter + 3) & !3;
            if output.program_counter > code.len() {
                return Err(invalid("VIF alignment exceeds code"));
            }
        }
        Ok(output.stop_state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn append(bytes: &mut Vec<u8>, word: u32) {
        bytes.extend(word.to_le_bytes());
    }
    #[test]
    fn continuation_preserves_memory_mask_and_reaches_end() {
        let mut code = Vec::new();
        for word in [
            0x6c01_0000,
            1,
            2,
            3,
            4,
            0x2000_0000,
            0xaaaa_aa8a,
            0x1400_0000,
            0x7201_0000,
            9,
            0x1700_0000,
        ] {
            append(&mut code, word);
        }
        let mut unpacker = Unpacker::new(&code).unwrap();
        assert_eq!(unpacker.run().unwrap(), "microprogram");
        assert_eq!(unpacker.output.program_counter, 32);
        assert_eq!(unpacker.run().unwrap(), "microprogram");
        assert_eq!(
            &unpacker.output.memory[..16],
            &[1u32, 2, 9, 4]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>()
        );
        assert_eq!(unpacker.run().unwrap(), "end");
    }
    #[test]
    fn preserves_masked_lanes_sign_extension_and_vector_peek() {
        let mut code = Vec::new();
        append(&mut code, 0x6501_0400); // public signed V2-16
        code.extend((-2i16).to_le_bytes());
        code.extend(7i16.to_le_bytes());
        append(&mut code, 0x2000_0000);
        append(&mut code, 0xaaaa_aaaa); // suppress writes except Z after edit
        code[12..16].copy_from_slice(&0xaaaa_aa8au32.to_le_bytes());
        append(&mut code, 0x7201_0000); // masked scalar8 overwrites only Z
        code.extend([9, 0, 0, 0]);
        let output = unpack_first_run(&code).unwrap();
        let words: Vec<_> = output.memory[..16]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| u32::from_le_bytes(*b))
            .collect();
        assert_eq!(words, [u32::MAX - 1, 7, 9, 7]);
        let mut code = Vec::new();
        append(&mut code, 0x6801_0000); // V3-32 peeks the next word into W
        for value in [1, 2, 3, 0x1400_0000] {
            append(&mut code, value);
        }
        let output = unpack_first_run(&code).unwrap();
        assert_eq!(output.stop_state, "microprogram");
        assert_eq!(output.program_counter, 20);
        assert_eq!(&output.memory[12..16], &0x1400_0000u32.to_le_bytes());
    }
    #[test]
    fn retains_first_stop_and_public_zero_count_while_rejecting_bad_ranges() {
        let code = [0x6c00_0000u32, 0x1400_0000, 0xffff_ffff]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>();
        let output = unpack_first_run(&code).unwrap();
        assert_eq!(output.program_counter, 8);
        assert!(output.memory.iter().all(|&b| b == 0));
        for code in [
            vec![1],
            0x0200_0000u32.to_le_bytes().to_vec(),
            0x6c01_0000u32.to_le_bytes().to_vec(),
            vec![0; 65537 * 4],
            vec![0; 1024 * 1024 + 4],
        ] {
            assert!(unpack_first_run(&code).is_err());
        }
        let mut code = 0x6c02_01ffu32.to_le_bytes().to_vec();
        code.extend([1; 32]);
        let output = unpack_first_run(&code).unwrap();
        assert_eq!(&output.memory[0x1ff * 16..0x1ff * 16 + 32], &[1; 32]);
    }
}
