//! Object-script interpreter, translated from the KH2FM executable (Steam build
//! 15194255): the instruction loop FUN_14041b400 and its address helpers
//! FUN_14041c6c0 (pop to memory), FUN_14041c780 (push from memory),
//! FUN_14041c840 (tag offset of an address), FUN_14041c890 (tag class) and
//! FUN_14041c900 (address of an operand), plus the thread set-up of
//! FUN_14041b320 / FUN_1403e1240.
//!
//! Values are 8-byte slots `{value, tag}`; the tag marks integers, floats,
//! addresses and never-written cells (`@INT`, `@FLT`, `@ADR`, `?` fill). Script
//! memory (work area, frame stack) keeps a shadow copy of every tag. The
//! executable hands scripts 32-bit handles for its 64-bit pointers
//! (FUN_1404ad240/270); here a handle is an address in a synthetic map:
//!
//! | high bits | meaning |
//! |---|---|
//! | `0x1ii0_0000 + n` | byte `n` of the payload of instance `ii` (strings are at `16 + 2·word`) |
//! | `0x2ii0_0000 + n` | work area of instance `ii` |
//! | `0x3iit_0000 + n` | frame memory of thread `t` of instance `ii` |
//! | `0x5000_0000` | the 4-byte scratch cell of `push.d.bd` (DAT_142aea790) |
//! | `0x6000_0000..` | engine memory, reached through [`Host::peek`]/[`Host::poke`] |

pub const TAG_INT: u32 = 0x544e_4940;
pub const TAG_FLT: u32 = 0x544c_4640;
pub const TAG_ADR: u32 = 0x5244_4140;
/// Fill for cells that were never written (`0x3f3f3f40`).
pub const TAG_NONE: u32 = 0x3f3f_3f40;

pub const CODE_SEGMENT: u32 = 0x1000_0000;
pub const WORK_SEGMENT: u32 = 0x2000_0000;
pub const FRAME_SEGMENT: u32 = 0x3000_0000;
pub const SCRATCH: u32 = 0x5000_0000;
pub const EXTERN_SEGMENT: u32 = 0x6000_0000;
/// Handles of child threads (returned by `trap_thread_start`).
pub const CHILD_THREADS: u32 = 0x6400_0000;

/// One evaluation-stack cell.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Slot {
    pub value: u32,
    pub tag: u32,
}

impl Slot {
    pub const fn int(v: i32) -> Self {
        Self {
            value: v as u32,
            tag: TAG_INT,
        }
    }
    pub fn float(v: f32) -> Self {
        Self {
            value: v.to_bits(),
            tag: TAG_FLT,
        }
    }
    pub const fn address(handle: u32) -> Self {
        Self {
            value: handle,
            tag: TAG_ADR,
        }
    }
    pub fn as_int(self) -> i32 {
        self.value as i32
    }
    pub fn as_float(self) -> f32 {
        f32::from_bits(self.value)
    }
}

/// How `run` stopped. Codes are the executable's return values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exit {
    /// 1: `halt`, resume next tick.
    Halt,
    /// 2: `exit`.
    Exit,
    /// 3: `ret` out of the outermost frame.
    End,
    /// 5: an opcode the executable rejects.
    BadOpcode,
    /// Not a native outcome: a memory access outside every region the
    /// executable could address, or evaluation-stack misuse.
    Fault(Fault),
    /// `run` consumed its instruction budget (not a native outcome).
    Budget,
}

