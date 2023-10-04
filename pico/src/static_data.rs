use core::ffi::CStr;

use embassy_net::{IpAddress, IpEndpoint};

// SAFETY - we never mutate the static variables; we only use `mut` to stop the compiler from inlining them.

#[used]
#[link_section = ".device_info"]
pub static mut DEVICE_ID: u32 = 0xbabebabe;

#[used]
#[link_section = ".wifi_info"]
pub static mut WIFI_SSID_BYTES: [u8; 32] = *b"Buffalo-G-1337\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";
#[used]
#[link_section = ".wifi_info"]
pub static mut WIFI_PW_BYTES: [u8; 32] = *b"mysecretpw\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";
#[used]
#[link_section = ".wifi_info"]
pub static mut SERVER_IP_BYTES: [u8; 4] = [192, 168, 12, 1];
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
