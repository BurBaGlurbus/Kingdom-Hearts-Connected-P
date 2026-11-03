//! Synthetic programs only: every byte is authored here.
use super::*;

/// Minimal assembler for the instruction encodings the interpreter decodes.
struct Asm {
    words: Vec<u16>,
}

const CODE_START: usize = 14;

impl Asm {
    fn new() -> Self {
        Self { words: Vec::new() }
    }
    fn pc(&self) -> usize {
        CODE_START + self.words.len()
    }
    fn op(&mut self, op: u16) -> &mut Self {
        self.words.push(op);
        self
    }
    fn op1(&mut self, op: u16, imm: u16) -> &mut Self {
        self.words.extend([op, imm]);
        self
    }
    fn int(&mut self, v: i32) -> &mut Self {
        self.words.extend([0x0000, v as u16, (v >> 16) as u16]);
        self
    }
    fn float(&mut self, v: f32) -> &mut Self {
        let b = v.to_bits();
        self.words.extend([0x0010, b as u16, (b >> 16) as u16]);
        self
    }
    fn syscall(&mut self, table: u16, index: u16) -> &mut Self {
        self.op1(0x000a | table << 6, index)
    }
    fn build(&self, work: u32, stack: u32, temp: u32) -> Vec<u8> {
        let mut b = vec![0u8; 16];
        b[..4].copy_from_slice(b"test");
        for v in [work, stack, temp] {
            b.extend(v.to_le_bytes());
        }
        // Trigger 0 at the first code word, then the terminator.
        for v in [0u32, CODE_START as u32, 0, 0] {
            b.extend(v.to_le_bytes());
        }
        assert_eq!((b.len() - 16) / 2, CODE_START);
        for w in &self.words {
            b.extend(w.to_le_bytes());
        }
        b
    }
}

struct TestHost {
    seen: Vec<(u16, u16, Vec<Slot>)>,
    frame_time: f32,
}

impl TestHost {
    fn new() -> Self {
        Self {
            seen: Vec::new(),
            frame_time: 1.0,
        }
    }
}

impl Host for TestHost {
    fn frame_time(&self) -> f32 {
        self.frame_time
    }
    fn trap_flags(&self, table: u16, index: u16) -> Option<u32> {
        match (table, index) {
            (1, 1) => Some(0x4000_0002), // two in, one out
            (1, 2) => Some(0x0000_0001), // one in
            (1, 3) => Some(0x4000_0001), // string length
            _ => None,
        }
    }
    fn trap(
        &mut self,
        memory: &mut Memory<'_>,
        table: u16,
        index: u16,
        args: &mut [Slot; 16],
    ) -> Result<(), Fault> {
        let count = (self.trap_flags(table, index).unwrap() & 0xffff) as usize;
        self.seen.push((table, index, args[..count].to_vec()));
        if (table, index) == (1, 1) {
            args[0] = Slot::int(args[0].as_int() * 10 + args[1].as_int());
        }
        if (table, index) == (1, 3) {
            let s = memory
                .string(args[0].value, 64)
                .ok_or(Fault::Memory(args[0].value))?;
            args[0] = Slot::int(s.len() as i32);
        }
        Ok(())
    }
}

fn start(asm: &Asm) -> (Instance, Thread) {
    let inst = Instance::new(0, &asm.build(64, 256, 128)).unwrap();
    let entry = inst.trigger(0);
    let th = Thread::new(&inst, 0, entry, &[]);
    (inst, th)
}

fn go(asm: &Asm, host: &mut TestHost) -> (Exit, Thread, Instance) {
    let (mut inst, mut th) = start(asm);
    let exit = run(&mut inst, &mut th, host, 100_000);
    (exit, th, inst)
}

#[test]
fn integer_and_float_arithmetic_keep_operand_order_and_tags() {
    let mut a = Asm::new();
    a.int(7)
        .int(5)
        .op(0x0046) // sub: 7 - 5
        .float(1.5)
        .float(4.0)
        .op(0x0056) // sub.s: 1.5 - 4.0
        .int(-9)
        .int(2)
        .op(0x00c6) // div: -9 / 2 truncates toward zero
        .int(-9)
        .int(2)
        .op(0x0106) // mod: -9 % 2
        .op(0x0049);
    let (exit, th, _) = go(&a, &mut TestHost::new());
    assert_eq!(exit, Exit::Exit);
    assert_eq!(
        th.stack(),
        [
            Slot::int(2),
            Slot::float(-2.5),
            Slot::int(-4),
            Slot::int(-1)
        ]
    );
}

