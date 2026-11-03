//! PC sound banks ("SEDBSSCF" SCD), as the installed exe loads them from
//! `se/se%03d.win32.scd` and from the `se/obj/seNNN` replacement assets of
//! object archives. Sound `no` of a bank lists {track, audio} layers; each
//! audio stream here is MS-ADPCM (codec 12) or, for music, Ogg Vorbis
//! (codec 6, decoded with lewton), as interleaved i16 PCM.
use crate::ogg;
use std::io;

const MAGIC: &[u8; 8] = b"SEDBSSCF";
const CODEC_MS_ADPCM: i32 = 12;
const CODEC_VORBIS: i32 = 6;
const MAX_FRAMES: usize = 48_000 * 600;
const ADAPT: [i32; 16] = [
    230, 230, 230, 230, 307, 409, 512, 614, 768, 614, 512, 409, 307, 230, 230, 230,
];

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_owned())
}

#[derive(Debug)]
pub struct Bank<'a> {
    bytes: &'a [u8],
    version: u32,
    pub number: u16,
    sounds: Vec<usize>,
    audio: Vec<usize>,
}

/// One playable sound: its volume, loop attribute and layers.
#[derive(Debug, Clone, PartialEq)]
pub struct Sound {
    pub volume: f32,
    pub looped: bool,
    /// Audio stream indices played together.
    pub audio: Vec<u16>,
}

/// Decoded stream: interleaved samples, loop frames as stored.
#[derive(Debug, Clone, PartialEq)]
pub struct Pcm {
    pub channels: u16,
    pub rate: u32,
    pub samples: Vec<i16>,
    pub loop_start: u32,
    pub loop_end: u32,
}

impl Pcm {
    pub fn frames(&self) -> usize {
        self.samples.len() / self.channels.max(1) as usize
    }
}

fn u16_at(b: &[u8], at: usize) -> io::Result<u16> {
    b.get(at..at + 2)
        .map(|w| u16::from_le_bytes([w[0], w[1]]))
        .ok_or_else(|| invalid("truncated SCD"))
}

fn u32_at(b: &[u8], at: usize) -> io::Result<u32> {
    b.get(at..at + 4)
        .map(|w| u32::from_le_bytes(w.try_into().unwrap()))
        .ok_or_else(|| invalid("truncated SCD"))
}

fn table(b: &[u8], at: usize, count: usize) -> io::Result<Vec<usize>> {
    (0..count)
        .map(|i| u32_at(b, at + i * 4).map(|o| o as usize))
        .collect()
}

pub fn read(bytes: &[u8]) -> io::Result<Bank<'_>> {
    if bytes.get(..8) != Some(MAGIC) {
        return Err(invalid("not an SEDBSSCF sound bank"));
    }
    let version = u32_at(bytes, 8)?;
    if !(3..=4).contains(&version) {
        return Err(invalid("unsupported SCD version"));
    }
    let header = u16_at(bytes, 0x0e)? as usize;
    let sound_count = u16_at(bytes, header)? as usize;
    let audio_count = u16_at(bytes, header + 4)? as usize;
    let number = u16_at(bytes, header + 6)?;
    let audio_table = u32_at(bytes, header + 0x0c)? as usize;
    Ok(Bank {
        bytes,
        version,
        number,
        sounds: table(bytes, header + 0x20, sound_count)?,
        audio: table(bytes, audio_table, audio_count)?,
    })
}