impl Exit {
    pub fn code(self) -> i32 {
        match self {
            Exit::Halt => 1,
            Exit::Exit => 2,
            Exit::End => 3,
            Exit::BadOpcode => 5,
            Exit::Fault(_) => -1,
            Exit::Budget => 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    Memory(u32),
    StackUnderflow,
    StackOverflow,
    DivideByZero,
    Trap { table: u16, index: u16 },
    CodeOutside(i32),
}

/// A script routine a trap asked to run right away on a throw-away thread
/// (FUN_1403e2000), after the trap returns and before its result is pushed.
#[derive(Clone, Debug, PartialEq)]
pub struct Callback {
    pub entry: u16,
    pub args: Vec<Slot>,
}

/// Everything the interpreter needs from the engine.
pub trait Host {
    /// Callbacks the trap that just ran queued, in order.
    fn take_callbacks(&mut self) -> Vec<Callback> {
        Vec::new()
    }
    /// `DAT_140717480`: 1.0 at 60 fps and normal speed.
    fn frame_time(&self) -> f32 {
        1.0
    }
    /// The float at +0xc of `FUN_140431920`'s record, read by one HD literal patch.
    fn patch_gate(&self) -> f32 {
        0.0
    }
    /// Read engine memory (handles at or above [`EXTERN_SEGMENT`]).
    fn peek(&self, _address: u32) -> Option<u32> {
        None
    }
    fn poke(&mut self, _address: u32, _value: u32) -> bool {
        false
    }
    /// Flags of a trap: low 16 bits = argument count, bit 30 = returns a
    /// value. `None` for a slot the executable's table does not have.
    fn trap_flags(&self, table: u16, index: u16) -> Option<u32>;
    /// Run a trap with `args[..count]` popped from the stack (first pushed =
    /// `args[0]`). When the flags return a value, the result is `args[0]`.
    fn trap(
        &mut self,
        memory: &mut Memory<'_>,
        table: u16,
        index: u16,
        args: &mut [Slot; 16],
    ) -> Result<(), Fault>;
}

/// One object's script: the payload, the shared work area and its sizes.
#[derive(Clone, Debug)]
pub struct Instance {
    pub id: u32,
    pub code: Vec<u8>,
    pub work: Vec<u8>,
    pub work_bytes: u32,
    pub stack_bytes: u32,
    pub temp_bytes: u32,
    scratch: u32,
    next_thread: u32,
}

/// Programs the executable's thread set-up (FUN_1403e1240) gives a 1024-byte
/// stack: those declaring 512 bytes whose name is one of these (DAT_1405c8610,
/// 1405c8618, 1405c8620, strcmp with "blizzard", DAT_1405c8628).
const BIG_STACK_NAMES: [&[u8]; 5] = [b"b_ex370", b"b_ex420", b"p_ex330", b"blizzard", b"m_ex880"];

impl Instance {
    /// An empty stand-in used while the real instance is lent to the interpreter.
    pub fn placeholder() -> Self {
        Self {
            id: 0,
            code: Vec::new(),
            work: Vec::new(),
            work_bytes: 0,
            stack_bytes: 0,
            temp_bytes: 0,
            scratch: 0,
            next_thread: 0,
        }
    }

    pub fn new(id: u32, payload: &[u8]) -> Option<Self> {
        if payload.len() < 36 {
            return None;
        }
        let word = |at: usize| u32::from_le_bytes(payload[at..at + 4].try_into().unwrap());
        let (work_bytes, mut stack_bytes, temp_bytes) = (word(16), word(20), word(24));
        if work_bytes > 0x4_0000 || stack_bytes > 0x1_0000 || temp_bytes > 0x1_0000 {
            return None;
        }
        let mut code = payload.to_vec();
        let name_end = code[..16].iter().position(|&b| b == 0).unwrap_or(16);
        if stack_bytes == 0x200 && BIG_STACK_NAMES.contains(&&code[..name_end]) {
            stack_bytes = 0x400;
            code[0x14..0x18].copy_from_slice(&0x400u32.to_le_bytes());
        }
        Some(Self {
            id,
            code,
            work: vec![0; work_bytes as usize * 2],
            work_bytes,
            stack_bytes,
            temp_bytes,
            scratch: 0,
            next_thread: 0,
        })
    }

    pub fn code_base(&self) -> u32 {
        CODE_SEGMENT + (self.id << 20)
    }
    pub fn work_base(&self) -> u32 {
        WORK_SEGMENT + (self.id << 20)
    }
    pub fn name(&self) -> &[u8] {
        let end = self.code[..16].iter().position(|&b| b == 0).unwrap_or(16);
        &self.code[..end]
    }
    /// Entry word index of trigger `key` (FUN_14041c690), 0 if absent.
    pub fn trigger(&self, key: i32) -> i32 {
        let mut at = 28usize;
        loop {
            let Some(k) = self.code.get(at..at + 8) else {
                return 0;
            };
            let k = i32::from_le_bytes(k[..4].try_into().unwrap());
            let a = i32::from_le_bytes(self.code[at + 4..at + 8].try_into().unwrap());
            if k == 0 && a == 0 {
                return 0;
            }
            if k == key {
                return a;
            }
            at += 8;
        }
    }
    fn word(&self, index: i32) -> Option<u16> {
        let at = 16usize.checked_add((index as isize).checked_mul(2)? as usize)?;
        if index < 0 {
            return None;
        }
        let b = self.code.get(at..at + 2)?;
        Some(u16::from_le_bytes([b[0], b[1]]))
    }
}

/// A thread started by `trap_thread_start` (FUN_1403e1e30), ticked after its
/// parent every frame.
#[derive(Clone, Debug)]
pub struct Child {
    /// +0x3c: the kind mask scripts use to replace or kill threads.
    pub kind: u32,
    /// +0x40: a caller-chosen word.
    pub tag: u32,
    pub handle: u32,
    pub thread: Thread,
}

/// One script thread (FUN_1403e1240 / FUN_14041b320).
#[derive(Clone, Debug)]
pub struct Thread {
    pub pc: i32,
    pub slot: u32,
    frame: Vec<u8>,
    fp: u32,
    eval: Vec<Slot>,
    eval_capacity: usize,
    /// The thread's own code has returned (halt resumes; anything else ends).
    pub ended: bool,
    pub children: Vec<Child>,
}

impl Thread {
    /// FUN_1403e1c80's result: the thread has ended and no child remains.
    pub fn finished(&self) -> bool {
        self.ended && self.children.is_empty()
    }

    /// A thread starting at word `entry`, with `args` pushed first-to-last.
    pub fn new(inst: &Instance, slot: u32, entry: i32, args: &[Slot]) -> Self {
        Self::with_sizes(inst.stack_bytes, inst.temp_bytes, slot, entry, args)
    }

    /// As [`Thread::new`] from the program's declared stack and temp sizes.
    pub fn with_sizes(
        stack_bytes: u32,
        temp_bytes: u32,
        slot: u32,
        entry: i32,
        args: &[Slot],
    ) -> Self {
        let s = stack_bytes as usize;
        let mut t = Self {
            pc: entry,
            slot,
            frame: vec![0; s * 2 + 0x100],
            fp: 0,
            eval: Vec::new(),
            eval_capacity: (temp_bytes as usize * 2 + 0x100) / 8,
            ended: false,
            children: Vec::new(),
        };
        // FUN_14041b320: sixteen bytes of zeroed, untagged locals.
        let tag_at = (s & !3) as u32;
        for _ in 0..4 {
            t.frame_set_tag(tag_at, 0, TAG_NONE);
            t.fp += 4;
        }
        t.eval.extend_from_slice(args);
        t
    }
    /// [I] Wraps as the release build does once instance ids grow large
    /// (a debug build would stop on the overflow).
    pub fn frame_base(&self, inst: &Instance) -> u32 {
        FRAME_SEGMENT
            .wrapping_add(inst.id.wrapping_shl(20))
            .wrapping_add(self.slot.wrapping_shl(16))
    }
    fn frame_set_tag(&mut self, tag_at: u32, value: u32, tag: u32) {
        let o = self.fp as usize;
        self.frame[o..o + 4].copy_from_slice(&value.to_le_bytes());
        let t = o + tag_at as usize;
        self.frame[t..t + 4].copy_from_slice(&tag.to_le_bytes());
    }
    /// Evaluation stack, bottom first (inspection).
    pub fn stack(&self) -> &[Slot] {
        &self.eval
    }
    pub fn push(&mut self, slot: Slot) {
        self.eval.push(slot);
    }
}

/// Memory view a trap receives: the script's own regions.
pub struct Memory<'a> {
    inst: &'a mut Instance,
    th: &'a mut Thread,
}

#[derive(Clone, Copy)]
enum Loc {
    Code(usize),
    Work(usize),
    Frame(usize),
    Scratch,
    Extern,
    Bad,
}

impl<'a> Memory<'a> {
    /// A view of one thread's memory (traps receive it from the interpreter).
    pub fn new(inst: &'a mut Instance, th: &'a mut Thread) -> Self {
        Self { inst, th }
    }
}

impl Memory<'_> {
    fn locate(&self, a: u32) -> Loc {
        let off = |base: u32, len: usize| {
            let o = a.wrapping_sub(base) as usize;
            (a >= base && o < len).then_some(o)
        };
        if let Some(o) = off(self.inst.code_base(), self.inst.code.len()) {
            Loc::Code(o)
        } else if let Some(o) = off(self.inst.work_base(), self.inst.work.len()) {
            Loc::Work(o)
        } else if let Some(o) = off(self.th.frame_base(self.inst), self.th.frame.len()) {
            Loc::Frame(o)
        } else if a == SCRATCH {
            Loc::Scratch
        } else if a >= EXTERN_SEGMENT {
            Loc::Extern
        } else {
            Loc::Bad
        }
    }
    /// FUN_14041c840: distance from a tracked cell to its tag, 0 when untracked.
    fn tag_offset(&self, a: u32) -> usize {
        match self.locate(a) {
            Loc::Work(o) if o < self.inst.work_bytes as usize => self.inst.work_bytes as usize,
            Loc::Frame(o) if o < self.inst.stack_bytes as usize => {
                self.inst.stack_bytes as usize & !3
            }
            _ => 0,
        }
    }
    pub fn read_u32(&self, a: u32) -> Option<u32> {
        let get = |v: &[u8], o: usize| Some(u32::from_le_bytes(v.get(o..o + 4)?.try_into().ok()?));
        match self.locate(a) {
            Loc::Code(o) => get(&self.inst.code, o),
            Loc::Work(o) => get(&self.inst.work, o),
            Loc::Frame(o) => get(&self.th.frame, o),
            Loc::Scratch => Some(self.inst.scratch),
            Loc::Extern | Loc::Bad => None,
        }
    }
    pub fn write_u32(&mut self, a: u32, v: u32) -> bool {
        let put = |buf: &mut [u8], o: usize| match buf.get_mut(o..o + 4) {
            Some(d) => {
                d.copy_from_slice(&v.to_le_bytes());
                true
            }
            None => false,
        };
        match self.locate(a) {
            Loc::Code(o) => put(&mut self.inst.code, o),
            Loc::Work(o) => put(&mut self.inst.work, o),
            Loc::Frame(o) => put(&mut self.th.frame, o),
            Loc::Scratch => {
                self.inst.scratch = v;
                true
            }
            Loc::Extern | Loc::Bad => false,
        }
    }
    pub fn read_f32(&self, a: u32) -> Option<f32> {
        self.read_u32(a).map(f32::from_bits)
    }
    pub fn write_f32(&mut self, a: u32, v: f32) -> bool {
        self.write_u32(a, v.to_bits())
    }
    /// NUL-terminated byte string at `a` (at most `limit` bytes).
    pub fn string(&self, a: u32, limit: usize) -> Option<Vec<u8>> {
        let mut out = Vec::new();
        for i in 0..limit as u32 {
            let c = self.read_u8(a.checked_add(i)?)?;
            if c == 0 {
                return Some(out);
            }
            out.push(c);
        }
        None
    }
    pub fn read_u8(&self, a: u32) -> Option<u8> {
        match self.locate(a) {
            Loc::Code(o) => self.inst.code.get(o).copied(),
            Loc::Work(o) => self.inst.work.get(o).copied(),
            Loc::Frame(o) => self.th.frame.get(o).copied(),
            Loc::Scratch => Some(self.inst.scratch.to_le_bytes()[0]),
            Loc::Extern | Loc::Bad => None,
        }
    }
    pub fn write_u8(&mut self, a: u32, v: u8) -> bool {
        match self.locate(a) {
            Loc::Code(o) => self.inst.code.get_mut(o).map(|b| *b = v).is_some(),
            Loc::Work(o) => self.inst.work.get_mut(o).map(|b| *b = v).is_some(),
            Loc::Frame(o) => self.th.frame.get_mut(o).map(|b| *b = v).is_some(),
            Loc::Scratch | Loc::Extern | Loc::Bad => false,
        }
    }
    /// A tracked cell with its tag; untracked cells report [`TAG_NONE`].
    pub fn load(&self, a: u32) -> Option<Slot> {
        let value = self.read_u32(a)?;
        let t = self.tag_offset(a);
        let tag = if t > 2 {
            self.read_u32(a + t as u32)?
        } else {
            TAG_NONE
        };
        Some(Slot { value, tag })
    }
    /// Write a slot, keeping the tag when the cell is tracked.
    pub fn store(&mut self, a: u32, slot: Slot) -> bool {
        let t = self.tag_offset(a);
        let ok = self.write_u32(a, slot.value);
        if t > 2 {
            return ok && self.write_u32(a + t as u32, slot.tag);
        }
        ok
    }
    pub fn work_base(&self) -> u32 {
        self.inst.work_base()
    }

