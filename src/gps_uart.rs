// USART2
// TX on pin 19 PA8 (USART2 AF4, USART3 AF13)
// RX on pin 18 PB15 (USART2 AF13, USART1 AF4, LPUART1, AF8)

use crate::cpu::WFE;
use crate::cpu::interrupt::{self, PRIO_COMMS};
use crate::dma::DMA_Channel;
use crate::vcell::VCell;

use stm32h503::GPDMA1 as DMA;
use stm32h503::USART2 as UART;
use stm32h503::Interrupt::USART2 as INTERRUPT;
use stm32h503::Interrupt::GPDMA1_CH0 as DMA_INTERRUPT;

// NOTE: In safe boot we seem to need a UU training sequence to get the baud
// rate sane.

/// Default baud rate at power up.  9600 matches the factory default of the
/// MAX-F10S.
const BAUD: u32 = 9600;

/// BRR setting to match BAUD.
const BRR: u32 = (crate::cpu::CPU_FREQ + BAUD/2) / BAUD;
const _: () = assert!(BRR > 32 && BRR < 65536);

pub type GpsPriority = crate::cpu::Priority<PRIO_COMMS>;

/// For serial TX we use DMA.
const DMA_CHANNEL: usize = 0;

/// USART2 DMA TX.
const TX_DMA_REQ: u8 = 24;

/// Set to true to loopback our own data instead of processing received data.
const LOOPBACK: bool = false;

static BAUD_RATE: VCell<u32> = VCell::new(BAUD);

macro_rules!dbgln {($($tt:tt)*) => {if false {crate::dbgln!($($tt)*)}};}

pub fn init() {
    let gpioa = unsafe {&*stm32h503::GPIOA::ptr()};
    let gpiob = unsafe {&*stm32h503::GPIOB::ptr()};
    let rcc   = unsafe {&*stm32h503::RCC  ::ptr()};
    let dma   = unsafe {&*DMA ::ptr()};
    let uart  = unsafe {&*UART::ptr()};

    rcc.AHB1ENR.modify(|_,w| w.GPDMA1EN().set_bit());
    rcc.APB1LENR.modify(|_,w| w.USART2EN().set_bit());

    gpioa.AFRH.modify(|_,w| w.AFSEL8().B_0x4());
    gpiob.AFRH.modify(|_,w| w.AFSEL15().B_0xD());
    gpioa.MODER.modify(|_,w| w.MODE8().B_0x2());
    gpiob.MODER.modify(|_,w| w.MODE15().B_0x2());

    uart.BRR.write(|w| w.bits(BRR));

    uart.CR3.write(|w| w.RXFTCFG().bits(5).RXFTIE().set_bit().DMAT().set_bit());
    uart.CR1.write(
        |w|w.FIFOEN().set_bit().RE().set_bit().RXFNEIE().set_bit()
            .TCIE().set_bit().TE().set_bit().UE().set_bit());

    let ch = &dma.C[DMA_CHANNEL];
    ch.writes_to(uart.TDR.as_ptr() as *mut u8, TX_DMA_REQ);

    // We interact with the USB subsystem, so share its priority.
    interrupt::enable_priority(INTERRUPT, PRIO_COMMS);
    interrupt::enable_priority(DMA_INTERRUPT, PRIO_COMMS);
}

pub fn set_baud_rate(baud: u32) -> bool {
    let uart  = unsafe {&*UART::ptr()};
    // We need to disable the UART to update the baud rate.
    // FIXME - use the prescalar also.
    let brr = (crate::cpu::CPU_FREQ * 2 + baud) / (baud * 2);
    // We are called from the USB ISR, which is the same priority as our ISRs.
    // So there should be no interrupt to race with.
    let config = uart.CR1.read().bits();
    uart.CR1.write(|w| w.UE().clear_bit());
    uart.BRR.write(|w| w.bits(brr));
    uart.CR1.write(|w| w.bits(config));
    BAUD_RATE.write(baud);
    true
}

pub fn get_baud_rate() -> u32 {
    BAUD_RATE.read()
}

/// Returns false if the DMA is busy, or true if the DMA is started.
/// Len must fit in 16 bits.  This is called at the same priority as our
/// interrupt handlers, so we do not race with our ISRs.
pub fn dma_tx(data: *const u8, len: usize) -> bool {
    dbgln!("UART TX {len} bytes");
    if LOOPBACK {
        for b in unsafe {core::slice::from_raw_parts(data, len)} {
            // Evil alert - this is broken rust, we grab a second &mut pointer!
            crate::usb::serial_tx_byte(*b);
        }
    }
    let dma  = unsafe {&*DMA ::ptr()};
    let ch = &dma.C[DMA_CHANNEL];

    if ch.busy() {
        return false;
    }

    ch.write(data as usize, len, 0);
    crate::cpu::barrier();
    true
}

pub fn dma_tx_busy() -> bool {
    let dma = unsafe {&*DMA ::ptr()};
    dma.C[DMA_CHANNEL].busy()
}

pub fn wait_for_tx_idle() {
    let uart = unsafe {&*UART::ptr()};
    while dma_tx_busy() || !uart.ISR.read().TC().bit() {
        // Arm the TC interrupt.
        let prio = GpsPriority::new();
        uart.CR1().modify(|_,w| w.TCIE().set_bit());
        drop(prio);
        WFE();
    }
}

fn uart_isr() {
    let uart = unsafe {&*UART::ptr()};
    let isr = uart.ISR.read();
    let cr1 = uart.CR1.read();
    // Attempt to clear all the interrupts.  Except we don't clear the TC
    // flag, instead we clear the TCIE.
    uart.ICR.write(|w| w.bits(isr.bits()).TCCF().clear_bit());
    //crate::dbg!("UART ISR = {:#010x}", isr.bits());

    let rxfne = isr.RXFNE().bit();
    // Whenever RXFT is set, or we reach idle, push the data through.
    // TODO - do we need IDLE interrupt?  We could just poll from SOF.
    if isr.RXFT().bit() || rxfne && isr.IDLE().bit() && cr1.IDLEIE().bit() {
        // Drain the FIFO.
        loop {
            let byte = uart.RDR.read().bits() as u8;
            if !LOOPBACK {
                crate::usb::serial_tx_byte(byte);
            }

            if !uart.ISR.read().RXFNE().bit() {
                break;
            }
        }
    }

    uart.CR1.write(
        |w| w.bits(cr1.bits()).RXFNEIE().bit(!rxfne).IDLEIE().bit(rxfne)
             .TCIE().bit(cr1.TCIE().bit() & !isr.TC().bit()));
}

fn dma_isr() {
    let dma = unsafe {&*DMA::ptr()};
    let ch = &dma.C[DMA_CHANNEL];

    let sr = ch.SR.read();
    ch.FCR.write(|w| w.bits(sr.bits()));      // Clear the interrupts.

    // Be care to read CR after SR.
    let cr = ch.CR.read();

    if !cr.EN().bit() && sr.bits() & 0x7f00 != 0 {
        // We completed a transfer, or it errored.
        crate::usb::serial_rx_done();
    }
}

impl crate::cpu::VectorTable {
    pub const fn gps_uart(&mut self) -> &mut Self {
        self.isr(INTERRUPT, uart_isr).isr(DMA_INTERRUPT, dma_isr)
    }
}