impl Bank<'_> {
    pub fn sound_count(&self) -> usize {
        self.sounds.len()
    }

    pub fn audio_count(&self) -> usize {
        self.audio.len()
    }

    /// Sound `no`, or None outside the bank (the engine drops it).
    pub fn sound(&self, no: usize) -> io::Result<Option<Sound>> {
        let Some(&at) = self.sounds.get(no) else {
            return Ok(None);
        };
        let b = self.bytes;
        let layers = *b.get(at).ok_or_else(|| invalid("truncated SCD sound"))? as usize;
        let kind = *b
            .get(at + 3)
            .ok_or_else(|| invalid("truncated SCD sound"))?;
        let attributes = u16_at(b, at + 4)?;
        let volume = f32::from_bits(u32_at(b, at + 8)?);
        // Random/selection sounds (types 12/13) carry 8-byte items; none
        // of the banks used here have them.
        if kind != 1 {
            return Ok(Some(Sound {
                volume,
                looped: false,
                audio: Vec::new(),
            }));
        }
        let first = at + if self.version >= 4 { 0x40 } else { 0x10 };
        let audio = (0..layers)
            .map(|i| u16_at(b, first + i * 4 + 2))
            .collect::<io::Result<_>>()?;
        Ok(Some(Sound {
            volume,
            looped: attributes & 1 != 0,
            audio,
        }))
    }

    /// Decodes audio stream `index`; None for an empty stream (codec −1).
    pub fn decode(&self, index: usize) -> io::Result<Option<Pcm>> {
        let at = *self
            .audio
            .get(index)
            .ok_or_else(|| invalid("SCD audio index outside the bank"))?;
        let b = self.bytes;
        let size = u32_at(b, at)? as usize;
        let channels = u32_at(b, at + 4)?;
        let rate = u32_at(b, at + 8)?;
        let codec = u32_at(b, at + 0x0c)? as i32;
        let loop_start = u32_at(b, at + 0x10)?;
        let loop_end = u32_at(b, at + 0x14)?;
        let extra = u32_at(b, at + 0x18)? as usize;
        if codec == -1 {
            return Ok(None);
        }
        if codec == CODEC_VORBIS {
            return self.vorbis(index)?.map(|v| decode_vorbis(&v)).transpose();
        }
        if codec != CODEC_MS_ADPCM {
            return Err(invalid("SCD codec other than MS-ADPCM"));
        }
        if !(1..=2).contains(&channels) {
            return Err(invalid("SCD stream channel count"));
        }
        let format = b
            .get(at + 0x20..at + 0x20 + extra)
            .ok_or_else(|| invalid("truncated SCD extradata"))?;
        let data = b
            .get(at + 0x20 + extra..at + 0x20 + extra + size)
            .ok_or_else(|| invalid("truncated SCD audio"))?;
        let samples = decode_ms_adpcm(format, channels as usize, data)?;
        Ok(Some(Pcm {
            channels: channels as u16,
            rate,
            samples,
            loop_start,
            loop_end,
        }))
    }
}

/// A codec-6 stream: the de-obfuscated Vorbis header pages, the Ogg data
/// and the loop values (FUN_1400ce8a0).
#[derive(Debug, Clone, PartialEq)]
pub struct VorbisStream<'a> {
    pub channels: u32,
    pub rate: u32,
    /// Page-aligned byte offset of the loop start in `data`; 0 with
    /// `looped` false for one-shots.
    pub loop_start_byte: u32,
    pub looped: bool,
    /// Samples dropped after a loop seek (miniheader +0x08).
    pub skip: u32,
    pub header: Vec<u8>,
    pub data: &'a [u8],
}

impl<'a> Bank<'a> {
    /// Audio stream `index` as a Vorbis stream (codec 6).
    pub fn vorbis(&self, index: usize) -> io::Result<Option<VorbisStream<'a>>> {
        let at = *self
            .audio
            .get(index)
            .ok_or_else(|| invalid("SCD audio index outside the bank"))?;
        let b = self.bytes;
        let size = u32_at(b, at)? as usize;
        let codec = u32_at(b, at + 0x0c)? as i32;
        if codec != CODEC_VORBIS {
            return Ok(None);
        }
        let loop_start = u32_at(b, at + 0x10)?;
        let loop_end = u32_at(b, at + 0x14)?;
        let extra = u32_at(b, at + 0x18)? as usize;
        // Auxiliary chunks (+0x1c bit 0) sit before the miniheader.
        let mini = if u32_at(b, at + 0x1c)? & 1 != 0 {
            at + 0x20 + u32_at(b, at + 0x24)? as usize
        } else {
            at + 0x20
        };
        let head = b
            .get(mini..mini + 0x20)
            .ok_or_else(|| invalid("truncated SCD Vorbis miniheader"))?;
        let (version, mini_size, key) = (head[0], head[1] as usize, head[2]);
        let skip = u32_at(head, 0x08)?;
        let seek = u32_at(head, 0x10)? as usize;
        let header_size = u32_at(head, 0x14)? as usize;
        let start = mini + mini_size + seek;
        let mut header = b
            .get(start..start + header_size)
            .ok_or_else(|| invalid("truncated SCD Vorbis header"))?
            .to_vec();
        if version == 2 && header.get(..4) != Some(b"OggS") {
            for x in &mut header {
                *x ^= key;
            }
        }
        let data = b
            .get(at + 0x20 + extra..at + 0x20 + extra + size)
            .ok_or_else(|| invalid("truncated SCD Vorbis data"))?;
        Ok(Some(VorbisStream {
            channels: u32_at(b, at + 4)?,
            rate: u32_at(b, at + 8)?,
            loop_start_byte: loop_start,
            looped: loop_end != 0,
            skip,
            header,
            data,
        }))
    }
}

