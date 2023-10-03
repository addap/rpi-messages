use core::ffi::CStr;

use embassy_net::{IpAddress, IpEndpoint};

use crate::error::{Error, Result};

#[used]
#[link_section = ".device_info"]
pub static DEVICE_ID: u32 = 0xbabebabe;

#[used]
#[link_section = ".wifi_info"]
pub static WIFI_SSID_BYTES: [u8; 32] = *b"Buffalo-G-1337\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";
#[used]
#[link_section = ".wifi_info"]
pub static WIFI_PW_BYTES: [u8; 32] = *b"mysecretpw\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";
#[used]
#[link_section = ".wifi_info"]
pub static SERVER_IP_BYTES: [u8; 4] = [192, 168, 12, 1];
#[used]
#[link_section = ".wifi_info"]
pub static SERVER_PORT: u16 = 1337;

#[inline(never)]
pub fn device_id() -> u32 {
    DEVICE_ID
}

pub fn wifi_ssid() -> Result<&'static str> {
    let cstr = CStr::from_bytes_until_nul(&WIFI_SSID_BYTES).map_err(|e| {
        log::error!("Parsing Wifi SSID failed: {}", e);
        Error::MemoryError
    })?;
    cstr.to_str().map_err(|e| {
        log::error!("Parsing Wifi SSID failed: {}", e);
        Error::MemoryError
    })
}

pub fn wifi_password() -> Result<&'static str> {
    let cstr = CStr::from_bytes_until_nul(&WIFI_PW_BYTES).map_err(|e| {
        log::error!("Parsing Wifi password failed: {}", e);
        Error::MemoryError
    })?;
    cstr.to_str().map_err(|e| {
        log::error!("Parsing Wifi password failed: {}", e);
        Error::MemoryError
    })
}

pub fn server_endpoint() -> IpEndpoint {
    let a0: u8 = SERVER_IP_BYTES[0];
    let a1: u8 = SERVER_IP_BYTES[1];
    let a2: u8 = SERVER_IP_BYTES[2];
    let a3: u8 = SERVER_IP_BYTES[3];
    let port = SERVER_PORT;

    IpEndpoint::new(IpAddress::v4(a0, a1, a2, a3), port)
}
