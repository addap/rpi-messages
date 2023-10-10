//! Module to keep our static items. To make changing settings easier after a device has been deployed we want to
//! be able to change settings by using the UF2 function of the Pico.
//!
//! There are two sections, a "device_info" section containing data unique to the device, which we want to override before deployment.
//! And "wifi_info", which contains information about the wifi/server the device should connect to. This information
//! could change multiple times over the device's lifetime.
//!
//! We create both sections in the linker script `memory.x` and place them at predetermined addresses.
//! Then we can create UF2 files (e.g. via the python script) containing new information and flash them on the Pico to
//! overwrite the memory at the predetermined addresses.
//! Since the sector size of the Pico flash is 4kB, which must all be erased, our sections are also 4kB which is a lot
//! more than they need.
//!
//! One hurdle is that the Rust compiler wants to inline some static variables when they are short and used seldomly.
//! We avoid this by declaring all variables public and mutable, which prevents inlining.
//! Then there is the bug in the UF2 bootloader of the Pico [0], which means we have to ensure that partial sectors in the
//! middle of the binary are filled up so that our initial values are written.
//!
//! [0] https://datasheets.raspberrypi.com/rp2040/rp2040-datasheet.pdf#errata-e14
//!
//! SAFETY - we never mutate the static variables; we only use `mut` to stop the compiler from inlining them.

use core::ffi::CStr;

use embassy_net::{IpAddress, IpEndpoint};

#[used]
#[link_section = ".device_info"]
pub static mut DEVICE_ID: u32 = 0xbabebabe;

#[used]
#[link_section = ".wifi_info"]
pub static mut WIFI_SSID_BYTES: [u8; 32] = *b"\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";
#[used]
#[link_section = ".wifi_info"]
pub static mut WIFI_PW_BYTES: [u8; 32] = *b"\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";
#[used]
#[link_section = ".wifi_info"]
pub static mut SERVER_IP_BYTES: [u8; 4] = [202, 61, 254, 108];
#[used]
#[link_section = ".wifi_info"]
pub static mut SERVER_PORT: u16 = 1337;

#[inline(never)]
pub fn device_id() -> u32 {
    unsafe { DEVICE_ID }
}

pub fn wifi_ssid() -> Option<&'static str> {
    let cstr = match CStr::from_bytes_until_nul(unsafe { &WIFI_SSID_BYTES }) {
        Ok(cstr) => cstr,
        Err(e) => {
            log::error!("Parsing Wifi SSID failed: {}", e);
            return None;
        }
    };
    match cstr.to_str() {
        Ok(wifi_ssid) => Some(wifi_ssid),
        Err(e) => {
            log::error!("Parsing Wifi SSID failed: {}", e);
            None
        }
    }
}

pub fn wifi_password() -> Option<&'static str> {
    let cstr = match CStr::from_bytes_until_nul(unsafe { &WIFI_PW_BYTES }) {
        Ok(cstr) => cstr,
        Err(e) => {
            log::error!("Parsing Wifi password failed: {}", e);
            return None;
        }
    };
    match cstr.to_str() {
        Ok(wifi_pw) => Some(wifi_pw),
        Err(e) => {
            log::error!("Parsing Wifi password failed: {}", e);
            None
        }
    }
}

pub fn server_endpoint() -> IpEndpoint {
    let a0: u8 = unsafe { SERVER_IP_BYTES[0] };
    let a1: u8 = unsafe { SERVER_IP_BYTES[1] };
    let a2: u8 = unsafe { SERVER_IP_BYTES[2] };
    let a3: u8 = unsafe { SERVER_IP_BYTES[3] };
    let port = unsafe { SERVER_PORT };

    IpEndpoint::new(IpAddress::v4(a0, a1, a2, a3), port)
}