#[test]
fn division_by_zero_keeps_the_left_operand() {
    let mut a = Asm::new();
    a.int(9)
        .int(0)
        .op(0x00c6)
        .float(2.0)
        .float(0.0)
        .op(0x00d6)
        .op(0x0049);
    let (_, th, _) = go(&a, &mut TestHost::new());
    assert_eq!(th.stack(), [Slot::int(9), Slot::float(2.0)]);
}

#[test]
fn comparison_and_conversion_operators() {
    let mut a = Asm::new();
    a.int(-3)
        .op(0x0185) // sltz
        .float(0.0)
        .op(0x0215) // seqz.s
        .float(-2.75)
        .op(0x0055) // float to int, truncating
        .int(3)
        .op(0x0005) // int to float
        .int(6)
        .int(0)
        .op(0x02c6) // lor
        .op(0x0049);
    let (_, th, _) = go(&a, &mut TestHost::new());
    assert_eq!(th.stack()[0], Slot::int(1));
    assert_eq!(th.stack()[1], Slot::int(1));
    assert_eq!(th.stack()[2], Slot::int(-2));
    assert_eq!(th.stack()[3], Slot::float(3.0));
    assert_eq!(th.stack()[4], Slot::int(1));
}

#[test]
fn calls_share_the_frame_stack_and_return() {
    // main: push 40; jal f; exit.  f: pop local0; push local0 + 2; ret.
    let mut a = Asm::new();
    a.int(40);
    let call_at = a.pc();
    a.op1(0x0008 | 1 << 6, 0); // one word of frame; target patched below
    a.op(0x0049);
    let f = a.pc();
    a.op1(0x0001, 0) // pop.sp 0
        .op1(0x0030, 0) // push.d.sp 0
        .int(2)
        .op(0x0006)
        .op(0x0089);
    a.words[call_at - CODE_START + 1] = (f - (call_at + 2)) as u16;
    let (exit, th, _) = go(&a, &mut TestHost::new());
    assert_eq!(exit, Exit::Exit);
    assert_eq!(th.stack(), [Slot::int(42)]);
}

#[test]
fn returning_from_the_outermost_frame_ends_the_thread() {
    let mut a = Asm::new();
    a.op(0x0089);
    let (exit, _, _) = go(&a, &mut TestHost::new());
    assert_eq!(exit, Exit::End);
}

#[test]
fn halt_resumes_after_the_instruction() {
    let mut a = Asm::new();
    a.int(1).op(0x0009).int(2).op(0x0049);
    let (mut inst, mut th) = start(&a);
    let mut host = TestHost::new();
    assert_eq!(run(&mut inst, &mut th, &mut host, 100), Exit::Halt);
    assert_eq!(th.stack(), [Slot::int(1)]);
    assert_eq!(run(&mut inst, &mut th, &mut host, 100), Exit::Exit);
    assert_eq!(th.stack(), [Slot::int(1), Slot::int(2)]);
}

#[test]
fn conditional_branches_are_relative_to_the_following_instruction() {
    let mut a = Asm::new();
    a.int(0);
    a.op1(0x0047, 3); // beqz +3: skips the three-word push 5
    a.int(5);
    a.int(8);
    a.op(0x0049);
    let (_, th, _) = go(&a, &mut TestHost::new());
    assert_eq!(th.stack(), [Slot::int(8)]);
}

#[test]
fn traps_receive_arguments_in_push_order_and_push_their_result() {
    let mut a = Asm::new();
    a.int(4)
        .int(2)
        .syscall(1, 1)
        .syscall(1, 2)
        .int(7)
        .op(0x0049);
    let mut host = TestHost::new();
    let (_, th, _) = go(&a, &mut host);
    assert_eq!(host.seen[0], (1, 1, vec![Slot::int(4), Slot::int(2)]));
    assert_eq!(host.seen[1], (1, 2, vec![Slot::int(42)]));
    assert_eq!(th.stack(), [Slot::int(7)]);
}

#[test]
fn unknown_trap_is_a_fault() {
    let mut a = Asm::new();
    a.syscall(3, 9);
    let (exit, _, _) = go(&a, &mut TestHost::new());
    assert_eq!(exit, Exit::Fault(Fault::Trap { table: 3, index: 9 }));
}

