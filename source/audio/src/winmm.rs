//! Windows waveOut playback: a thread keeps a ring of prepared buffers
//! queued, refilling each one from `fill` as the device marks it done.
#![allow(unsafe_code)]

use std::{
    ffi::c_void,
    io,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

const WAVE_MAPPER: u32 = u32::MAX;
const CALLBACK_NULL: u32 = 0;
const WAVE_FORMAT_PCM: u16 = 1;
const WHDR_DONE: u32 = 1;
const BUFFERS: usize = 4;
const FRAMES_PER_BUFFER: usize = 512;

#[repr(C, packed(1))]
struct WaveFormatEx {
    format_tag: u16,
    channels: u16,
    samples_per_sec: u32,
    avg_bytes_per_sec: u32,
    block_align: u16,
    bits_per_sample: u16,
    size: u16,
}

#[repr(C)]
struct WaveHdr {
    data: *mut u8,
    buffer_length: u32,
    bytes_recorded: u32,
    user: usize,
    flags: u32,
    loops: u32,
    next: *mut WaveHdr,
    reserved: usize,
}

type Handle = *mut c_void;

#[link(name = "winmm")]
unsafe extern "system" {
    fn waveOutOpen(
        out: *mut Handle,
        device: u32,
        format: *const WaveFormatEx,
        callback: usize,
        instance: usize,
        flags: u32,
    ) -> u32;
    fn waveOutPrepareHeader(out: Handle, header: *mut WaveHdr, size: u32) -> u32;
    fn waveOutUnprepareHeader(out: Handle, header: *mut WaveHdr, size: u32) -> u32;
    fn waveOutWrite(out: Handle, header: *mut WaveHdr, size: u32) -> u32;
    fn waveOutReset(out: Handle) -> u32;
    fn waveOutClose(out: Handle) -> u32;
}

const HEADER_SIZE: u32 = size_of::<WaveHdr>() as u32;

fn check(result: u32, what: &str) -> io::Result<()> {
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "{what} failed (MMRESULT {result})"
        )))
    }
}

pub struct Device {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

/// The open device handle, moved into the playback thread.
struct Open(Handle);
// SAFETY: a waveOut handle may be used from any thread; only the playback
// thread uses it after `open` returns.
unsafe impl Send for Open {}

impl Device {
    pub fn open(rate: u32, mut fill: impl FnMut(&mut [i16]) + Send + 'static) -> io::Result<Self> {
        let format = WaveFormatEx {
            format_tag: WAVE_FORMAT_PCM,
            channels: 2,
            samples_per_sec: rate,
            avg_bytes_per_sec: rate * 4,
            block_align: 4,
            bits_per_sample: 16,
            size: 0,
        };
        let mut handle: Handle = std::ptr::null_mut();
        // SAFETY: `handle` and `format` outlive the call; no callback is used.
        check(
            unsafe { waveOutOpen(&mut handle, WAVE_MAPPER, &format, 0, 0, CALLBACK_NULL) },
            "waveOutOpen",
        )?;
        let open = Open(handle);
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let thread = thread::Builder::new()
            .name("audio".into())
            .spawn(move || run(open, &stopping, &mut fill))?;
        Ok(Self {
            stop,
            thread: Some(thread),
        })
    }
}

fn run(open: Open, stop: &AtomicBool, fill: &mut dyn FnMut(&mut [i16])) {
    let handle = open.0;
    // Buffers and headers live in boxes that do not move until unprepared.
    let mut buffers: Vec<Box<[i16]>> = (0..BUFFERS)
        .map(|_| vec![0i16; FRAMES_PER_BUFFER * 2].into_boxed_slice())
        .collect();
    let mut headers: Box<[WaveHdr]> = buffers
        .iter_mut()
        .map(|b| WaveHdr {
            data: b.as_mut_ptr().cast(),
            buffer_length: (b.len() * 2) as u32,
            bytes_recorded: 0,
            user: 0,
            flags: 0,
            loops: 0,
            next: std::ptr::null_mut(),
            reserved: 0,
        })
        .collect();
    let mut prepared = 0;
    for header in headers.iter_mut() {
        // SAFETY: the header and its buffer stay in place until unprepared.
        if unsafe { waveOutPrepareHeader(handle, header, HEADER_SIZE) } != 0 {
            break;
        }
        header.flags |= WHDR_DONE;
        prepared += 1;
    }
    let mut next = 0;
    while prepared == BUFFERS && !stop.load(Ordering::Relaxed) {
        let header: *mut WaveHdr = &mut headers[next];
        // SAFETY: the device sets WHDR_DONE from its own thread; read the
        // flag word volatilely through the header's stable address.
        let flags = unsafe { std::ptr::addr_of!((*header).flags).read_volatile() };
        if flags & WHDR_DONE == 0 {
            thread::sleep(Duration::from_millis(2));
            continue;
        }
        fill(&mut buffers[next]);
        // SAFETY: the buffer is no longer queued (WHDR_DONE), so it may be
        // refilled and queued again.
        if unsafe { waveOutWrite(handle, header, HEADER_SIZE) } != 0 {
            break;
        }
        next = (next + 1) % BUFFERS;
    }
    // SAFETY: reset returns every queued buffer before unpreparing; the
    // handle is closed last and never used again.
    unsafe {
        waveOutReset(handle);
        for header in headers.iter_mut().take(prepared) {
            waveOutUnprepareHeader(handle, header, HEADER_SIZE);
        }
        waveOutClose(handle);
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
