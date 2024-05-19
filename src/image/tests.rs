use std::path::Path;
use std::{fmt, fs};

use super::{ImageData, Px};
use crate::test_utils::log;

#[test]
fn px_parsing() {
    assert_eq!("500".parse::<Px>().unwrap(), Px(500));
    assert_eq!("500px".parse::<Px>().unwrap(), Px(500));
}

// Checks that the image crate converting to RGBA8 is the same as our technique
fn check(input_path: &Path) {
    let bytes = fs::read(input_path).unwrap();

    let expected = image::load_from_memory(&bytes)
        .unwrap()
        .into_rgba8()
        .into_vec();

    let image = ImageData::load(&bytes, false).unwrap();
    let actual = image.to_bytes();

    assert_eq!(
        Rgba8Data::new(&actual),
        Rgba8Data::new(&expected),
        "Input: {:?}",
        input_path
    );
}

#[test]
fn source_image_variety() {
    log::init();

    for file in [
        "rgb8.gif",
        "rgb8.jpg",
        "rgb8.png",
        // QOI tests our non-streamed fallback code because who uses this?
        "rgb8.qoi",
        "rgba8.gif",
        "rgba8.jpg",
        "rgba8.png",
        // QOI tests our non-streamed fallback code because who uses this?
        "rgba8.qoi",
    ] {
        check(&Path::new("assets").join("test_data").join(file));
    }
}

#[derive(PartialEq)]
struct Rgba8Data(Vec<[u8; 4]>);

impl Rgba8Data {
    fn new(bytes: &[u8]) -> Self {
        assert_eq!(bytes.len() % 4, 0);
        let pixels = bytes
            .chunks(4)
            .map(|chunk| match chunk {
                &[r, g, b, a] => [r, g, b, a],
                _ => unreachable!(),
            })
            .collect();
        Self(pixels)
    }
}

impl fmt::Debug for Rgba8Data {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(
                self.0
                    .iter()
                    .map(|&pixel| format!("0x{:08x}", u32::from_be_bytes(pixel))),
            )
            .finish()
    }
}
