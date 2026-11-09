//! M7's restricted interpreter, not WebAssembly or a native-code sandbox.
//! No unsafe, pointer instructions, object writes, I/O or dynamic allocation.
//! Program bytes stay immutably borrowed for the lifetime of every Context.
const WIDTH: usize = 16;
pub const MAX_BYTES: u64 = 256 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    Format,
    Opcode,
    Register,
    Reserved,
    Jump,
    Bounds,
    Authority,
    Fuel,
    Stopped,
}
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Yield(u64),
    Halt(u64),
}

/// Trusted adapter owns the calling task identity; bytecode cannot supply it.
/// A call is synchronous and bounded by MAX_BYTES. No host reentry is allowed.
pub trait Host {
    fn sum(&mut self, handle: u64, offset: u64, length: u64) -> Result<u64, Fault>;
}

pub const fn instruction(op: u8, dst: u8, src: u8, aux: u8, immediate: u64) -> [u8; WIDTH] {
    let bytes = immediate.to_le_bytes();
    [
        op, dst, src, aux, bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6],
        bytes[7], 0, 0, 0, 0,
    ]
}

#[derive(Clone, Copy, Debug)]
pub struct Program<'a> {
    code: &'a [u8],
}
impl<'a> Program<'a> {
    pub fn validate(code: &'a [u8]) -> Result<Self, Fault> {
        if code.is_empty() || !code.len().is_multiple_of(WIDTH) || code.len() > WIDTH * 16 {
            return Err(Fault::Format);
        }
        for word in code.chunks_exact(WIDTH) {
            let immediate = u64::from_le_bytes(word[4..12].try_into().unwrap());
            if word[12..].iter().any(|&byte| byte != 0) {
                return Err(Fault::Reserved);
            }
            if word[1..4].iter().any(|&register| register >= 4) {
                return Err(Fault::Register);
            }
            let canonical = match word[0] {
                0 | 7 => word[1..12].iter().all(|&byte| byte == 0),
                1 => word[2] == 0 && word[3] == 0,
                2 => {
                    if immediate > MAX_BYTES {
                        return Err(Fault::Bounds);
                    }
                    true
                }
                3..=5 => word[3] == 0 && immediate == 0,
                6 => {
                    if immediate >= (code.len() / WIDTH) as u64 {
                        return Err(Fault::Jump);
                    }
                    word[1..4].iter().all(|&byte| byte == 0)
                }
                _ => return Err(Fault::Opcode),
            };
            if !canonical {
                return Err(Fault::Reserved);
            }
        }
        Ok(Self { code })
    }
}

