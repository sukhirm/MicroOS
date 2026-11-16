//! 8259 Programmable Interrupt Controller (PIC) Driver.
//!
//! Remaps the legacy PIC interrupt vectors to 32..47 (0x20..0x2F) to eliminate
//! hardware conflicts with CPU exception vectors 0..31, and manages IRQ masking and EOI signaling.

use x86_64::instructions::port::Port;

/// Master PIC I/O ports.
const PIC1_COMMAND: u16 = 0x20;
const PIC1_DATA: u16 = 0x21;

/// Slave PIC I/O ports.
const PIC2_COMMAND: u16 = 0xA0;
const PIC2_DATA: u16 = 0xA1;

/// PIC Initialization and EOI command words.
const ICW1_INIT: u8 = 0x11;
const ICW4_8086: u8 = 0x01;
const PIC_EOI: u8 = 0x20;

/// Base interrupt vector for Master PIC (IRQs 0..7 map to vectors 32..39).
pub const PIC1_OFFSET: u8 = 32;

/// Base interrupt vector for Slave PIC (IRQs 8..15 map to vectors 40..47).
pub const PIC2_OFFSET: u8 = 40;

/// Waits briefly on unused port 0x80 to allow slow PIC hardware bus lines to settle.
#[inline(always)]
unsafe fn io_wait() {
    let mut port: Port<u8> = Port::new(0x80);
    port.write(0);
}

/// Initializes and remaps the 8259 Master and Slave PICs.
///
/// # Safety
/// Caller must ensure that the Interrupt Descriptor Table (IDT) is configured
/// with appropriate handlers for vectors 32..47 before interrupts are enabled.
pub unsafe fn init() {
    let mut pic1_cmd: Port<u8> = Port::new(PIC1_COMMAND);
    let mut pic1_data: Port<u8> = Port::new(PIC1_DATA);
    let mut pic2_cmd: Port<u8> = Port::new(PIC2_COMMAND);
    let mut pic2_data: Port<u8> = Port::new(PIC2_DATA);

    // Save existing masks
    let _mask1 = pic1_data.read();
    let _mask2 = pic2_data.read();

    // 1. ICW1: Start initialization sequence in cascade mode
    pic1_cmd.write(ICW1_INIT);
    io_wait();
    pic2_cmd.write(ICW1_INIT);
    io_wait();

    // 2. ICW2: Remap vector offsets to avoid CPU exception collisions (0..31)
    pic1_data.write(PIC1_OFFSET);
    io_wait();
    pic2_data.write(PIC2_OFFSET);
    io_wait();

    // 3. ICW3: Configure cascading (Master has Slave at IRQ2, Slave identity is 2)
    pic1_data.write(0x04);
    io_wait();
    pic2_data.write(0x02);
    io_wait();

    // 4. ICW4: Set 8086/88 mode
    pic1_data.write(ICW4_8086);
    io_wait();
    pic2_data.write(ICW4_8086);
    io_wait();

    // 5. Masks: Unmask IRQ0 (Timer) on Master PIC; mask all other IRQs initially
    // 0xFE = 1111_1110 (bit 0 = 0 -> IRQ0 enabled)
    pic1_data.write(0xFE);
    io_wait();
    // 0xFF = mask all IRQs on Slave PIC
    pic2_data.write(0xFF);
    io_wait();

    log::info!(
        "[PIC] 8259 PIC initialized: Master remapped to {:#x}, Slave to {:#x}. IRQ0 unmasked.",
        PIC1_OFFSET,
        PIC2_OFFSET
    );
}

/// Sends End-of-Interrupt (EOI) acknowledgment to the PIC.
///
/// Must be invoked by interrupt handlers before resuming execution or context switching.
#[inline(always)]
pub unsafe fn send_eoi(irq: u8) {
    if irq >= 8 {
        let mut pic2_cmd: Port<u8> = Port::new(PIC2_COMMAND);
        pic2_cmd.write(PIC_EOI);
    }
    let mut pic1_cmd: Port<u8> = Port::new(PIC1_COMMAND);
    pic1_cmd.write(PIC_EOI);
}

/// Unmasks a specific IRQ line on Master or Slave PIC.
pub unsafe fn clear_mask(irq: u8) {
    let port = if irq < 8 { PIC1_DATA } else { PIC2_DATA };
    let irq_line = if irq < 8 { irq } else { irq - 8 };
    let mut data_port: Port<u8> = Port::new(port);
    let value = data_port.read() & !(1 << irq_line);
    data_port.write(value);
}

/// Masks a specific IRQ line on Master or Slave PIC.
#[allow(dead_code)]
pub unsafe fn set_mask(irq: u8) {
    let port = if irq < 8 { PIC1_DATA } else { PIC2_DATA };
    let irq_line = if irq < 8 { irq } else { irq - 8 };
    let mut data_port: Port<u8> = Port::new(port);
    let value = data_port.read() | (1 << irq_line);
    data_port.write(value);
}

/// Enables hardware interrupts for keyboard (IRQ1) and mouse (IRQ12 via cascade IRQ2).
pub unsafe fn enable_input_irqs() {
    clear_mask(1);  // IRQ1: Keyboard
    clear_mask(2);  // IRQ2: Cascade to Slave PIC
    clear_mask(12); // IRQ12: Mouse
    log::info!("[PIC] Input IRQs enabled: IRQ1 (Keyboard), IRQ2 (Cascade), IRQ12 (Mouse).");
}