/// Decodes a whole codec-6 stream with lewton. PCM is
/// `clamp(trunc(f x 32767))` (FUN_1400af3a0); the loop runs from the
/// granule of the page at the loop byte offset plus the skip count to the
/// last granule (bgm-input-01 §A2.1), end inclusive.
pub fn decode_vorbis(v: &VorbisStream) -> io::Result<Pcm> {
    let bad = |e: &dyn std::fmt::Debug| invalid(&format!("Vorbis: {e:?}"));
    let mut headers = ogg::Packets::new(&v.header, 0);
    let mut next = || -> io::Result<Vec<u8>> {
        Ok(headers
            .next_packet()?
            .ok_or_else(|| invalid("Vorbis header packets missing"))?
            .data)
    };
    let identification = next()?;
    let _comment = next()?;
    let setup = next()?;
    let ident = lewton::header::read_header_ident(&identification).map_err(|e| bad(&e))?;
    let setup = lewton::header::read_header_setup(
        &setup,
        ident.audio_channels,
        (ident.blocksize_0, ident.blocksize_1),
    )
    .map_err(|e| bad(&e))?;
    let channels = ident.audio_channels as usize;
    if channels as u32 != v.channels {
        return Err(invalid("Vorbis channel count differs from the SCD"));
    }
    let mut window = lewton::audio::PreviousWindowRight::new();
    let mut samples: Vec<i16> = Vec::new();
    let mut packets = ogg::Packets::new(v.data, 0);
    let mut last_granule = 0i64;
    while let Some(p) = packets.next_packet()? {
        let pcm: Vec<Vec<f32>> =
            lewton::audio::read_audio_packet_generic(&ident, &setup, &p.data, &mut window)
                .map_err(|e| bad(&e))?;
        if let Some(first) = pcm.first() {
            for i in 0..first.len() {
                for c in &pcm {
                    samples.push((c[i] * 32767.).trunc().clamp(-32768., 32767.) as i16);
                }
            }
        }
        if p.granule >= 0 {
            last_granule = p.granule;
        }
        if samples.len() / channels > MAX_FRAMES {
            return Err(invalid("Vorbis stream exceeds decode bound"));
        }
    }
    // The last page's granule trims the final packet.
    samples.truncate(last_granule.max(0) as usize * channels);
    let (loop_start, loop_end) = if v.looped {
        let page = ogg::page(v.data, v.loop_start_byte as usize)?;
        (
            (page.granule.max(0) as u32).saturating_add(v.skip),
            (last_granule.max(1) - 1) as u32,
        )
    } else {
        (0, 0)
    };
    Ok(Pcm {
        channels: channels as u16,
        rate: ident.audio_sample_rate,
        samples,
        loop_start,
        loop_end,
    })
}
/// MS-ADPCM with a WAVEFORMATEX + ADPCMWAVEFORMAT `format` block.
pub fn decode_ms_adpcm(format: &[u8], channels: usize, data: &[u8]) -> io::Result<Vec<i16>> {
    let align = u16_at(format, 12)? as usize;
    let per_block = u16_at(format, 18)? as usize;
    let coef_count = u16_at(format, 20)? as usize;
    let coefs: Vec<(i32, i32)> = (0..coef_count)
        .map(|i| {
            Ok((
                u16_at(format, 22 + i * 4)? as i16 as i32,
                u16_at(format, 24 + i * 4)? as i16 as i32,
            ))
        })
        .collect::<io::Result<_>>()?;
    let header = 7 * channels;
    if align <= header || per_block < 2 || (align - header) * 2 / channels + 2 < per_block {
        return Err(invalid("MS-ADPCM block layout"));
    }
    let blocks = data.len() / align;
    if blocks * per_block > MAX_FRAMES {
        return Err(invalid("MS-ADPCM stream exceeds decode bound"));
    }
    let mut out = Vec::with_capacity(blocks * per_block * channels);
    for block in data.chunks_exact(align) {
        let i16_at = |k: usize| i16::from_le_bytes([block[k], block[k + 1]]) as i32;
        let mut coef = [(0, 0); 2];
        let mut delta = [0; 2];
        let mut s1 = [0; 2];
        let mut s2 = [0; 2];
        for c in 0..channels {
            coef[c] = *coefs
                .get(block[c] as usize)
                .ok_or_else(|| invalid("MS-ADPCM predictor"))?;
            delta[c] = i16_at(channels + 2 * c);
            s1[c] = i16_at(3 * channels + 2 * c);
            s2[c] = i16_at(5 * channels + 2 * c);
        }
        out.extend((0..channels).map(|c| s2[c] as i16));
        out.extend((0..channels).map(|c| s1[c] as i16));
        let nibbles = block[header..]
            .iter()
            .flat_map(|&byte| [byte >> 4, byte & 15])
            .take((per_block - 2) * channels);
        for (k, n) in nibbles.enumerate() {
            let c = k % channels;
            let signed = if n >= 8 { n as i32 - 16 } else { n as i32 };
            let predicted = (s1[c] * coef[c].0 + s2[c] * coef[c].1) >> 8;
            let value = (predicted + signed * delta[c]).clamp(-32768, 32767);
            out.push(value as i16);
            s2[c] = s1[c];
            s1[c] = value;
            delta[c] = ((ADAPT[n as usize] * delta[c]) >> 8).max(16);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn format(channels: u16, align: u16) -> Vec<u8> {
        let mut f = Vec::new();
        f.extend(2u16.to_le_bytes());
        f.extend(channels.to_le_bytes());
        f.extend(48000u32.to_le_bytes());
        f.extend(0u32.to_le_bytes());
        f.extend(align.to_le_bytes());
        f.extend(4u16.to_le_bytes());
        f.extend(32u16.to_le_bytes());
        f.extend(32u16.to_le_bytes());
        f.extend(7u16.to_le_bytes());
        for (a, b) in [
            (256i16, 0i16),
            (512, -256),
            (0, 0),
            (192, 64),
            (240, 0),
            (460, -208),
            (392, -232),
        ] {
            f.extend(a.to_le_bytes());
            f.extend(b.to_le_bytes());
        }
        f
    }

    fn mono_block(pred: u8, delta: i16, s1: i16, s2: i16, nibble_byte: u8) -> Vec<u8> {
        let mut b = vec![pred];
        b.extend(delta.to_le_bytes());
        b.extend(s1.to_le_bytes());
        b.extend(s2.to_le_bytes());
        b.extend([nibble_byte; 15]);
        b
    }

    #[test]
    fn decodes_mono_blocks_by_the_standard_rules() {
        // Predictor 0 (256, 0): v = s1 + n·delta; nibble 1 keeps delta 230/256·d.
        let data = mono_block(0, 16, 100, 50, 0x11);
        let pcm = decode_ms_adpcm(&format(1, 22), 1, &data).unwrap();
        assert_eq!(pcm.len(), 32);
        assert_eq!(&pcm[..4], &[50, 100, 116, 132]);
        // Negative nibble 0xF = −1, clamped at the i16 range.
        let data = mono_block(0, 16, -32768, 0, 0xff);
        let pcm = decode_ms_adpcm(&format(1, 22), 1, &data).unwrap();
        assert_eq!(pcm[2], -32768);
        // Unknown predictor and bad block sizes are rejected.
        assert!(decode_ms_adpcm(&format(1, 22), 1, &mono_block(9, 16, 0, 0, 0)).is_err());
        assert!(decode_ms_adpcm(&format(1, 6), 1, &data).is_err());
    }

    #[test]
    fn interleaves_stereo_nibbles_by_channel() {
        let mut block = vec![0u8, 0];
        block.extend(16i16.to_le_bytes());
        block.extend(32i16.to_le_bytes());
        block.extend(10i16.to_le_bytes());
        block.extend(20i16.to_le_bytes());
        block.extend(1i16.to_le_bytes());
        block.extend(2i16.to_le_bytes());
        block.extend([0x1f; 30]);
        let pcm = decode_ms_adpcm(&format(2, 44), 2, &block).unwrap();
        assert_eq!(pcm.len(), 64);
        // Frames: (s2), (s1), then left +1·16, right −1·32.
        assert_eq!(&pcm[..6], &[1, 2, 10, 20, 26, -12]);
    }

    fn bank(version: u32) -> Vec<u8> {
        let mut b = vec![0u8; 0x30];
        b[..8].copy_from_slice(MAGIC);
        b[8..12].copy_from_slice(&version.to_le_bytes());
        b[0x0e..0x10].copy_from_slice(&0x30u16.to_le_bytes());
        // Table header: 2 sounds, 0 tracks, 1 audio, bank 201.
        let mut t = vec![0u8; 0x28];
        t[0..2].copy_from_slice(&2u16.to_le_bytes());
        t[4..6].copy_from_slice(&1u16.to_le_bytes());
        t[6..8].copy_from_slice(&201u16.to_le_bytes());
        b.extend(t);
        let entry = if version >= 4 { 0x40 } else { 0x10 };
        let sound0 = b.len();
        let sound1 = sound0 + entry + 4;
        let audio_table = sound1 + entry;
        let audio0 = audio_table + 4;
        b[0x30 + 0x0c..0x30 + 0x10].copy_from_slice(&(audio_table as u32).to_le_bytes());
        b[0x30 + 0x20..0x30 + 0x24].copy_from_slice(&(sound0 as u32).to_le_bytes());
        b[0x30 + 0x24..0x30 + 0x28].copy_from_slice(&(sound1 as u32).to_le_bytes());
        let mut s = vec![0u8; entry];
        s[0] = 1;
        s[3] = 1;
        s[4..6].copy_from_slice(&0x81u16.to_le_bytes());
        s[8..12].copy_from_slice(&0.8f32.to_le_bytes());
        b.extend(s);
        b.extend([0, 0, 0, 0]);
        let mut empty = vec![0u8; entry];
        empty[3] = 16;
        empty[8..12].copy_from_slice(&1f32.to_le_bytes());
        b.extend(empty);
        b.extend((audio0 as u32).to_le_bytes());
        let f = format(1, 22);
        let data = mono_block(0, 16, 0, 0, 0);
        for w in [
            data.len() as u32,
            1,
            48000,
            12,
            3,
            20,
            f.len() as u32,
            0x0100_0000,
        ] {
            b.extend(w.to_le_bytes());
        }
        b.extend(f);
        b.extend(data);
        b
    }

    #[test]
    fn reads_synthetic_banks_of_both_versions() {
        for version in [3, 4] {
            let bytes = bank(version);
            let bank = read(&bytes).unwrap();
            assert_eq!((bank.number, bank.sound_count()), (201, 2));
            let sound = bank.sound(0).unwrap().unwrap();
            assert_eq!(sound.audio, vec![0]);
            assert!(sound.looped);
            assert_eq!(sound.volume, 0.8);
            assert!(bank.sound(1).unwrap().unwrap().audio.is_empty());
            assert!(bank.sound(2).unwrap().is_none());
            let pcm = bank.decode(0).unwrap().unwrap();
            assert_eq!((pcm.channels, pcm.rate, pcm.frames()), (1, 48000, 32));
            assert_eq!((pcm.loop_start, pcm.loop_end), (3, 20));
        }
        let mut bytes = bank(3);
        bytes[0] = b'X';
        assert!(read(&bytes).is_err());
    }
}