    /// FUN_1403e1e30: start a child thread of the running thread. Returns its
    /// handle.
    pub fn spawn_child(&mut self, kind: u32, tag: u32, entry: i32, args: &[Slot]) -> u32 {
        self.inst.next_thread += 1;
        let handle = CHILD_THREADS + self.inst.next_thread * 16;
        let slot = 3 + self.inst.next_thread % 12;
        let thread = Thread::with_sizes(
            self.inst.stack_bytes,
            self.inst.temp_bytes,
            slot,
            entry,
            args,
        );
        self.th.children.push(Child {
            kind,
            tag,
            handle,
            thread,
        });
        handle
    }

    /// FUN_1403e1f00: end the running thread's children whose kind matches.
    pub fn kill_children(&mut self, mask: u32, value: u32) {
        self.th.children.retain(|c| c.kind & mask != value);
    }
    pub fn instance(&self) -> &Instance {
        self.inst
    }
}

fn tag_class(tag: u32) -> i32 {
    match tag {
        TAG_NONE => -1,
        TAG_ADR => 2,
        TAG_FLT | TAG_INT => 1,
        _ => 0,
    }
}

fn hd_curve(x: f32) -> f32 {
    // FUN_1403b4d30.
    let root = (x * 8.0 + 1.0).sqrt();
    (root - 1.0) * 0.5 * 0.35 + ((x + x) / (x + 1.0)) * 0.65
}

