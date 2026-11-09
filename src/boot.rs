//! Minimal ABI subset of Limine v8.7.0, base revision 3 (see docs/DECISIONS.md).
//! The bootloader is trusted. Its responses remain mapped and are never reclaimed
//! in M1/M2. No APs or interrupts run, and requests are only read after handoff.

use core::ptr;
use shared_first_os::memory::Region;

#[repr(C)]
struct Request<T> {
    id: [u64; 4],
    revision: u64,
    response: *const T,
}

impl<T> Request<T> {
    const fn new(id: [u64; 2]) -> Self {
        Self {
            id: [0xc7b1dd30df4c8b88, 0x0a82e883a194f07b, id[0], id[1]],
            revision: 0,
            response: ptr::null(),
        }
    }
}

#[repr(C)]
struct MemoryMapResponse {
    revision: u64,
    entry_count: u64,
    entries: *const *const Region,
}

#[repr(C)]
struct FirmwareResponse {
    revision: u64,
    kind: u64,
}

#[repr(C)]
struct HhdmResponse {
    revision: u64,
    offset: u64,
}

#[repr(C)]
struct KernelResponse {
    revision: u64,
    physical: u64,
    virtual_address: u64,
}

// Limine revision-0 module/file ABI prefixes; later fields are not accessed.
#[repr(C)]
struct ModuleResponse {
    revision: u64,
    count: u64,
    files: *const *const ModuleFile,
}
#[repr(C)]
struct ModuleFile {
    revision: u64,
    address: *const u8,
    size: u64,
}
#[used]
#[unsafe(link_section = ".limine_requests")]
static mut MODULES: Request<ModuleResponse> =
    Request::new([0x3e7e279702be32af, 0xca1c4f3bd1280cee]);

pub struct Module {
    pub bytes: &'static [u8],
    pub physical: u64,
    pub retained_pages: u64,
}

/// Transport metadata is trusted Limine data. Contents are untrusted bytecode.
/// Retain the original module pages and all response structures for this boot.
pub fn bytecode_module() -> Result<Module, &'static str> {
    // SAFETY: sole CPU, Limine completed handoff; response storage is retained.
    let response = unsafe { ptr::read_volatile(ptr::addr_of!(MODULES.response)) };
    if response.is_null() {
        return Err("missing");
    }
    // SAFETY: non-null trusted Limine response, mapped by M2 as kind 5.
    let response = unsafe { &*response };
    if response.count != 1 || response.files.is_null() {
        return Err("count");
    }
    // SAFETY: exactly one pointer in the retained Limine array.
    let file = unsafe { response.files.read() };
    if file.is_null() {
        return Err("file");
    }
    // SAFETY: Limine file record contains at least this ABI prefix.
    let file = unsafe { &*file };
    if file.address.is_null()
        || file.size == 0
        || file.size > shared_first_os::loader::MAX_IMAGE as u64
    {
        return Err("size");
    }
    let physical = (file.address as u64)
        .checked_sub(address_info().0)
        .ok_or("address")?;
    let end = physical.checked_add(file.size).ok_or("address")?;
    let map = MemoryMap::get()?;
    let mut retained = false;
    for index in 0..map.len() {
        let region = map.region(index)?;
        if region.kind == 6
            && region.base <= physical
            && region
                .base
                .checked_add(region.length)
                .is_some_and(|limit| end <= limit)
        {
            retained = true;
        }
    }
    if !retained || physical == 0 {
        return Err("region");
    }
    // SAFETY: trusted file pointer, size bounded before slice creation, entire
    // range verified in retained kind-6 memory mapped in HHDM by M2. No code
    // writes these pages, no AP/DMA execution target, and no reclamation occurs.
    let bytes = unsafe { core::slice::from_raw_parts(file.address, file.size as usize) };
    Ok(Module {
        bytes,
        physical,
        retained_pages: ((physical % 4096) + file.size).div_ceil(4096),
    })
}

#[used]
#[unsafe(link_section = ".limine_requests")]
static mut HHDM: Request<HhdmResponse> = Request::new([0x48dcf1cb8ad2b852, 0x63984e959a98244b]);

