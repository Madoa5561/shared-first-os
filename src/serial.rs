use core::{arch::asm, fmt};

const COM1: u16 = 0x3f8;

fn write_register(offset: u16, byte: u8) {
    // SAFETY: offsets in this module address only the QEMU COM1 UART. M1 is
    // single-CPU with interrupts disabled, so register access cannot interleave.
    unsafe {
        asm!("out dx, al", in("dx") COM1 + offset, in("al") byte, options(nomem, nostack));
    }
}

fn read_register(offset: u16) -> u8 {
    let byte;
    // SAFETY: same UART ownership and ring-0 invariant as write_register.
    unsafe {
        asm!("in al, dx", in("dx") COM1 + offset, out("al") byte, options(nomem, nostack));
    }
    byte
}

pub fn init() {
    write_register(1, 0x00); // Interrupts off.
    write_register(3, 0x80); // Divisor latch.
    write_register(0, 0x01); // 115200 baud.
    write_register(1, 0x00);
    write_register(3, 0x03); // 8N1.
    write_register(2, 0xc7); // FIFO enabled and cleared.
    write_register(4, 0x0b);
}

struct Serial;

impl Serial {
    fn byte(&mut self, byte: u8) -> fmt::Result {
        // A missing UART must not hang panic reporting forever.
        for _ in 0..1_000_000 {
            if read_register(5) & 0x20 != 0 {
                write_register(0, byte);
                return Ok(());
            }
            core::hint::spin_loop();
        }
        Err(fmt::Error)
    }
}

impl fmt::Write for Serial {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        for byte in text.bytes() {
            if byte == b'\n' {
                self.byte(b'\r')?;
            }
            self.byte(byte)?;
        }
        Ok(())
    }
}

pub fn print(arguments: fmt::Arguments<'_>) {
    use fmt::Write;
    let _ = Serial.write_fmt(arguments);
}
