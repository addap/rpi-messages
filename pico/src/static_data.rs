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
//! middle of the binary are filled up so that our initial values are written. a.d. TODO how solved?
//!
//! [0] https://datasheets.raspberrypi.com/rp2040/rp2040-datasheet.pdf#errata-e14
//!
//! SAFETY - we never mutate the static variables; we only use `mut` to stop the compiler from inlining them.
//! TODO - statics are supposed to never be inlined. Check it again and remove mut if possible.

use core::ffi::CStr;

use common::{
    consts::{WIFI_PW_LEN, WIFI_SSID_LEN},
    types::DeviceID,
};
use embassy_net::{IpAddress, IpEndpoint};

#[used]
#[link_section = ".device_info.id"]
pub static DEVICE_ID: u32 = 0xcafebabe;

#[used]
#[link_section = ".wifi_info.ssid"]
pub static WIFI_SSID_BYTES: [u8; WIFI_SSID_LEN] = *b"\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";
#[used]
#[link_section = ".wifi_info.pw"]
pub static WIFI_PW_BYTES: [u8; WIFI_PW_LEN] =
    *b"\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";
#[used]
#[link_section = ".wifi_info.ip"]
pub static SERVER_IPV4_BYTES: [u8; 4] = [192, 168, 188, 69];
#[used]
#[link_section = ".wifi_info.port"]
pub static SERVER_PORT: u16 = 1338;

#[inline(never)]
pub fn device_id() -> DeviceID {
    let id = DEVICE_ID;
    DeviceID(id)
}

pub fn wifi_ssid() -> Option<&'static str> {
    let cstr = match CStr::from_bytes_until_nul(&WIFI_SSID_BYTES) {
        Ok(cstr) => cstr,
        Err(e) => {
            log::error!("Parsing Wifi SSID failed.\n{}", e);
            return None;
        }
    };
    match cstr.to_str() {
        Ok(wifi_ssid) => Some(wifi_ssid),
        Err(e) => {
            log::error!("Parsing Wifi SSID failed\n{}", e);
            None
        }
    }
}

pub fn wifi_password() -> Option<&'static str> {
    let cstr = match CStr::from_bytes_until_nul(&WIFI_PW_BYTES) {
        Ok(cstr) => cstr,
        Err(e) => {
            log::error!("Parsing Wifi password failed.\n{}", e);
            return None;
        }
    };
    match cstr.to_str() {
        Ok(wifi_pw) => Some(wifi_pw),
        Err(e) => {
            log::error!("Parsing Wifi password failed.\n{}", e);
            None
        }
    }
}

pub fn server_endpoint() -> IpEndpoint {
    let a0: u8 = SERVER_IPV4_BYTES[0];
    let a1: u8 = SERVER_IPV4_BYTES[1];
    let a2: u8 = SERVER_IPV4_BYTES[2];
    let a3: u8 = SERVER_IPV4_BYTES[3];
    let port = SERVER_PORT;

    IpEndpoint::new(IpAddress::v4(a0, a1, a2, a3), port)
}
