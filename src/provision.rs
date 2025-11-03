//! Device config provisioning.
//!
//! This applies a canned config at start-up.
//!
//! A config sector in flash contains a list of commands, which are run in
//! sequence.  The commands are either our native command set, or else a UBlox
//! command (to be sent to the GPS unit).
//!
//! The format of the provisioning buffer is:
//!
//! u32 Magic
//! u32 Format revision (currently 1)
//! u32 Configuration generation number.
//! u32 Byte length (including magic and CRC)
//! The byte array containing the config.
//! u32 CRC.
//!
//! The byte content just a sequence of packets, we parse them out of the
//! byte stream.  Zero bytes maybe inserted for padding.
//!
//! The flash can contain multiple configs; each config is up to 2kB long and
//! is 2kB align.  Four 8kB flash sectors (the last two in each 64kB flash bank)
//! are assigned for this purpose.
//!
//! The config to apply at start up is choosen as the valid config with the
//! largest generation number.  With a 32 bit generation number there should
//! be no risk of rollover within the lifetime of the flash!
//!
//! The procedure for writing a new config is:
//! - Give it a generation number one more than that of the current config.
//! - Write it to a blank sector in the second flash bank.  If there is none,
//!   then erase a sector in the second flash bank that does not contain the
//!   current config.
//!
//! That procedure should ensure that an interrupted config update leaves us
//! still using the previous one.

use crate::crc32::{self, VERIFY_MAGIC};
use crate::gps_uart::GpsPriority;
use crate::vcell::UCell;

const CONFIG_MAGIC: u32 = 0x4b72a6ce;

const CONFIG_MAX_LENGTH: usize = 2048;

const MIN_SUPPORTED_VERSION: u32 = 1;
const MAX_SUPPORTED_VERSION: u32 = 1;

macro_rules!dbgln {($($tt:tt)*) => {if false {crate::dbgln!($($tt)*)}};}

#[repr(C)]
struct ConfigBlock {
    /// Magic number, 0x4b72a6ce
    magic: u32,
    /// Version number, currently 1.
    version: u32,
    /// Config generation counter.
    generation: u32,
    /// Total lenth, including magic and CRC.
    length: u32,
    data: [u8; CONFIG_MAX_LENGTH - 16],
}
const _: () = assert!(size_of::<ConfigBlock>() == CONFIG_MAX_LENGTH);

pub fn provision() {
    let Some(c) = best_config() else {
        dbgln!("No config found");
        return;
    };
    let mut data = &c.data[.. c.length as usize - 20];
    use crate::led::BLUE;
    BLUE.set(true);

    while data.len() > 0 {
        // Check for a valid command packet.
        dbgln!("Next packet @ {:#?}", data.as_ptr());
        if data.len() >= 6 && data[0] == 0xce && data[1] == 0x93 {
            let length = data[3] as usize + 6;
            dbgln!("Freak packet len {} total len {length}", data[3]);
            if data.len() < length {
                dbgln!("Config command doesn't fit.");
                break;
            }
            // Ok, it looks like a packet try and run it...
            run_command_packet(&data[0..length]);
            data = &data[length ..];
            continue;
        }
        // Check for a valid U-Blox message.
        if data.len() >= 8 && data[0] == 'µ' as u8 && data[1] == 'b' as u8 {
            let lfield = data[4] as usize + data[5] as usize * 256;
            let length = lfield + 8;
            dbgln!("UBX packet len {lfield} total {length}");
            if data.len() < length {
                dbgln!("Config u-blox doesn't fit @ {:#?}.", data.as_ptr());
                break;
            }
            run_ublox_command(data.as_ptr(), length);
            data = &data[length ..];
            continue;
        }
        dbgln!("Unknown data in config @ {:#?}.", data.as_ptr());
        break;
    }
    BLUE.set(false);
}

static COM_BUF: UCell<crate::command::MessageBuf> = Default::default();

fn run_command_packet(data: &[u8]) {
    dbgln!("Run command packet @{:#?} {} bytes", data.as_ptr(), data.len());

    if data.len() > 64 {
        dbgln!("That's too big, drop the packet.");
        return;
    }

    // Copy, because the command handling needs a 32-bit aligned 64-byte buffer
    // 'cos its stupid.
    let com_buf = unsafe{COM_BUF.as_mut()};
    unsafe {
        core::ptr::copy_nonoverlapping(
            data.as_ptr(), com_buf as *mut _ as *mut u8, data.len());
    }

    crate::command::command_handler(com_buf, data.len(), |_| ());
}

fn run_ublox_command(data: *const u8, length: usize) {
    dbgln!("Run U-Blox packet @{data:#?} {length} bytes");
    loop {
        let prio = GpsPriority::new();
        let ok = crate::gps_uart::dma_tx(data, length);
        drop(prio);
        if ok {
            break;
        }
        crate::cpu::WFE();
    }
    crate::gps_uart::wait_for_tx_idle();
}

fn config_by_index(i: u8) -> &'static ConfigBlock {
    let base: usize = 0x0800c000 + if i & 8 != 0 {0x10000} else {0};
    const {assert!(0x4000 / CONFIG_MAX_LENGTH == 8)};
    const {assert!(0x4000 == 2 * 8192)};
    let address = base + 2048 * (i as usize & 7);

    unsafe {&* (address as *const ConfigBlock)}
}

fn best_config() -> Option<&'static ConfigBlock> {
    let mut indexes = [0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];

    indexes.sort_unstable_by_key(config_sort_key);

    for c in indexes.iter().rev().map(|&i| config_by_index(i)) {
        if c.magic != CONFIG_MAGIC {
            dbgln!("Magic wrong @ {:#?}", c as *const ConfigBlock);
            break;
        }
        let length = c.length as usize;
        if length < 20 || length >= CONFIG_MAX_LENGTH {
            dbgln!("Length {length} too big @ {:#?}", c as *const ConfigBlock);
            continue;
        }
        if crc32::compute(c as *const ConfigBlock as *const u8, length)
            == VERIFY_MAGIC {
            dbgln!("CRC good @ {:#?}", c as *const ConfigBlock);
            return Some(c);
        }
    }
    None
}

/// Key for sorting configs.  Configs with "greater" keys are better.
fn config_sort_key(i: &u8) -> (bool, u32, u8) {
    let c = config_by_index(*i);
    let version_ok = MIN_SUPPORTED_VERSION <= c.version
        && c.version <= MAX_SUPPORTED_VERSION;
    (c.magic == CONFIG_MAGIC && version_ok, c.generation, *i)
}