pub struct Context<'a> {
    program: Program<'a>,
    pc: usize,
    registers: [u64; 4],
    scratch: [u8; 64],
    fuel: u64,
    stopped: bool,
}
impl<'a> Context<'a> {
    pub fn new(program: Program<'a>, fuel: u64) -> Self {
        Self {
            program,
            pc: 0,
            registers: [0; 4],
            scratch: [0; 64],
            fuel,
            stopped: false,
        }
    }
    pub fn remaining(&self) -> u64 {
        self.fuel
    }
    pub fn run(&mut self, host: &mut impl Host) -> Result<Outcome, Fault> {
        if self.stopped {
            return Err(Fault::Stopped);
        }
        loop {
            match self.step(host) {
                Ok(Some(outcome)) => return Ok(outcome),
                Ok(None) => (),
                Err(error) => {
                    self.stopped = true;
                    return Err(error);
                }
            }
        }
    }
    fn step(&mut self, host: &mut impl Host) -> Result<Option<Outcome>, Fault> {
        let word = self
            .program
            .code
            .get(self.pc * WIDTH..(self.pc + 1) * WIDTH)
            .ok_or(Fault::Bounds)?;
        let dst = word[1] as usize;
        let src = word[2] as usize;
        let aux = word[3] as usize;
        let immediate = u64::from_le_bytes(word[4..12].try_into().unwrap());
        let cost = if word[0] == 2 { 1 + immediate } else { 1 };
        self.fuel = self.fuel.checked_sub(cost).ok_or(Fault::Fuel)?;
        self.pc += 1;
        match word[0] {
            0 => {
                self.stopped = true;
                return Ok(Some(Outcome::Halt(self.registers[0])));
            }
            1 => self.registers[dst] = immediate,
            2 => {
                self.registers[dst] =
                    host.sum(self.registers[src], self.registers[aux], immediate)?
            }
            3 => {
                self.registers[dst] = u64::from(
                    *self
                        .scratch
                        .get(usize::try_from(self.registers[src]).map_err(|_| Fault::Bounds)?)
                        .ok_or(Fault::Bounds)?,
                )
            }
            4 => {
                *self
                    .scratch
                    .get_mut(usize::try_from(self.registers[src]).map_err(|_| Fault::Bounds)?)
                    .ok_or(Fault::Bounds)? = self.registers[dst] as u8
            }
            5 => self.registers[dst] = self.registers[dst].wrapping_add(self.registers[src]),
            6 => self.pc = immediate as usize,
            7 => return Ok(Some(Outcome::Yield(self.registers[0]))),
            _ => unreachable!("validated opcode"),
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Deny;
    impl Host for Deny {
        fn sum(&mut self, _: u64, _: u64, _: u64) -> Result<u64, Fault> {
            panic!("unauthorized host call");
        }
    }
    #[test]
    fn decoder_rejects_unknown_opcodes_registers_reserved_and_targets() {
        for opcode in 8..=255 {
            assert!(matches!(
                Program::validate(&instruction(opcode, 0, 0, 0, 0)),
                Err(Fault::Opcode)
            ));
        }
        assert!(matches!(Program::validate(&[]), Err(Fault::Format)));
        assert!(matches!(Program::validate(&[0; 15]), Err(Fault::Format)));
        assert!(matches!(Program::validate(&[0; 272]), Err(Fault::Format)));
        assert!(matches!(
            Program::validate(&instruction(1, 4, 0, 0, 0)),
            Err(Fault::Register)
        ));
        assert!(matches!(
            Program::validate(&instruction(6, 0, 0, 0, 1)),
            Err(Fault::Jump)
        ));
        assert!(matches!(
            Program::validate(&instruction(2, 0, 0, 0, MAX_BYTES + 1)),
            Err(Fault::Bounds)
        ));
        let mut word = instruction(0, 0, 0, 0, 0);
        word[15] = 1;
        assert!(matches!(Program::validate(&word), Err(Fault::Reserved)));
    }
    #[test]
    fn arithmetic_scratch_and_halt_are_defined() {
        let code = [
            instruction(1, 0, 0, 0, u64::MAX),
            instruction(1, 1, 0, 0, 1),
            instruction(5, 0, 1, 0, 0),
            instruction(1, 2, 0, 0, 63),
            instruction(4, 1, 2, 0, 0),
            instruction(3, 0, 2, 0, 0),
            instruction(0, 0, 0, 0, 0),
        ]
        .concat_for_test();
        let mut vm = Context::new(Program::validate(&code).unwrap(), 7);
        assert_eq!(vm.run(&mut Deny), Ok(Outcome::Halt(1)));
        assert_eq!(vm.remaining(), 0);
        assert_eq!(vm.run(&mut Deny), Err(Fault::Stopped));
    }
    #[test]
    fn fuel_stops_loops_bulk_calls_and_cannot_reset_at_yield() {
        let loop_code = instruction(6, 0, 0, 0, 0);
        let mut vm = Context::new(Program::validate(&loop_code).unwrap(), 100);
        assert_eq!(vm.run(&mut Deny), Err(Fault::Fuel));
        assert_eq!(vm.run(&mut Deny), Err(Fault::Stopped));
        let bulk = instruction(2, 0, 1, 2, 4096);
        assert_eq!(
            Context::new(Program::validate(&bulk).unwrap(), 4096).run(&mut Deny),
            Err(Fault::Fuel)
        );
        let code = [instruction(7, 0, 0, 0, 0), instruction(6, 0, 0, 0, 0)].concat_for_test();
        let mut vm = Context::new(Program::validate(&code).unwrap(), 2);
        assert_eq!(vm.run(&mut Deny), Ok(Outcome::Yield(0)));
        assert_eq!(vm.run(&mut Deny), Err(Fault::Fuel));
    }
    #[test]
    fn scratch_oob_and_fallthrough_fail_closed() {
        for offset in [64, u64::MAX] {
            for opcode in [3, 4] {
                let code = [
                    instruction(1, 1, 0, 0, offset),
                    instruction(opcode, 0, 1, 0, 0),
                ]
                .concat_for_test();
                let mut vm = Context::new(Program::validate(&code).unwrap(), 10);
                assert_eq!(vm.run(&mut Deny), Err(Fault::Bounds));
                assert_eq!(vm.run(&mut Deny), Err(Fault::Stopped));
            }
        }
        let code = instruction(1, 0, 0, 0, 1);
        assert_eq!(
            Context::new(Program::validate(&code).unwrap(), 10).run(&mut Deny),
            Err(Fault::Bounds)
        );
    }
    #[test]
    fn scratch_is_private_between_contexts() {
        let code = [
            instruction(3, 0, 1, 0, 0),
            instruction(7, 0, 0, 0, 0),
            instruction(1, 0, 0, 0, 99),
            instruction(4, 0, 1, 0, 0),
            instruction(0, 0, 0, 0, 0),
        ]
        .concat_for_test();
        let program = Program::validate(&code).unwrap();
        let mut first = Context::new(program, 10);
        let mut second = Context::new(program, 10);
        assert_eq!(first.run(&mut Deny), Ok(Outcome::Yield(0)));
        assert_eq!(first.run(&mut Deny), Ok(Outcome::Halt(99)));
        assert_eq!(second.run(&mut Deny), Ok(Outcome::Yield(0)));
    }
    extern crate std;
    trait Flatten {
        fn concat_for_test(&self) -> std::vec::Vec<u8>;
    }
    impl<const N: usize> Flatten for [[u8; 16]; N] {
        fn concat_for_test(&self) -> std::vec::Vec<u8> {
            self.iter().flatten().copied().collect()
        }
    }
}