/// Frame slot of throw-away callback threads, and their instruction budget.
const CALLBACK_THREAD: u32 = 15;
const CALLBACK_BUDGET: u64 = 1_000_000;

/// Run `th` until it halts, exits, ends or faults, executing at most `budget`
/// instructions.
pub fn run(inst: &mut Instance, th: &mut Thread, host: &mut dyn Host, budget: u64) -> Exit {
    let hd = host.frame_time() < 2.0;
    let fire = hd && inst.name().starts_with(b"fire");
    let name = inst.name().to_vec();
    let mut spent = 0u64;
    loop {
        if spent >= budget {
            return Exit::Budget;
        }
        spent += 1;
        match step(inst, th, host, hd, fire, &name) {
            Ok(None) => {}
            Ok(Some(exit)) => return exit,
            Err(fault) => return Exit::Fault(fault),
        }
    }
}

/// FUN_1403e1c80: run the thread once unless it has ended, then each child
/// (recursively). True once the thread has ended and no child is left.
pub fn tick_tree(inst: &mut Instance, th: &mut Thread, host: &mut dyn Host, budget: u64) -> Exit {
    let mut result = Exit::Halt;
    if !th.ended {
        result = run(inst, th, host, budget);
        if result != Exit::Halt {
            th.ended = true;
        }
    }
    let mut i = 0;
    while i < th.children.len() {
        let mut child = std::mem::replace(
            &mut th.children[i],
            Child {
                kind: 0,
                tag: 0,
                handle: 0,
                thread: Thread::with_sizes(0, 0, 0, 0, &[]),
            },
        );
        tick_tree(inst, &mut child.thread, host, budget);
        let done = child.thread.ended && child.thread.children.is_empty();
        if done {
            th.children.remove(i);
        } else {
            th.children[i] = child;
            i += 1;
        }
    }
    result
}

fn pop(th: &mut Thread) -> Result<Slot, Fault> {
    th.eval.pop().ok_or(Fault::StackUnderflow)
}

fn push(th: &mut Thread, s: Slot) -> Result<(), Fault> {
    if th.eval.len() >= th.eval_capacity {
        return Err(Fault::StackOverflow);
    }
    th.eval.push(s);
    Ok(())
}

