//! Versioned, bounded bytecode package. Input bytes carry no authority.
use crate::{
    sandbox::{Fault, Program},
    scheduler::{CAPACITY, MAX_BATCH_FUEL},
};

pub const MAGIC: [u8; 8] = *b"SFOSBC09";
pub const MAX_CODE: usize = 256;
pub const MAX_TOTAL_CODE: usize = 512;
pub const MAX_IMAGE: usize = 16 + CAPACITY * 16 + MAX_TOTAL_CODE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadError {
    Size,
    Header,
    Version,
    Count,
    Reserved,
    CodeSize,
    Fuel,
    Truncated,
    Trailing,
    Code(Fault),
}

/// Owns code independently of the transport; programs borrow this immutable copy.
/// clear cannot compile while any scheduler still borrows a program from it.
pub struct Batch {
    code: [[u8; MAX_CODE]; CAPACITY],
    lengths: [usize; CAPACITY],
    fuel: [u64; CAPACITY],
    count: usize,
}
impl Batch {
    pub fn load(image: &[u8]) -> Result<Self, LoadError> {
        if image.len() < 16 || image.len() > MAX_IMAGE {
            return Err(LoadError::Size);
        }
        if image[..8] != MAGIC {
            return Err(LoadError::Header);
        }
        if u16::from_le_bytes(image[8..10].try_into().unwrap()) != 1 {
            return Err(LoadError::Version);
        }
        let count = u16::from_le_bytes(image[10..12].try_into().unwrap()) as usize;
        if count == 0 || count > CAPACITY {
            return Err(LoadError::Count);
        }
        if image[12..16] != [0; 4] {
            return Err(LoadError::Reserved);
        }
        let mut batch = Self {
            code: [[0; MAX_CODE]; CAPACITY],
            lengths: [0; CAPACITY],
            fuel: [0; CAPACITY],
            count,
        };
        let mut cursor = 16;
        let mut total_code = 0;
        let mut total_fuel = 0;
        for index in 0..count {
            let header = image.get(cursor..cursor + 16).ok_or(LoadError::Truncated)?;
            let length = u32::from_le_bytes(header[..4].try_into().unwrap()) as usize;
            let fuel = u32::from_le_bytes(header[4..8].try_into().unwrap()) as u64;
            if header[8..] != [0; 8] {
                return Err(LoadError::Reserved);
            }
            if length == 0
                || length > MAX_CODE
                || !length.is_multiple_of(16)
                || length > MAX_TOTAL_CODE - total_code
            {
                return Err(LoadError::CodeSize);
            }
            if fuel == 0 || fuel > MAX_BATCH_FUEL - total_fuel {
                return Err(LoadError::Fuel);
            }
            cursor += 16;
            // length and cursor are bounded above before addition, even on 32-bit.
            let code = image
                .get(cursor..cursor + length)
                .ok_or(LoadError::Truncated)?;
            Program::validate(code).map_err(LoadError::Code)?;
            batch.code[index][..length].copy_from_slice(code);
            batch.lengths[index] = length;
            batch.fuel[index] = fuel;
            cursor += length;
            total_code += length;
            total_fuel += fuel;
        }
        if cursor != image.len() {
            return Err(LoadError::Trailing);
        }
        Ok(batch)
    }
    pub fn count(&self) -> usize {
        self.count
    }
    pub fn code_bytes(&self) -> usize {
        self.lengths.iter().sum()
    }
    pub fn fuel(&self) -> u64 {
        self.fuel.iter().sum()
    }
    pub fn program(&self, index: usize) -> Option<(Program<'_>, u64)> {
        if index >= self.count {
            return None;
        }
        Some((
            Program::validate(&self.code[index][..self.lengths[index]])
                .expect("owned validated code"),
            self.fuel[index],
        ))
    }
    /// Logical release and buffer clearing, not a cryptographic erasure guarantee.
    pub fn clear(&mut self) {
        self.code.fill([0; MAX_CODE]);
        self.lengths.fill(0);
        self.fuel.fill(0);
        self.count = 0;
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use crate::sandbox::{Context, Host, Outcome, instruction as ins};
    use std::vec::Vec;
    fn package(entries: &[(&[u8], u32)]) -> Vec<u8> {
        let mut bytes = Vec::from(MAGIC);
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        bytes.extend_from_slice(&[0; 4]);
        for (code, fuel) in entries {
            bytes.extend_from_slice(&(code.len() as u32).to_le_bytes());
            bytes.extend_from_slice(&fuel.to_le_bytes());
            bytes.extend_from_slice(&[0; 8]);
            bytes.extend_from_slice(code);
        }
        bytes
    }
    struct Deny;
    impl Host for Deny {
        fn sum(&mut self, _: u64, _: u64, _: u64) -> Result<u64, Fault> {
            Err(Fault::Authority)
        }
    }
    #[test]
    fn input_mutation_does_not_change_owned_code_and_clear_releases_logically() {
        let words = [ins(1, 0, 0, 0, 42), ins(0, 0, 0, 0, 0)];
        let mut input = package(&[(words.as_flattened(), 2)]);
        let mut batch = Batch::load(&input).unwrap();
        input.fill(0xff);
        let (program, fuel) = batch.program(0).unwrap();
        assert_eq!(
            Context::new(program, fuel).run(&mut Deny),
            Ok(Outcome::Halt(42))
        );
        assert_eq!(
            (batch.count(), batch.code_bytes(), batch.fuel()),
            (1, 32, 2)
        );
        assert!(batch.program(usize::MAX).is_none());
        batch.clear();
        assert_eq!((batch.count(), batch.code_bytes(), batch.fuel()), (0, 0, 0));
        assert!(batch.program(0).is_none());
        assert!(batch.code.iter().flatten().all(|byte| *byte == 0));
    }
    #[test]
    fn all_truncations_and_trailing_data_rejected() {
        let code = ins(0, 0, 0, 0, 0);
        let mut image = package(&[(&code, 1), (&code, 1)]);
        for size in 0..image.len() {
            assert!(Batch::load(&image[..size]).is_err(), "size={size}");
        }
        image.push(0);
        assert!(matches!(Batch::load(&image), Err(LoadError::Trailing)));
        assert!(matches!(
            Batch::load(&[0; MAX_IMAGE + 1]),
            Err(LoadError::Size)
        ));
    }
    #[test]
    fn headers_reservations_and_lengths_rejected() {
        let image = package(&[(&ins(0, 0, 0, 0, 0), 1)]);
        for (offset, value, error) in [
            (0, 0, LoadError::Header),
            (8, 2, LoadError::Version),
            (10, 0, LoadError::Count),
            (10, 5, LoadError::Count),
            (12, 1, LoadError::Reserved),
            (24, 1, LoadError::Reserved),
            (16, 0, LoadError::CodeSize),
            (16, 17, LoadError::CodeSize),
            (19, 255, LoadError::CodeSize),
        ] {
            let mut invalid = image.clone();
            invalid[offset] = value;
            assert!(
                matches!(Batch::load(&invalid), Err(actual) if actual == error),
                "offset={offset}"
            );
        }
    }
    #[test]
    fn aggregate_code_and_fuel_limits_are_enforced() {
        let large = [0; 256];
        let medium = [0; 128];
        // 592-byte image fits the transport limit but code exceeds its quota.
        assert!(matches!(
            Batch::load(&package(&[
                (&large, 1),
                (&large, 1),
                (&ins(0, 0, 0, 0, 0), 1)
            ])),
            Err(LoadError::CodeSize)
        ));
        let batch = Batch::load(&package(&[(&medium[..], 1); 4])).unwrap();
        assert_eq!((batch.count(), batch.code_bytes()), (4, MAX_TOTAL_CODE));
        for fuel in [0, u32::MAX] {
            assert!(matches!(
                Batch::load(&package(&[(&large, fuel)])),
                Err(LoadError::Fuel)
            ));
        }
        assert!(Batch::load(&package(&[(&large, MAX_BATCH_FUEL as u32)])).is_ok());
        assert!(matches!(
            Batch::load(&package(&[(&large, MAX_BATCH_FUEL as u32), (&medium, 1)])),
            Err(LoadError::Fuel)
        ));
    }
    #[test]
    fn invalid_last_program_prevents_any_batch_publication() {
        let valid = ins(0, 0, 0, 0, 0);
        for opcode in 8..=255 {
            let invalid = ins(opcode, 0, 0, 0, 0);
            assert!(matches!(
                Batch::load(&package(&[(&valid, 1), (&invalid, 1)])),
                Err(LoadError::Code(Fault::Opcode))
            ));
        }
        let invalid = ins(6, 0, 0, 0, 1);
        assert!(matches!(
            Batch::load(&package(&[(&valid, 1), (&invalid, 1)])),
            Err(LoadError::Code(Fault::Jump))
        ));
    }
}
