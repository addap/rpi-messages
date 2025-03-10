use anyhow::anyhow;
use axum::{
    http::{header, HeaderMap},
    response::IntoResponse,
    Form,
};
use common::consts::{WIFI_PW_LEN, WIFI_SSID_LEN};
use serde::Deserialize;

use crate::WebResult;

#[derive(Deserialize)]
pub struct WifiData {
    wifissid: String,
    wifipw: String,
}

fn gen_block(address: u32, block_id: u32, data: &[u8]) -> Vec<u8> {
    assert!(data.len() == 256);
    let mut result = Vec::with_capacity(512);
    // magic numbers
    result.extend_from_slice(&[0x55, 0x46, 0x32, 0x0a, 0x57, 0x51, 0x5d, 0x9e]);
    // flags (familyID present)
    result.extend_from_slice(&[0x00, 0x20, 0x00, 0x00]);
    // address where it should be written
    result.extend_from_slice(&address.to_le_bytes());
    // size of block (256)
    result.extend_from_slice(&[0x00, 0x01, 0x00, 0x00]);
    // sequential block number
    result.extend_from_slice(&block_id.to_le_bytes());
    // total number of blocks
    result.extend_from_slice(&16u32.to_le_bytes());
    // familyID
    result.extend_from_slice(&[0x56, 0xff, 0x8b, 0xe4]);
    result.extend_from_slice(data);
    // padding to bring block to 512 bytes
    result.extend_from_slice(&[0u8; 476 - 256]);
    // magic number
    result.extend_from_slice(&[0x30, 0x6f, 0xb1, 0x0a]);

    assert!(result.len() == 512);

    return result;
}

pub async fn submit_wifi_config(Form(data): Form<WifiData>) -> WebResult<impl IntoResponse> {
    println!("ssid: {}\npw: {}", data.wifissid, data.wifipw);
    // Compare >= X_LEN because we are saving null-terminated strings, so the data must be stricly smaller.
    if data.wifissid.as_bytes().len() >= WIFI_SSID_LEN
        || data.wifipw.as_bytes().len() >= WIFI_PW_LEN
    {
        return Err(anyhow!("Wifi password or SSID are too long.").into());
    }

    const WIFI_BASE_ADDRESS: u32 = 0x10fff000;
    let ssid = data.wifissid.as_bytes();
    let pw = data.wifipw.as_bytes();

    let mut wifi_data = Vec::with_capacity(256);
    wifi_data.extend_from_slice(ssid);
    wifi_data.extend_from_slice(&vec![0u8; 32 - ssid.len()]);
    wifi_data.extend_from_slice(pw);
    wifi_data.extend_from_slice(&vec![0u8; 32 - pw.len()]);
    wifi_data.extend_from_slice(&[0u8; 256 - 64]);

    let mut file = Vec::with_capacity(16 * 512);
    file.append(&mut gen_block(WIFI_BASE_ADDRESS, 0, &wifi_data[..]));

    for i in 1..16 {
        file.append(&mut gen_block(WIFI_BASE_ADDRESS + 256 * i, i, &[0u8; 256]));
    }

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_DISPOSITION,
        "attachment; filename=\"wifi.uf2\"".parse().unwrap(),
    );
    headers.insert(
        header::CONTENT_TYPE,
        "application/octet-stream".parse().unwrap(),
    );

    Ok((headers, file))
}