fn th_read(inst: &Instance, th: &Thread, a: u32, host: &dyn Host) -> Option<u32> {
    // Reads of the frame/work/code regions without a `Memory` view.
    let fb = th.frame_base(inst);
    if a >= fb && ((a - fb) as usize) + 4 <= th.frame.len() {
        let o = (a - fb) as usize;
        return Some(u32::from_le_bytes(th.frame[o..o + 4].try_into().unwrap()));
    }
    let wb = inst.work_base();
    if a >= wb && ((a - wb) as usize) + 4 <= inst.work.len() {
        let o = (a - wb) as usize;
        return Some(u32::from_le_bytes(inst.work[o..o + 4].try_into().unwrap()));
    }
    host.peek(a)
}

#[allow(clippy::too_many_arguments)]
fn step(
    inst: &mut Instance,
    th: &mut Thread,
    host: &mut dyn Host,
    hd: bool,
    fire: bool,
    name: &[u8],
) -> Result<Option<Exit>, Fault> {
    let pc0 = th.pc;
    let op = inst.word(pc0).ok_or(Fault::CodeOutside(pc0))?;
    th.pc = pc0 + 1;
    let at2 = pc0.wrapping_mul(2);
    match op & 0xf {
        0 => class0(inst, th, host, hd, name, op, pc0, at2)?,
        1 => {
            if at2 == 0x26a
                && fire
                && let Some(top) = th.eval.last_mut()
                && top.as_float() == 44.0
            {
                *top = Slot {
                    value: (top.as_float() - 2.0).to_bits(),
                    tag: top.tag,
                };
            }
            let a = operand_address(inst, th, host, op)?;
            pop_to(inst, th, host, a)?;
        }
        2 => {
            let src = pop(th)?.value;
            let size = inst.word(th.pc).ok_or(Fault::CodeOutside(th.pc))? as usize;
            let off = inst.word(th.pc + 1).ok_or(Fault::CodeOutside(th.pc + 1))? as u32;
            th.pc += 2;
            let dst = match op >> 6 {
                0 => th.frame_base(inst) + th.fp + off,
                1 => inst.work_base() + off,
                2 => th_read(inst, th, th.frame_base(inst) + th.fp, host)
                    .ok_or(Fault::Memory(th.frame_base(inst) + th.fp))?
                    .wrapping_add(off),
                _ => return Err(Fault::Memory(0)),
            };
            copy_block(inst, th, host, dst, src, size)?;
        }
        3 => {
            let base = pop(th)?.value;
            let imm = inst.word(th.pc).ok_or(Fault::CodeOutside(th.pc))? as u32;
            th.pc += 1;
            let s = load(inst, th, host, base.wrapping_add(imm))?;
            push(th, s)?;
        }
        4 => {
            let value = pop(th)?;
            let dst = pop(th)?.value;
            if op < 0x40 {
                store_slot(inst, th, host, dst, value)?;
            } else {
                let size = (op >> 6) as usize;
                copy_block(inst, th, host, dst, value.value, size)?;
            }
        }
        5 => {
            if let Some(exit) = unary(th, op)? {
                return Ok(Some(exit));
            }
        }
        6 => {
            if let Some(exit) = binary(inst, th, op)? {
                return Ok(Some(exit));
            }
        }
        7 => {
            let rel = inst.word(th.pc).ok_or(Fault::CodeOutside(th.pc))? as i16 as i32;
            let next = pc0 + 2;
            th.pc = next;
            match op >> 6 {
                0 => th.pc = next + rel,
                1 => {
                    if pop(th)?.value == 0 {
                        th.pc = next + rel;
                    }
                }
                2 => {
                    if pop(th)?.value != 0 {
                        th.pc = next + rel;
                    }
                }
                _ => return Ok(Some(Exit::BadOpcode)),
            }
        }
        8 | 11 => {
            let (rel, next) = if op & 0xf == 8 {
                (
                    inst.word(pc0 + 1).ok_or(Fault::CodeOutside(pc0 + 1))? as i16 as i32,
                    pc0 + 2,
                )
            } else {
                let lo = inst.word(pc0 + 1).ok_or(Fault::CodeOutside(pc0 + 1))? as u32;
                let hi = inst.word(pc0 + 2).ok_or(Fault::CodeOutside(pc0 + 2))? as u32;
                ((hi << 16 | lo) as i32, pc0 + 3)
            };
            let k = (op >> 6) as u32;
            th.fp += k * 4;
            let tag_at = (inst.stack_bytes & !3) as usize;
            let fp = th.fp as usize;
            if fp < 8 || fp + tag_at > th.frame.len() {
                return Err(Fault::Memory(th.frame_base(inst) + th.fp));
            }
            th.frame[fp - 4..fp].copy_from_slice(&(next as u32).to_le_bytes());
            th.frame[fp - 8..fp - 4].copy_from_slice(&k.to_le_bytes());
            th.frame[fp - 4 + tag_at..fp + tag_at].copy_from_slice(&TAG_NONE.to_le_bytes());
            th.frame[fp - 8 + tag_at..fp - 4 + tag_at].copy_from_slice(&TAG_NONE.to_le_bytes());
            th.pc = next + rel;
        }
        9 => match op >> 6 {
            0 => return Ok(Some(Exit::Halt)),
            1 => return Ok(Some(Exit::Exit)),
            2 => {
                let fp = th.fp as usize;
                if fp < 8 {
                    return Err(Fault::Memory(th.frame_base(inst) + th.fp));
                }
                let ret = u32::from_le_bytes(th.frame[fp - 4..fp].try_into().unwrap());
                let k = i32::from_le_bytes(th.frame[fp - 8..fp - 4].try_into().unwrap());
                th.pc = ret as i32;
                th.fp = (fp as i64 - k as i64 * 4).max(0) as u32;
                if ret == 0 {
                    return Ok(Some(Exit::End));
                }
            }
            3 => {
                pop(th)?;
            }
            5 => {
                let s = pop(th)?;
                push(th, s)?;
                push(th, s)?;
            }
            6 => {
                let s = pop(th)?;
                let x = wrap_angle(s.as_float());
                push(th, Slot::float(x.sin()))?;
            }
            7 => {
                let s = pop(th)?;
                let x = wrap_angle(s.as_float());
                let c = if (std::f32::consts::FRAC_PI_2 % std::f32::consts::TAU
                    - std::f32::consts::PI)
                    == x
                {
                    0.0
                } else {
                    x.cos()
                };
                push(th, Slot::float(c))?;
            }
            8 => {
                let s = pop(th)?;
                let v = ((s.as_float() * std::f32::consts::PI) / 180.0 + std::f32::consts::PI)
                    % std::f32::consts::TAU
                    - std::f32::consts::PI;
                push(th, Slot::float(v))?;
            }
            9 => {
                let s = pop(th)?;
                push(th, Slot::float(s.as_float() * 180.0 / std::f32::consts::PI))?;
            }
            _ => return Ok(Some(Exit::BadOpcode)),
        },
        10 => {
            let table = op >> 6;
            let index = inst.word(th.pc).ok_or(Fault::CodeOutside(th.pc))?;
            th.pc += 1;
            let flags = host
                .trap_flags(table, index)
                .ok_or(Fault::Trap { table, index })?;
            let count = (flags & 0xffff) as usize;
            if count > 16 || th.eval.len() < count {
                return Err(Fault::StackUnderflow);
            }
            let mut args = [Slot::default(); 16];
            for k in (0..count).rev() {
                args[k] = th.eval.pop().ok_or(Fault::StackUnderflow)?;
            }
            let mut mem = Memory { inst, th };
            host.trap(&mut mem, table, index, &mut args)?;
            for cb in host.take_callbacks() {
                let mut temp = Thread::new(inst, CALLBACK_THREAD, i32::from(cb.entry), &cb.args);
                run(inst, &mut temp, host, CALLBACK_BUDGET);
            }
            if flags & 0x4000_0000 != 0 {
                push(th, args[0])?;
            }
        }
        _ => return Ok(Some(Exit::BadOpcode)),
    }
    Ok(None)
}

