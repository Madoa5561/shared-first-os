//! Metadata-only 4 KiB frame allocator. Callers must unmap and end all accesses
//! before release; this is not a user-facing ownership or capability API.
use crate::memory::{Region, USABLE};

pub const PAGE_SIZE: u64 = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameError {
    InvalidStorage,
    InvalidRegion,
    CapacityExceeded,
    Unaligned,
    Reserved,
    NotAllocated,
}

pub struct FrameAllocator<'a> {
    eligible: &'a mut [u64],
    used: &'a mut [u64],
    cursor: usize,
    total: usize,
    allocated: usize,
    peak: usize,
}

impl<'a> FrameAllocator<'a> {
    pub fn new(
        eligible: &'a mut [u64],
        used: &'a mut [u64],
        regions: &[Region],
    ) -> Result<Self, FrameError> {
        if eligible.is_empty() || eligible.len() != used.len() {
            return Err(FrameError::InvalidStorage);
        }
        let capacity = eligible
            .len()
            .checked_mul(64)
            .and_then(|n| (n as u64).checked_mul(PAGE_SIZE))
            .ok_or(FrameError::InvalidStorage)?;
        for region in regions {
            let end = region
                .base
                .checked_add(region.length)
                .filter(|_| region.length != 0)
                .ok_or(FrameError::InvalidRegion)?;
            if region.kind == USABLE && end > capacity {
                return Err(FrameError::CapacityExceeded);
            }
        }
        eligible.fill(0);
        used.fill(0);
        // Inward alignment for usable ranges. Overlapping usable ranges are idempotent.
        for region in regions.iter().filter(|region| region.kind == USABLE) {
            let first = region.base.div_ceil(PAGE_SIZE);
            let end = (region.base + region.length) / PAGE_SIZE;
            for frame in first..end {
                eligible[frame as usize / 64] |= 1 << (frame % 64);
            }
        }
        // Reserved wins over usable regardless of descriptor order. Reserve any
        // partially touched page, including unknown and reclaimable kinds.
        for region in regions.iter().filter(|region| region.kind != USABLE) {
            let first = region.base.min(capacity) / PAGE_SIZE;
            let end = (region.base + region.length)
                .min(capacity)
                .div_ceil(PAGE_SIZE);
            for frame in first..end {
                eligible[frame as usize / 64] &= !(1 << (frame % 64));
            }
        }
        eligible[0] &= !1; // Physical null is never an allocation.
        let total = eligible.iter().map(|word| word.count_ones() as usize).sum();
        Ok(Self {
            eligible,
            used,
            cursor: 0,
            total,
            allocated: 0,
            peak: 0,
        })
    }

    pub fn allocate(&mut self) -> Option<u64> {
        while self.cursor < self.eligible.len() {
            let free = self.eligible[self.cursor] & !self.used[self.cursor];
            if free != 0 {
                let bit = free.trailing_zeros() as usize;
                self.used[self.cursor] |= 1 << bit;
                self.allocated += 1;
                self.peak = self.peak.max(self.allocated);
                return Some((self.cursor * 64 + bit) as u64 * PAGE_SIZE);
            }
            self.cursor += 1;
        }
        None
    }

    pub fn release(&mut self, address: u64) -> Result<(), FrameError> {
        if !address.is_multiple_of(PAGE_SIZE) {
            return Err(FrameError::Unaligned);
        }
        if !self.is_eligible(address) {
            return Err(FrameError::Reserved);
        }
        if !self.is_allocated(address) {
            return Err(FrameError::NotAllocated);
        }
        let frame = (address / PAGE_SIZE) as usize;
        self.used[frame / 64] &= !(1 << (frame % 64));
        self.allocated -= 1;
        self.cursor = self.cursor.min(frame / 64);
        Ok(())
    }

    pub fn is_eligible(&self, address: u64) -> bool {
        let frame = address / PAGE_SIZE;
        address.is_multiple_of(PAGE_SIZE)
            && frame < self.eligible.len() as u64 * 64
            && self.eligible[frame as usize / 64] & (1 << (frame % 64)) != 0
    }

    pub fn is_allocated(&self, address: u64) -> bool {
        self.is_eligible(address)
            && self.used[(address / PAGE_SIZE) as usize / 64] & (1 << ((address / PAGE_SIZE) % 64))
                != 0
    }

