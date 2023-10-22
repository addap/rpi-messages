use image::{imageops::resize, EncodableLayout, ImageBuffer, RgbImage};
use rpi_messages_common::{IMAGE_BYTES_PER_PIXEL, IMAGE_HEIGHT, IMAGE_WIDTH};

fn convert_image(img: RgbImage) -> Vec<u8> {
    //
    let img = resize(
        &img,
        IMAGE_WIDTH as u32,
        IMAGE_HEIGHT as u32,
        image::imageops::FilterType::Gaussian,
    );

    let mut bytes = Vec::with_capacity(IMAGE_HEIGHT * IMAGE_WIDTH * IMAGE_BYTES_PER_PIXEL);
    for px in img.pixels() {
        let [r, g, b] = px.0;

        let [c1, c2] = rgb565::Rgb565::from_srgb888_components(r, g, b).to_rgb565_be();
        bytes.push(c1);
        bytes.push(c2);
    }
    bytes
}