#[test]
fn work_memory_keeps_the_tag_of_what_was_stored() {
    let mut a = Asm::new();
    a.float(2.5)
        .op1(0x0041, 8) // pop.wp 8
        .op1(0x0070, 8) // push.d.wp 8
        .op1(0x0070, 12) // push.d.wp 12: never written (the work area starts zeroed)
        .op(0x0049);
    let (_, th, _) = go(&a, &mut TestHost::new());
    assert_eq!(th.stack()[0], Slot::float(2.5));
    assert_eq!(th.stack()[1], Slot { value: 0, tag: 0 });
}

#[test]
fn address_sums_stay_addresses() {
    let mut a = Asm::new();
    a.op1(0x0060, 4) // push.wp 4
        .int(8)
        .op(0x0006)
        .op1(0x0060, 20)
        .int(4)
        .op(0x0046)
        .op(0x0049);
    let (_, th, inst) = go(&a, &mut TestHost::new());
    assert_eq!(th.stack()[0], Slot::address(inst.work_base() + 12));
    // Only add is address-aware: subtraction is an integer operation.
    assert_eq!(th.stack()[1].tag, TAG_INT);
}

#[test]
fn strings_are_reached_through_code_handles() {
    let mut a = Asm::new();
    a.op1(0x00e0, 0).syscall(1, 3).op(0x0049);
    let string_word = a.pc();
    a.words
        .extend([u16::from_le_bytes(*b"he"), u16::from_le_bytes(*b"y\0")]);
    a.words[1] = string_word as u16;
    let (_, th, _) = go(&a, &mut TestHost::new());
    assert_eq!(th.stack(), [Slot::int(3)]);
}

#[test]
fn block_copy_clears_destination_tags() {
    let mut a = Asm::new();
    a.int(77)
        .op1(0x0041, 0) // work[0] = 77 (tagged int)
        .op1(0x0060, 0); // source address
    a.words.extend([0x0002, 4, 0]); // memcpy.sp size 4 to frame + 0
    a.op1(0x0030, 0).op(0x0049);
    let (_, th, _) = go(&a, &mut TestHost::new());
    assert_eq!(
        th.stack()[0],
        Slot {
            value: 77,
            tag: TAG_NONE
        }
    );
}

#[test]
fn unsupported_opcode_class_reports_the_native_error_code() {
    let mut a = Asm::new();
    a.op(0x000c);
    let (exit, _, _) = go(&a, &mut TestHost::new());
    assert_eq!(exit, Exit::BadOpcode);
    assert_eq!(exit.code(), 5);
}

#[test]
fn hd_literal_patch_applies_by_code_offset_and_frame_time() {
    // `push.s 20.0` located at word 0xc06 (byte offset 0x180c) becomes 18.0 at 60 Hz.
    let filler = 3078 - CODE_START - 2;
    let mut a = Asm::new();
    a.op1(0x0007, filler as u16);
    a.words.extend(std::iter::repeat_n(0x0009, filler));
    a.float(20.0).op(0x0049);
    assert_eq!(a.pc() - 4, 3078);
    let (_, th, _) = go(&a, &mut TestHost::new());
    assert_eq!(th.stack(), [Slot::float(18.0)]);
    let mut slow = TestHost::new();
    slow.frame_time = 2.0;
    let (_, th, _) = go(&a, &mut slow);
    assert_eq!(th.stack(), [Slot::float(20.0)]);
}

#[test]
fn sine_wraps_and_cosine_has_its_exact_zero() {
    let mut a = Asm::new();
    a.float(std::f32::consts::FRAC_PI_2)
        .op(0x0189) // sin
        .float(-std::f32::consts::FRAC_PI_2)
        .op(0x01c9) // cos at the special angle
        .float(180.0)
        .op(0x0209) // degrees to radians
        .op(0x0049);
    let (_, th, _) = go(&a, &mut TestHost::new());
    assert_eq!(th.stack()[0], Slot::float(1.0));
    assert_eq!(th.stack()[1], Slot::float(0.0));
    let r = th.stack()[2].as_float();
    assert!((r.abs() - std::f32::consts::PI).abs() < 1e-5, "{r}");
}

#[test]
fn trigger_lookup_and_stack_size_patch() {
    let mut a = Asm::new();
    a.op(0x0049);
    let mut bytes = a.build(64, 0x200, 128);
    bytes[..8].copy_from_slice(b"b_ex370\0");
    let inst = Instance::new(0, &bytes).unwrap();
    assert_eq!(inst.stack_bytes, 0x400);
    assert_eq!(inst.trigger(0), CODE_START as i32);
    assert_eq!(inst.trigger(6), 0);
}