/// CVTTSS2SI: out-of-range and NaN give the "integer indefinite" value.
fn truncate_to_int(f: f32) -> i32 {
    if f.is_nan() || f >= 2147483648.0 || f < -2147483648.0 {
        i32::MIN
    } else {
        f as i32
    }
}

fn wrap_angle(x: f32) -> f32 {
    (x + std::f32::consts::PI) % std::f32::consts::TAU - std::f32::consts::PI
}

#[allow(clippy::too_many_arguments)]
fn class0(
    inst: &mut Instance,
    th: &mut Thread,
    host: &mut dyn Host,
    hd: bool,
    name: &[u8],
    op: u16,
    pc0: i32,
    at2: i32,
) -> Result<(), Fault> {
    match op & 0x30 {
        0x00 | 0x10 => {
            let lo = inst.word(pc0 + 1).ok_or(Fault::CodeOutside(pc0 + 1))? as u32;
            let hi = inst.word(pc0 + 2).ok_or(Fault::CodeOutside(pc0 + 2))? as u32;
            let bits = hi << 16 | lo;
            th.pc = pc0 + 3;
            let slot = if op & 0x30 == 0 {
                Slot {
                    value: bits,
                    tag: TAG_INT,
                }
            } else {
                let mut v = f32::from_bits(bits);
                if hd {
                    v = hd_literal(host, name, v, at2);
                }
                Slot::float(v)
            };
            push(th, slot)
        }
        0x20 => {
            let a = operand_address(inst, th, host, op)?;
            push(th, Slot::address(a))
        }
        _ => {
            let a = operand_address_literal(inst, th, host, op)?;
            if at2 == 0x2c38 && op & 0xffc0 == 0x80 && name == b"b_ex330" {
                let imm_is_100 = inst.word(pc0 + 1) == Some(100);
                if imm_is_100 && th_read(inst, th, a, host) == Some(0) {
                    let mut mem = Memory { inst, th };
                    mem.write_u32(a, 0xdead_beef);
                }
            }
            let s = load(inst, th, host, a)?;
            push(th, s)
        }
    }
}

/// HD-port float literal patches (the `push.s` case of FUN_14041b400).
fn hd_literal(host: &dyn Host, name: &[u8], v: f32, at2: i32) -> f32 {
    let bits = v.to_bits();
    let is = |c: f32| v == c;
    if is(0.83) {
        if matches!(at2, 0x2e8 | 0x3aa4 | 0x734 | 0x378) {
            return hd_curve(v);
        }
    } else if is(0.8) && at2 == 0x40f2 {
        return hd_curve(v);
    } else if is(2.4) && at2 == 0x262a || is(0.08) && at2 == 0x8410 || is(0.2) && at2 == 0x6804 {
        return v * 0.5;
    } else if is(20.0) && at2 == 0x180c {
        return 18.0;
    } else if is(42.0) && at2 == 0x147a {
        // 3.141 <= gate < 3.142 (DAT_1405d4518 / DAT_1405d451c).
        let gate = host.patch_gate();
        if gate >= f32::from_bits(0x4049_0625) && gate < f32::from_bits(0x4049_1687) {
            return 21.0;
        }
        return v;
    } else if bits == 0x3ba3_d70a && at2 == 0x6380 || bits == 0x3d4c_cccd && at2 == 0x109e {
        return v * 0.5;
    }
    // Reached by every literal the chain above did not claim.
    if is(30.0) && at2 == 0x1a8c && name == b"b_ex240" {
        return v + v;
    }
    v
}

