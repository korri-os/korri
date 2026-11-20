//! A sign-in's `challenge_url` to the treaty's `SurfaceQrCode` (contract
//! "Sign-in": a QR encoder in korrid; the surface never encodes). Every
//! account sign-in, Steam's and a plugin's, uses it. The URL is the QR payload
//! and may be published; it is never logged.
use korri_surface_treaty::SurfaceQrCode;

/// The module grid of `payload`: `size` rows of `size` `0`/`1` characters,
/// row by row from the top, with no quiet zone. Error correction is level H
/// (ADR 0089): about 30% of the code can be lost, so a surface may draw the
/// service's logo in the middle. `None` when the payload does not fit a QR
/// code.
pub fn grid(payload: &str) -> Option<SurfaceQrCode> {
    let code =
        qrcode::QrCode::with_error_correction_level(payload.as_bytes(), qrcode::EcLevel::H).ok()?;
    let size = code.width();
    let modules = code
        .to_colors()
        .into_iter()
        .map(|color| match color {
            qrcode::Color::Dark => '1',
            qrcode::Color::Light => '0',
        })
        .collect();
    Some(SurfaceQrCode {
        size: u32::try_from(size).ok()?,
        modules,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_challenge_url_becomes_a_square_grid_with_finder_patterns() {
        let code = grid("https://s.team/q/1/1234567890123456789").unwrap();
        let size = code.size as usize;
        assert!(
            size >= 21 && (size - 21).is_multiple_of(4),
            "QR sizes are 21 + 4k"
        );
        assert_eq!(code.modules.len(), size * size);
        assert!(code.modules.bytes().all(|b| b == b'0' || b == b'1'));
        let row = |y: usize| &code.modules[y * size..(y + 1) * size];
        // The top-left finder pattern: a dark 7-module border, then a light
        // ring, then a dark 3x3 core; the top-right one mirrors it.
        assert_eq!(&row(0)[..7], "1111111");
        assert_eq!(&row(1)[..7], "1000001");
        assert_eq!(&row(3)[..7], "1011101");
        assert_eq!(&row(0)[size - 7..], "1111111");
        assert_eq!(&row(size - 1)[..7], "1111111");
        // Same payload, same grid: the surface can compare to redraw.
        assert_eq!(grid("https://s.team/q/1/1234567890123456789"), Some(code));
    }

    /// Level H: the same payload needs a larger code than the default level
    /// M would, and the format bits next to the top-left finder say H.
    #[test]
    fn every_code_is_encoded_at_error_correction_level_h() {
        let payload = "https://login.tailscale.com/a/1a2b3c4d5e6f7";
        let code = grid(payload).unwrap();
        let medium =
            qrcode::QrCode::with_error_correction_level(payload.as_bytes(), qrcode::EcLevel::M)
                .unwrap();
        assert!(code.size as usize > medium.width());
        let high =
            qrcode::QrCode::with_error_correction_level(payload.as_bytes(), qrcode::EcLevel::H)
                .unwrap();
        assert_eq!(high.error_correction_level(), qrcode::EcLevel::H);
        assert_eq!(code.size as usize, high.width());
        let modules: String = high
            .to_colors()
            .into_iter()
            .map(|c| if c == qrcode::Color::Dark { '1' } else { '0' })
            .collect();
        assert_eq!(code.modules, modules);
    }

    #[test]
    fn a_payload_too_long_for_any_qr_code_has_no_grid() {
        assert_eq!(grid(&"x".repeat(8000)), None);
    }
}