    pub fn total(&self) -> usize {
        self.total
    }
    pub fn allocated(&self) -> usize {
        self.allocated
    }
    /// Begin a measurement epoch without changing allocation ownership.
    pub fn reset_peak(&mut self) {
        self.peak = self.allocated;
    }
    pub fn peak(&self) -> usize {
        self.peak
    }
    pub fn free(&self) -> usize {
        self.total - self.allocated
    }
    pub fn metadata_bytes(&self) -> usize {
        core::mem::size_of_val(self.eligible) + core::mem::size_of_val(self.used)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn region(base: u64, length: u64, kind: u64) -> Region {
        Region { base, length, kind }
    }

    #[test]
    fn peak_records_transient_allocations_and_resets_to_live_count() {
        let mut eligible = [0; 1];
        let mut used = [0; 1];
        let mut frames =
            FrameAllocator::new(&mut eligible, &mut used, &[region(4096, 8192, 0)]).unwrap();
        let first = frames.allocate().unwrap();
        let second = frames.allocate().unwrap();
        assert_eq!(frames.allocate(), None);
        frames.release(second).unwrap();
        assert_eq!(frames.peak(), 2);
        frames.reset_peak();
        assert_eq!(frames.peak(), 1);
        frames.release(first).unwrap();
        assert_eq!(frames.peak(), 1);
        frames.reset_peak();
        assert_eq!(frames.peak(), 0);
    }

    #[test]
    fn reservations_win_in_any_order_and_reclaimable_is_not_free() {
        let regions = [
            region(0x4101, 1, 99),
            region(0, 0x10000, 0),
            region(0x8000, 4096, 5),
            region(0x9000, 4096, 6),
            region(0xa001, 4096, 2),
        ];
        for reversed in [false, true] {
            let mut map = regions;
            if reversed {
                map.reverse();
            }
            let mut eligible = [0; 1];
            let mut used = [0; 1];
            let mut allocator = FrameAllocator::new(&mut eligible, &mut used, &map).unwrap();
            let mut seen = [false; 16];
            while let Some(address) = allocator.allocate() {
                let index = address as usize / 4096;
                assert!(!seen[index]);
                seen[index] = true;
                assert!(![0, 4, 8, 9, 10, 11].contains(&index));
            }
            assert_eq!(allocator.allocated(), 10);
            assert_eq!(allocator.free(), 0);
        }
    }

    #[test]
    fn inward_alignment_and_duplicate_usable_ranges() {
        let mut eligible = [0; 1];
        let mut used = [0; 1];
        let mut allocator = FrameAllocator::new(
            &mut eligible,
            &mut used,
            &[region(4097, 12286, 0), region(4097, 12286, 0)],
        )
        .unwrap();
        assert_eq!(allocator.total(), 1);
        assert_eq!(allocator.allocate(), Some(8192));
        assert_eq!(allocator.allocate(), None);
    }

    #[test]
    fn exhaustion_reuse_and_invalid_free() {
        let mut eligible = [0; 1];
        let mut used = [0; 1];
        let mut allocator =
            FrameAllocator::new(&mut eligible, &mut used, &[region(4096, 8192, 0)]).unwrap();
        assert_eq!(allocator.release(4096), Err(FrameError::NotAllocated));
        assert_eq!(allocator.release(0), Err(FrameError::Reserved));
        assert_eq!(allocator.release(4097), Err(FrameError::Unaligned));
        let first = allocator.allocate().unwrap();
        let second = allocator.allocate().unwrap();
        assert_ne!(first, second);
        assert_eq!(allocator.allocate(), None);
        allocator.release(first).unwrap();
        assert_eq!(allocator.release(first), Err(FrameError::NotAllocated));
        assert_eq!(allocator.allocate(), Some(first));
        assert_eq!(allocator.free() + allocator.allocated(), allocator.total());
    }

    #[test]
    fn rejects_malformed_maps_and_capacity_overflow() {
        for (map, expected) in [
            (region(u64::MAX, 1, 0), FrameError::InvalidRegion),
            (region(0, 0, 1), FrameError::InvalidRegion),
            (region(0, 65 * 4096, 0), FrameError::CapacityExceeded),
        ] {
            assert!(
                matches!(FrameAllocator::new(&mut [0], &mut [0], &[map]), Err(error) if error == expected)
            );
        }
    }

    #[test]
    fn deterministic_churn_matches_independent_ownership_model() {
        let mut eligible = [0; 4];
        let mut used = [0; 4];
        let mut allocator =
            FrameAllocator::new(&mut eligible, &mut used, &[region(4096, 255 * 4096, 0)]).unwrap();
        let mut owned = [false; 256];
        let mut seed = 0x12345678_u64;
        for _ in 0..10000 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let index = ((seed >> 32) % 255 + 1) as usize;
            if seed & 1 == 0 && owned[index] {
                allocator.release(index as u64 * 4096).unwrap();
                owned[index] = false;
            } else if let Some(address) = allocator.allocate() {
                let index = (address / 4096) as usize;
                assert!(!owned[index]);
                owned[index] = true;
            }
            assert_eq!(
                allocator.allocated(),
                owned.iter().filter(|&&value| value).count()
            );
        }
    }
}
