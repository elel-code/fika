pub(crate) fn copy_rgba_to_premultiplied_argb8888(rgba: &[u8], argb: &mut [u8]) {
    for (source, destination) in rgba.chunks_exact(4).zip(argb.chunks_exact_mut(4)) {
        let alpha = source[3];
        let red = premultiply_alpha(source[0], alpha);
        let green = premultiply_alpha(source[1], alpha);
        let blue = premultiply_alpha(source[2], alpha);
        let native_argb = (u32::from(alpha) << 24
            | u32::from(red) << 16
            | u32::from(green) << 8
            | u32::from(blue))
        .to_ne_bytes();
        destination.copy_from_slice(&native_argb);
    }
}

fn premultiply_alpha(component: u8, alpha: u8) -> u8 {
    ((u16::from(component) * u16::from(alpha) + 127) / 255) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgba_is_premultiplied_and_encoded_as_native_argb() {
        let mut encoded = [0; 8];
        copy_rgba_to_premultiplied_argb8888(&[200, 100, 50, 128, 255, 200, 100, 0], &mut encoded);

        assert_eq!(
            u32::from_ne_bytes(encoded[..4].try_into().unwrap()),
            0x8064_3219
        );
        assert_eq!(u32::from_ne_bytes(encoded[4..].try_into().unwrap()), 0);
    }
}