fn operand_address(
    inst: &Instance,
    th: &mut Thread,
    host: &dyn Host,
    op: u16,
) -> Result<u32, Fault> {
    let imm = inst.word(th.pc).ok_or(Fault::CodeOutside(th.pc))? as u32;
    th.pc += 1;
    resolve(inst, th, host, op >> 6, imm, false)
}

fn operand_address_literal(
    inst: &mut Instance,
    th: &mut Thread,
    host: &dyn Host,
    op: u16,
) -> Result<u32, Fault> {
    let imm = inst.word(th.pc).ok_or(Fault::CodeOutside(th.pc))? as u32;
    th.pc += 1;
    resolve_literal(inst, th, host, op >> 6, imm)
}

fn resolve(
    inst: &Instance,
    th: &Thread,
    host: &dyn Host,
    mode: u16,
    imm: u32,
    _literal: bool,
) -> Result<u32, Fault> {
    Ok(match mode {
        0 => th.frame_base(inst) + th.fp + imm,
        1 => inst.work_base() + imm,
        2 => {
            let slot0 = th.frame_base(inst) + th.fp;
            th_read(inst, th, slot0, host)
                .ok_or(Fault::Memory(slot0))?
                .wrapping_add(imm)
        }
        3 => inst.code_base() + 16 + imm * 2,
        _ => 0,
    })
}

/// FUN_14041c900 with `param_3 == 4`: `push.d.bd` reads two code words into
/// the scratch cell.
fn resolve_literal(
    inst: &mut Instance,
    th: &Thread,
    host: &dyn Host,
    mode: u16,
    imm: u32,
) -> Result<u32, Fault> {
    if mode == 3 {
        let lo = inst
            .word(imm as i32)
            .ok_or(Fault::CodeOutside(imm as i32))? as u32;
        let hi = inst
            .word(imm as i32 + 1)
            .ok_or(Fault::CodeOutside(imm as i32 + 1))? as u32;
        inst.scratch = hi << 16 | lo;
        return Ok(SCRATCH);
    }
    resolve(inst, th, host, mode, imm, true)
}

fn load(inst: &mut Instance, th: &mut Thread, host: &dyn Host, a: u32) -> Result<Slot, Fault> {
    let mem = Memory { inst, th };
    if let Some(s) = mem.load(a) {
        return Ok(s);
    }
    match mem.locate(a) {
        Loc::Extern => host
            .peek(a)
            .map(|value| Slot {
                value,
                tag: TAG_NONE,
            })
            .ok_or(Fault::Memory(a)),
        _ => Err(Fault::Memory(a)),
    }
}

fn pop_to(inst: &mut Instance, th: &mut Thread, host: &mut dyn Host, a: u32) -> Result<(), Fault> {
    let s = pop(th)?;
    store_slot_value(inst, th, host, a, s)
}

fn store_slot(
    inst: &mut Instance,
    th: &mut Thread,
    host: &mut dyn Host,
    a: u32,
    s: Slot,
) -> Result<(), Fault> {
    store_slot_value(inst, th, host, a, s)
}

fn store_slot_value(
    inst: &mut Instance,
    th: &mut Thread,
    host: &mut dyn Host,
    a: u32,
    s: Slot,
) -> Result<(), Fault> {
    let mut mem = Memory { inst, th };
    if matches!(mem.locate(a), Loc::Extern) {
        return host.poke(a, s.value).then_some(()).ok_or(Fault::Memory(a));
    }
    mem.store(a, s).then_some(()).ok_or(Fault::Memory(a))
}

/// `memcpy` of `size` bytes. When the destination is tracked its tags become
/// the never-written fill; source tags are not copied.
fn copy_block(
    inst: &mut Instance,
    th: &mut Thread,
    host: &mut dyn Host,
    dst: u32,
    src: u32,
    size: usize,
) -> Result<(), Fault> {
    if dst == src {
        return Ok(());
    }
    {
        let mem = Memory { inst, th };
        let t = mem.tag_offset(dst);
        if t > 2 {
            let mut mem = Memory { inst, th };
            for k in 0..size / 4 {
                mem.write_u32(dst + (t + k * 4) as u32, TAG_NONE);
            }
        }
    }
    let mut bytes = Vec::with_capacity(size);
    {
        let mem = Memory { inst, th };
        for k in 0..size as u32 {
            let a = src.wrapping_add(k);
            let b = match mem.read_u8(a) {
                Some(b) => b,
                None => {
                    let word = host.peek(a & !3).ok_or(Fault::Memory(a))?;
                    word.to_le_bytes()[(a & 3) as usize]
                }
            };
            bytes.push(b);
        }
    }
    let mut mem = Memory { inst, th };
    for (k, b) in bytes.into_iter().enumerate() {
        let a = dst.wrapping_add(k as u32);
        if matches!(mem.locate(a), Loc::Extern) {
            let word = host.peek(a & !3).unwrap_or(0);
            let mut w = word.to_le_bytes();
            w[(a & 3) as usize] = b;
            host.poke(a & !3, u32::from_le_bytes(w));
        } else if !mem.write_u8(a, b) {
            return Err(Fault::Memory(a));
        }
    }
    Ok(())
}

