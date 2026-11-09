//! Allocation-free accounting of Limine's physical address map, not RAM usage.

pub const USABLE: u64 = 0;
pub const KIND_NAMES: [&str; 9] = [
    "usable",
    "reserved",
    "acpi_reclaimable",
    "acpi_nvs",
    "bad_memory",
    "bootloader_reclaimable",
    "kernel_and_modules",
    "framebuffer",
    "unknown",
];

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Region {
    pub base: u64,
    pub length: u64,
    pub kind: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Totals {
    pub regions: u64,
    pub bytes: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub kinds: [Totals; 9],
    pub all: Totals,
    /// Every region other than USABLE, including reclaimable memory and MMIO.
    pub reserved: Totals,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapError {
    EmptyRegion,
    AddressOverflow,
    CounterOverflow,
}

impl Totals {
    fn adding(self, length: u64) -> Result<Self, MapError> {
        Ok(Self {
            regions: self
                .regions
                .checked_add(1)
                .ok_or(MapError::CounterOverflow)?,
            bytes: self
                .bytes
                .checked_add(length)
                .ok_or(MapError::CounterOverflow)?,
        })
    }
}

impl Summary {
    /// Only kind 0 is immediately usable. Unknown kinds fail closed as reserved.
    /// A failed update leaves the original counters intact.
    pub fn add(&mut self, region: Region) -> Result<(), MapError> {
        if region.length == 0 {
            return Err(MapError::EmptyRegion);
        }
        region
            .base
            .checked_add(region.length)
            .ok_or(MapError::AddressOverflow)?;
        let index = if region.kind < 8 {
            region.kind as usize
        } else {
            8
        };
        let kind = self.kinds[index].adding(region.length)?;
        let all = self.all.adding(region.length)?;
        let reserved = if region.kind == USABLE {
            self.reserved
        } else {
            self.reserved.adding(region.length)?
        };
        self.kinds[index] = kind;
        self.all = all;
        self.reserved = reserved;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_usable_is_available_including_future_types() {
        let mut summary = Summary::default();
        for kind in [0, 1, 2, 3, 4, 5, 6, 7, u64::MAX] {
            summary
                .add(Region {
                    base: 0x1000,
                    length: 4096,
                    kind,
                })
                .unwrap();
        }
        assert_eq!(
            summary.kinds[0],
            Totals {
                regions: 1,
                bytes: 4096
            }
        );
        assert_eq!(
            summary.reserved,
            Totals {
                regions: 8,
                bytes: 32768
            }
        );
        assert_eq!(
            summary.all.bytes,
            summary.kinds[0].bytes + summary.reserved.bytes
        );
        assert_eq!(summary.kinds[8].regions, 1);
    }

    #[test]
    fn rejects_wrapping_address_without_mutating_summary() {
        let mut summary = Summary::default();
        assert_eq!(
            summary.add(Region {
                base: u64::MAX,
                length: 1,
                kind: 0
            }),
            Err(MapError::AddressOverflow)
        );
        assert_eq!(summary, Summary::default());
    }

    #[test]
    fn rejects_empty_regions() {
        assert_eq!(
            Summary::default().add(Region {
                base: 0,
                length: 0,
                kind: 0
            }),
            Err(MapError::EmptyRegion)
        );
    }

    #[test]
    fn counter_overflow_is_transactional() {
        let mut summary = Summary::default();
        summary
            .add(Region {
                base: 0,
                length: u64::MAX,
                kind: 1,
            })
            .unwrap();
        let before = summary;
        assert_eq!(
            summary.add(Region {
                base: 0,
                length: 1,
                kind: 0
            }),
            Err(MapError::CounterOverflow)
        );
        assert_eq!(summary, before);
    }

    #[test]
    fn preserves_unaligned_byte_counts_without_rounding_into_free_frames() {
        let mut summary = Summary::default();
        summary
            .add(Region {
                base: 17,
                length: 5001,
                kind: 0,
            })
            .unwrap();
        assert_eq!(summary.kinds[0].bytes, 5001);
    }
}
