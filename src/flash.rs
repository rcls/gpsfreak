//! Flash memory handling.  The flash is in two 64kB banks, at 0x08000000
//! and 0x08010000 repspectively.  We run from the first bank and only write
//! to the second bank.

pub type Mem32 = [u32; 8];

pub type Result = core::result::Result<(), ()>;

macro_rules!dbgln {($($tt:tt)*) => {if true {crate::dbgln!($($tt)*)}};}

pub unsafe fn program32(address: usize, data: &Mem32) -> Result {
    let flash = unsafe {&*stm32h503::FLASH::ptr()};

    dbgln!("FLASH - program32.");

    address_check(address, 31)?;

    // Check that the flash block is erased.
    if unsafe{&*(address as *const Mem32)}.iter().any(|&x| x != !0) {
        dbgln!("FLASH - block @ {address:#010x} is already written.");
        return Err(());
    }
    if data.iter().all(|&x| x == !0) {
        dbgln!("FLASH - nothing to do.");
        return Ok(());                  // Nothing to do!
    }
    if busy() {
        dbgln!("FLASH - busy! WTF? NSSR = {:#010x}", flash.NSSR.read().bits());
        return Err(());
    }

    write_unlock()?;

    flash.NSCR.write(|w| w.PG().set_bit().LOCK().clear_bit());

    // Do the write.
    let target = address as *mut u32;
    for (i, &b) in data.iter().enumerate() {
        unsafe {core::ptr::write_volatile(target.wrapping_add(i), b)};
    }

    flash_result()
}

/// Erase a sector (in the inactive bank), or erase the entire bank.
pub fn erase(address: usize) -> Result {
    let flash = unsafe {&*stm32h503::FLASH::ptr()};

    let bank = address == 0x0801ffff;
    if bank {
        dbgln!("FLASH - bank erase");
    }
    else {
        dbgln!("FLASH - erase_sector {address:#010x}");
        address_check(address, 8191)?;
    }

    if busy() {
        dbgln!("FLASH - busy! WTF? NSSR = {:#010x}", flash.NSSR.read().bits());
        return Err(());
    }

    write_unlock()?;

    let swapped = flash.OPTCR.read().SWAP_BANK().bit();
    let snb = if bank {address >> 13 & 7} else {0};

    dbgln!("FLASH - bank {bank} swapped {swapped} sector number {snb}");

    flash.NSCR.write(
        |w|w.BKSEL().bit(!swapped).BER().bit(bank).SER().bit(!bank)
            .SNB().bits(snb as u8).STRT().set_bit().LOCK().clear_bit());

    flash_result()
}

fn address_check(address: usize, mask: usize) -> Result {
    if address & mask != 0 || address < 0x08010000 || address >= 0x08020000 {
        dbgln!("FLASH - out of range or unaligned {address:#010x}.");
        return Err(());
    }
    Ok(())
}

/// Unlock flash if needed.  First, carry out routine prep for starting a
/// flash command.
fn write_unlock() -> Result {
    let flash  = unsafe {&*stm32h503::FLASH ::ptr()};
    let icache = unsafe {&*stm32h503::ICACHE::ptr()};

    crate::cpu::barrier();
    // Start an ICACHE invalidation.  The docs don't say we can't continue
    // during the invalidate.
    icache.CR.modify(|_,w| w.CACHEINV().set_bit());

    // Clear any left over errors.
    flash.NSCCR.write(|w| w.bits(flash.NSSR.read().bits()));

    if !flash.NSCR.read().LOCK().bit() {
        dbgln!("FLASH - already unlocked.");
        return Ok(());                  // Already unlocked.
    }
    // Magic key numbers.
    crate::cpu::interrupt::disable_all();
    flash.NSKEYR.write(|w| w.bits(0x45670123));
    flash.NSKEYR.write(|w| w.bits(0xcdef89ab));
    crate::cpu::interrupt::enable_all();

    dbgln!("FLASH - after unlock, NSCR = {:#010x}.", flash.NSCR.read().bits());

    if flash.NSCR.read().LOCK().bit() {Err(())} else {Ok(())}
}

fn flash_result() -> Result {
    let flash  = unsafe {&*stm32h503::FLASH ::ptr()};
    let icache = unsafe {&*stm32h503::ICACHE::ptr()};
    // Wait for the write operation.  Be lazy and spin.  FIXME - the interrupts
    // are easy to manage.

    while flash.NSSR.read().BSY().bit() {}

    flash.NSCR.write(|w| w.bits(0));

    // Check for errors.  EOP clear should be an error, because it means we did
    // nothing.
    let errors = flash.NSSR.read().bits();
    flash.NSCCR.write(|w| w.bits(errors & 0xffff0000));

    dbgln!("FLASH - result NSSR = {:#010x}", errors);
    dbgln!("FLASH - icache CR = {:#010x}", icache.CR.read().bits());

    if errors & 0xfffe0000 == 0 {
        dbgln!("FLASH success");
        Ok(())
    } else {
        dbgln!("FLASH failure");
        Err(())
    }
}

fn busy() -> bool {
    let flash = unsafe {&*stm32h503::FLASH::ptr()};
    let nssr = flash.NSSR.read();
    // TODO - is it possible to clear the write buffer?
    nssr.BSY().bit() || nssr.DBNE().bit() || nssr.WBNE().bit()
}