fn unary(th: &mut Thread, op: u16) -> Result<Option<Exit>, Fault> {
    let s = pop(th)?;
    let v = s.value as i32;
    let f = s.as_float();
    let sub = op >> 6;
    let r = match (op >> 4) & 3 {
        0 => match sub {
            0 => Slot::float(v as f32),
            2 => Slot::int(v.wrapping_neg()),
            3 => Slot::int(!v),
            4 | 8 => Slot::int((v == 0) as i32),
            5 => Slot::int(v.wrapping_abs()),
            6 => Slot::int((v < 0) as i32),
            7 => Slot::int((v <= 0) as i32),
            9 => Slot::int((v != 0) as i32),
            10 => Slot::int((v >= 0) as i32),
            11 => Slot::int((v > 0) as i32),
            _ => {
                th.eval.push(s);
                return Ok(Some(Exit::BadOpcode));
            }
        },
        1 => match sub {
            1 => Slot::int(truncate_to_int(f)),
            2 => Slot::float(-f),
            5 => Slot::float(f.abs()),
            6 => Slot::int((0.0 > f) as i32),
            7 => Slot::int((0.0 >= f) as i32),
            8 => Slot::int((f == 0.0) as i32),
            9 => Slot::int((f != 0.0) as i32),
            10 => Slot::int((f >= 0.0) as i32),
            11 => Slot::int((f > 0.0) as i32),
            _ => {
                th.eval.push(s);
                return Ok(Some(Exit::BadOpcode));
            }
        },
        _ => {
            th.eval.push(s);
            return Ok(Some(Exit::BadOpcode));
        }
    };
    th.eval.push(r);
    Ok(None)
}

fn binary(inst: &Instance, th: &mut Thread, op: u16) -> Result<Option<Exit>, Fault> {
    let right = pop(th)?;
    let left = pop(th)?;
    let (r, l) = (right.value as i32, left.value as i32);
    let (rf, lf) = (right.as_float(), left.as_float());
    let sub = op >> 6;
    let out = match (op >> 4) & 3 {
        0 => {
            if sub > 11 {
                th.eval.push(left);
                return Ok(Some(Exit::BadOpcode));
            }
            match sub {
                // An address operand makes the sum an address.
                0 => add_values(inst, th, left, right),
                1 => Slot::int(l.wrapping_sub(r)),
                2 => Slot::int(l.wrapping_mul(r)),
                3 => {
                    if r == 0 {
                        left
                    } else {
                        Slot::int(l.wrapping_div(r))
                    }
                }
                4 => {
                    if r == 0 {
                        return Err(Fault::DivideByZero);
                    }
                    Slot::int(l.wrapping_rem(r))
                }
                5 => Slot::int(l & r),
                6 => Slot::int(l | r),
                7 => Slot::int(l ^ r),
                8 => Slot::int(l.wrapping_shl(r as u32 & 31)),
                9 => Slot::int(l.wrapping_shr(r as u32 & 31)),
                10 => Slot::int((l != 0 && r != 0) as i32),
                _ => Slot::int((l != 0 || r != 0) as i32),
            }
        }
        1 => match sub {
            0 => Slot::float(lf + rf),
            1 => Slot::float(lf - rf),
            2 => Slot::float(lf * rf),
            3 => {
                if rf == 0.0 {
                    left
                } else {
                    Slot::float(lf / rf)
                }
            }
            4 => Slot::float(lf % rf),
            _ => {
                th.eval.push(left);
                return Ok(Some(Exit::BadOpcode));
            }
        },
        _ => {
            th.eval.push(left);
            return Ok(Some(Exit::BadOpcode));
        }
    };
    th.eval.push(out);
    Ok(None)
}

fn add_values(inst: &Instance, th: &Thread, left: Slot, right: Slot) -> Slot {
    let _ = (inst, th);
    // Integer sum unless an operand is an address (tag class 2) or an
    // unwritten cell whose value lands inside a script region.
    let mut addresses = 0;
    let class_r = tag_class(right.tag);
    if class_r == 2 {
        addresses += 1;
    }
    let class_l = tag_class(left.tag);
    if class_l == 2 || (class_l < 0 && in_script_region(inst, th, left.value)) {
        addresses += 1;
    }
    let sum = left.value.wrapping_add(right.value);
    if addresses == 0 {
        Slot {
            value: sum,
            tag: TAG_INT,
        }
    } else {
        Slot {
            value: sum,
            tag: TAG_ADR,
        }
    }
}

/// FUN_14041c840 != 0: the value lies in the work area or the frame stack.
fn in_script_region(inst: &Instance, th: &Thread, a: u32) -> bool {
    let within = |base: u32, len: u32| a >= base && a - base < len;
    within(inst.work_base(), inst.work_bytes) || within(th.frame_base(inst), inst.stack_bytes)
}

#[cfg(test)]
mod tests;