#[used]
#[unsafe(link_section = ".limine_requests")]
static mut KERNEL: Request<KernelResponse> = Request::new([0x71ba76863cc55f63, 0xb2644a48c516a487]);

pub fn address_info() -> (u64, u64, u64) {
    // SAFETY: Limine has initialized these retained request structures before
    // handoff. Their non-null responses remain mapped and are never reclaimed.
    unsafe {
        let hhdm = ptr::read_volatile(ptr::addr_of!(HHDM.response));
        let kernel = ptr::read_volatile(ptr::addr_of!(KERNEL.response));
        assert!(
            !hhdm.is_null() && !kernel.is_null(),
            "missing address information"
        );
        (
            (*hhdm).offset,
            (*kernel).physical,
            (*kernel).virtual_address,
        )
    }
}

#[used]
#[unsafe(link_section = ".limine_requests_start")]
static START: [u64; 4] = [
    0xf6b8f4b39de7d1ae,
    0xfab91a6940fcb9cf,
    0x785c6ed015d3e316,
    0x181e920a7852b9d9,
];

#[used]
#[unsafe(link_section = ".limine_requests")]
static mut BASE_REVISION: [u64; 3] = [0xf9562b2d5c95a6c8, 0x6a7b384944536bdc, 3];

#[used]
#[unsafe(link_section = ".limine_requests")]
static mut MEMORY_MAP: Request<MemoryMapResponse> =
    Request::new([0x67cf3d9d378a806f, 0xe304acdfc50c3c62]);

#[used]
#[unsafe(link_section = ".limine_requests")]
static mut FIRMWARE: Request<FirmwareResponse> =
    Request::new([0x8c2f75d90bef28a8, 0x7045a4688eac00c3]);

#[used]
#[unsafe(link_section = ".limine_requests_end")]
static END: [u64; 2] = [0xadc0e0531bb10d03, 0x9572709f31764c62];

pub fn check_revision() -> Result<(), &'static str> {
    // SAFETY: the writable, aligned tag belongs to this image. Limine has stopped
    // modifying it before entry. Raw volatile access avoids static-mut references.
    let revision = unsafe { ptr::read_volatile(ptr::addr_of!(BASE_REVISION).cast::<u64>().add(2)) };
    if revision != 0 {
        return Err("unsupported Limine base revision");
    }
    Ok(())
}

pub fn firmware_kind() -> Result<u64, &'static str> {
    // SAFETY: request storage is in a retained writable ELF segment, only updated
    // by the bootloader before handoff. No concurrent writes can occur in M1.
    let response = unsafe { ptr::read_volatile(ptr::addr_of!(FIRMWARE.response)) };
    if response.is_null() {
        return Err("missing firmware response");
    }
    // SAFETY: a non-null response is aligned, initialized and mapped by trusted
    // Limine. Bootloader-reclaimable memory is retained for the whole M1 lifetime.
    Ok(unsafe { (*response).kind })
}

pub struct MemoryMap {
    entries: *const *const Region,
    count: usize,
}

impl MemoryMap {
    pub fn get() -> Result<Self, &'static str> {
        // SAFETY: same request-storage and handoff invariant as firmware_kind.
        let response = unsafe { ptr::read_volatile(ptr::addr_of!(MEMORY_MAP.response)) };
        if response.is_null() {
            return Err("missing memory-map response");
        }
        // SAFETY: trusted Limine supplies a valid response; M1 never frees it.
        let response = unsafe { &*response };
        if response.entry_count == 0 || response.entry_count > 4096 || response.entries.is_null() {
            return Err("invalid memory-map entry array");
        }
        Ok(Self {
            entries: response.entries,
            count: response.entry_count as usize,
        })
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn region(&self, index: usize) -> Result<Region, &'static str> {
        if index >= self.count {
            return Err("memory-map index out of bounds");
        }
        // SAFETY: index is within the array length supplied by trusted Limine;
        // the entire array is mapped and remains alive throughout M1.
        let entry = unsafe { ptr::read(self.entries.add(index)) };
        if entry.is_null() {
            return Err("null memory-map entry");
        }
        // SAFETY: Limine's non-null entries point to initialized repr(C) records.
        // Copy the record; do not expose mutable aliases to bootloader storage.
        Ok(unsafe { ptr::read(entry) })
    }
}
