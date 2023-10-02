use core::ffi::CStr;

use embassy_net::{IpAddress, IpEndpoint, Stack};

pub static DEVICE_ID: u8 = 0;

// #[link_section = ".wifi_info"]
// static WIFI_SSID_BYTES: [u8; 32] = *b"Buffalo-G-1337\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";
// #[link_section = ".wifi_info"]
// static WIFI_PW_BYTES: [u8; 32] = *b"mysecretpw\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";

#[link_section = ".wifi_info"]
static WIFI_SSID_BYTES: [u8; 32] = *b"TP-Link_0FFC\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";
#[link_section = ".wifi_info"]
static WIFI_PW_BYTES: [u8; 32] = *b"70667103\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";
// static WIFI_SSID_BYTES: [u8; 32] = *b"Buffalo-G-1337\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";
// static WIFI_PW_BYTES: [u8; 32] = *b"mysecretpw\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";

pub static SERVER_ENDPOINT: IpEndpoint = IpEndpoint::new(IpAddress::v4(192, 168, 0, 194), 1337);

pub fn wifi_ssid() -> Option<&'static str> {
    let end = WIFI_SSID_BYTES
        .iter()
        .position(|b| *b == 0)
        .unwrap_or_else(|| WIFI_SSID_BYTES.len());

    log::info!(
        "SSID: {:x}{:x}{:x}",
        WIFI_SSID_BYTES[0],
        WIFI_SSID_BYTES[1],
        WIFI_SSID_BYTES[2]
    );

    match core::str::from_utf8(&WIFI_SSID_BYTES[..end]) {
        Ok(wifi_ssid) => Some(wifi_ssid),
        Err(e) => {
            log::error!("Parsing Wifi SSID failed: {}", e);
            None
        }
    }

    // let cstr = match CStr::from_bytes_until_nul(&WIFI_SSID_BYTES) {
    //     Ok(cstr) => cstr,
    //     Err(e) => {
    //         log::error!("Parsing Wifi SSID failed: {}", e);
    //         return None;
    //     }
    // };
    // match cstr.to_str() {
    //     Ok(wifi_ssid) => Some(wifi_ssid),
    //     Err(e) => {
    //         log::error!("Parsing Wifi SSID failed: {}", e);
    //         None
    //     }
    // }
}

pub fn wifi_password() -> Option<&'static str> {
    return Some("70667103");
    // let cstr = match CStr::from_bytes_until_nul(&WIFI_PW_BYTES) {
    //     Ok(cstr) => cstr,
    //     Err(e) => {
    //         log::error!("Parsing Wifi password failed: {}", e);
    //         return None;
    //     }
    // };
    // match cstr.to_str() {
    //     Ok(wifi_pw) => Some(wifi_pw),
    //     Err(e) => {
    //         log::error!("Parsing Wifi password failed: {}", e);
    //         None
    //     }
    // }
}
