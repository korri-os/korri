// SPDX-License-Identifier: Zlib
//! A controller's kind, from its USB vendor and product: the data that
//! Artemis asks SDL's lists for (`MoonBridge.guessControllerType`,
//! `guessControllerHasPaddles`, `guessControllerHasShareButton`). The
//! table is Valve's `controller_list.h` as Korri's copy of Artemis carried
//! it (`clients/android/app/src/main/jni/moonlight-core` at the tag
//! `artemis-final`), reduced to the
//! entries that give the stream a kind; the three functions are SDL's, from
//! the same copy's `minisdl.c` and `usb_ids.h`. Korri changed both on
//! 2026-11-14: the table is Rust data, sorted, with the stream's kind in
//! the place of Valve's controller type. Both originals are under the zlib
//! licence:
//!
//! ```text
//! Copyright (C) Valve Corporation
//! Simple DirectMedia Layer
//! Copyright (C) 1997-2023 Sam Lantinga <slouken@libsdl.org>
//!
//! This software is provided 'as-is', without any express or implied
//! warranty.  In no event will the authors be held liable for any damages
//! arising from the use of this software.
//!
//! Permission is granted to anyone to use this software for any purpose,
//! including commercial applications, and to alter it and redistribute it
//! freely, subject to the following restrictions:
//!
//! 1. The origin of this software must not be misrepresented; you must not
//!    claim that you wrote the original software. If you use this software
//!    in a product, an acknowledgment in the product documentation would be
//!    appreciated but is not required.
//! 2. Altered source versions must be plainly marked as such, and must not be
//!    misrepresented as being the original software.
//! 3. This notice may not be removed or altered from any source distribution.
//! ```

/// The stream's controller kinds (`Limelight.h`, `LI_CTYPE_*`).
pub(crate) const UNKNOWN: u8 = 0x00;
pub(crate) const XBOX: u8 = 0x01;
pub(crate) const PS: u8 = 0x02;
pub(crate) const NINTENDO: u8 = 0x03;

/// The kind that SDL's list gives a vendor and product; Xbox 360 and One
/// are Xbox, PlayStation 3 to 5 are PlayStation, Wii and the Switch
/// controllers are Nintendo, and every other type is unknown.
pub(crate) fn kind(vendor: u16, product: u16) -> u8 {
    let id = u32::from(vendor) << 16 | u32::from(product);
    CONTROLLERS
        .binary_search_by_key(&id, |&(entry, _)| entry)
        .map_or(UNKNOWN, |at| CONTROLLERS[at].1)
}

/// `SDL_IsJoystickXboxOneElite || SDL_IsJoystickDualSenseEdge`.
pub(crate) fn has_paddles(vendor: u16, product: u16) -> bool {
    matches!(
        (vendor, product),
        (0x045e, 0x02e3 | 0x0b00 | 0x0b05 | 0x0b22) | (0x054c, 0x0df2)
    )
}

/// `SDL_IsJoystickXboxSeriesX`: the Share button.
pub(crate) fn has_share(vendor: u16, product: u16) -> bool {
    match vendor {
        0x045e => matches!(product, 0x0b12 | 0x0b13),
        // PDP
        0x0e6f => matches!(product, 0x02d6 | 0x02d9 | 0x02da),
        // PowerA
        0x20d6 => matches!(product, 0x2001..=0x201a | 0x4001 | 0x890b | 0x4002),
        // HORI
        0x0f0d => matches!(product, 0x0150 | 0x014f),
        // Razer
        0x1532 => matches!(product, 0x0a29 | 0x0a2e),
        // Thrustmaster
        0x044f => product == 0xd012,
        // Turtle Beach
        0x10f5 => matches!(product, 0x7013 | 0x7009),
        // 8BitDo
        0x2dc8 => product == 0x2002,
        // GameSir
        0x3537 => product == 0x1001,
        _ => false,
    }
}

/// (vendor << 16 | product, kind), sorted by the first.
#[rustfmt::skip]
static CONTROLLERS: &[(u32, u8)] = &[
    (0x00000000, XBOX), (0x00006686, XBOX), (0x00010001, XBOX),
    (0x0079181a, PS), (0x0079181b, PS), (0x00791832, XBOX),
    (0x00791844, PS), (0x00791874, XBOX), (0x0079187c, XBOX),
    (0x0079187f, XBOX), (0x00791883, XBOX), (0x0079188e, XBOX),
    (0x0079189c, XBOX), (0x007918a1, XBOX), (0x007918c2, XBOX),
    (0x007918c8, XBOX), (0x007918cf, XBOX), (0x007918d3, XBOX),
    (0x007918d4, XBOX), (0x03ebff01, XBOX), (0x03ebff02, XBOX),
    (0x044fb315, PS), (0x044fb326, XBOX), (0x044fd007, PS),
    (0x044fd00e, PS), (0x044fd012, XBOX), (0x045e028e, XBOX),
    (0x045e028f, XBOX), (0x045e0291, XBOX), (0x045e02a0, XBOX),
    (0x045e02a1, XBOX), (0x045e02a2, XBOX), (0x045e02a9, XBOX),
    (0x045e02d1, XBOX), (0x045e02dd, XBOX), (0x045e02e0, XBOX),
    (0x045e02e3, XBOX), (0x045e02ea, XBOX), (0x045e02fd, XBOX),
    (0x045e02ff, XBOX), (0x045e0719, XBOX), (0x045e0867, XBOX),
    (0x045e0b00, XBOX), (0x045e0b05, XBOX), (0x045e0b0a, XBOX),
    (0x045e0b0c, XBOX), (0x045e0b12, XBOX), (0x045e0b13, XBOX),
    (0x045e0b20, XBOX), (0x045e0b21, XBOX), (0x045e0b22, XBOX),
    (0x046d0000, XBOX), (0x046d0291, XBOX), (0x046d0301, XBOX),
    (0x046d0401, XBOX), (0x046d1000, XBOX), (0x046d1004, XBOX),
    (0x046d1007, XBOX), (0x046d1008, XBOX), (0x046dc21d, XBOX),
    (0x046dc21e, XBOX), (0x046dc21f, XBOX), (0x046dc242, XBOX),
    (0x046dc261, XBOX), (0x046dcaa3, XBOX), (0x046df301, XBOX),
    (0x054c0268, PS), (0x054c05c4, PS), (0x054c05c5, PS),
    (0x054c09cc, PS), (0x054c0ba0, PS), (0x054c0ce6, PS),
    (0x054c0df2, PS), (0x056e2004, XBOX), (0x056e200f, PS),
    (0x056e2012, XBOX), (0x056e2013, PS), (0x057e2006, NINTENDO),
    (0x057e2007, NINTENDO), (0x057e2008, NINTENDO), (0x057e2009, NINTENDO),
    (0x05b81004, PS), (0x05b81006, PS), (0x06a3f622, PS),
    (0x073802a0, XBOX), (0x07383180, PS), (0x07383250, PS),
    (0x07383481, PS), (0x07384716, XBOX), (0x07384718, XBOX),
    (0x07384726, XBOX), (0x07384728, XBOX), (0x07384736, XBOX),
    (0x07384738, XBOX), (0x07384740, XBOX), (0x07384a01, XBOX),
    (0x07387263, XBOX), (0x07388180, PS), (0x07388250, PS),
    (0x07388384, PS), (0x07388480, PS), (0x07388481, PS),
    (0x07388838, PS), (0x0738b726, XBOX), (0x0738b738, XBOX),
    (0x0738beef, XBOX), (0x0738cb02, XBOX), (0x0738cb03, XBOX),
    (0x0738cb29, XBOX), (0x0738f401, XBOX), (0x0738f738, XBOX),
    (0x08100001, PS), (0x08100003, PS), (0x09250005, PS),
    (0x09258866, PS), (0x09258888, PS), (0x09557210, XBOX),
    (0x0955b400, XBOX), (0x0c120e10, PS), (0x0c120e13, PS),
    (0x0c120e15, PS), (0x0c120e17, XBOX), (0x0c120e1c, XBOX),
    (0x0c120e20, PS), (0x0c120e22, XBOX), (0x0c120e30, XBOX),
    (0x0c120ef6, PS), (0x0c120ef8, XBOX), (0x0c121cf6, PS),
    (0x0c121e10, PS), (0x0d629a1a, XBOX), (0x0d629a1b, XBOX),
    (0x0e000e00, XBOX), (0x0e6f0105, XBOX), (0x0e6f0109, PS),
    (0x0e6f0113, XBOX), (0x0e6f011e, PS), (0x0e6f011f, XBOX),
    (0x0e6f0125, XBOX), (0x0e6f0127, XBOX), (0x0e6f0128, PS),
    (0x0e6f012a, XBOX), (0x0e6f0131, XBOX), (0x0e6f0133, XBOX),
    (0x0e6f0139, XBOX), (0x0e6f013a, XBOX), (0x0e6f013b, XBOX),
    (0x0e6f0143, XBOX), (0x0e6f0145, XBOX), (0x0e6f0146, XBOX),
    (0x0e6f0147, XBOX), (0x0e6f0152, XBOX), (0x0e6f0159, XBOX),
    (0x0e6f015b, XBOX), (0x0e6f015c, XBOX), (0x0e6f015d, XBOX),
    (0x0e6f015f, XBOX), (0x0e6f0160, XBOX), (0x0e6f0161, XBOX),
    (0x0e6f0162, XBOX), (0x0e6f0163, XBOX), (0x0e6f0164, XBOX),
    (0x0e6f0165, XBOX), (0x0e6f0166, XBOX), (0x0e6f0167, XBOX),
    (0x0e6f0180, NINTENDO), (0x0e6f0181, NINTENDO), (0x0e6f0184, NINTENDO),
    (0x0e6f0185, NINTENDO), (0x0e6f0186, NINTENDO), (0x0e6f0187, NINTENDO),
    (0x0e6f0188, NINTENDO), (0x0e6f0201, XBOX), (0x0e6f0203, PS),
    (0x0e6f0205, XBOX), (0x0e6f0206, XBOX), (0x0e6f0207, PS),
    (0x0e6f0213, XBOX), (0x0e6f0214, PS), (0x0e6f021f, XBOX),
    (0x0e6f0246, XBOX), (0x0e6f0261, XBOX), (0x0e6f0262, XBOX),
    (0x0e6f02a0, XBOX), (0x0e6f02a1, XBOX), (0x0e6f02a2, XBOX),
    (0x0e6f02a3, XBOX), (0x0e6f02a4, XBOX), (0x0e6f02a5, XBOX),
    (0x0e6f02a6, XBOX), (0x0e6f02a7, XBOX), (0x0e6f02a8, XBOX),
    (0x0e6f02a9, XBOX), (0x0e6f02aa, XBOX), (0x0e6f02ab, XBOX),
    (0x0e6f02ac, XBOX), (0x0e6f02ad, XBOX), (0x0e6f02ae, XBOX),
    (0x0e6f02af, XBOX), (0x0e6f02b0, XBOX), (0x0e6f02b1, XBOX),
    (0x0e6f02b2, XBOX), (0x0e6f02b3, XBOX), (0x0e6f02b5, XBOX),
    (0x0e6f02b6, XBOX), (0x0e6f02b8, XBOX), (0x0e6f02bd, XBOX),
    (0x0e6f02be, XBOX), (0x0e6f02bf, XBOX), (0x0e6f02c0, XBOX),
    (0x0e6f02c1, XBOX), (0x0e6f02c2, XBOX), (0x0e6f02c3, XBOX),
    (0x0e6f02c4, XBOX), (0x0e6f02c5, XBOX), (0x0e6f02c6, XBOX),
    (0x0e6f02c7, XBOX), (0x0e6f02c8, XBOX), (0x0e6f02c9, XBOX),
    (0x0e6f02ca, XBOX), (0x0e6f02cb, XBOX), (0x0e6f02cd, XBOX),
    (0x0e6f02ce, XBOX), (0x0e6f02cf, XBOX), (0x0e6f02d5, XBOX),
    (0x0e6f02d6, XBOX), (0x0e6f02d9, XBOX), (0x0e6f02da, XBOX),
    (0x0e6f0301, XBOX), (0x0e6f0313, XBOX), (0x0e6f0314, XBOX),
    (0x0e6f0346, XBOX), (0x0e6f0401, XBOX), (0x0e6f0413, XBOX),
    (0x0e6f0446, XBOX), (0x0e6f0501, XBOX), (0x0e6f1314, PS),
    (0x0e6f1414, XBOX), (0x0e6f6302, PS), (0x0e6ff501, XBOX),
    (0x0e6ff900, XBOX), (0x0e8f0008, PS), (0x0e8f3075, PS),
    (0x0e8f310d, PS), (0x0f0d0009, PS), (0x0f0d000a, XBOX),
    (0x0f0d000c, XBOX), (0x0f0d000d, XBOX), (0x0f0d0016, XBOX),
    (0x0f0d001b, XBOX), (0x0f0d004d, PS), (0x0f0d0055, PS),
    (0x0f0d005e, PS), (0x0f0d005f, PS), (0x0f0d0063, XBOX),
    (0x0f0d0066, PS), (0x0f0d0067, XBOX), (0x0f0d006a, PS),
    (0x0f0d006d, XBOX), (0x0f0d006e, PS), (0x0f0d0078, XBOX),
    (0x0f0d0084, PS), (0x0f0d0085, PS), (0x0f0d0086, PS),
    (0x0f0d0087, PS), (0x0f0d0088, PS), (0x0f0d008a, PS),
    (0x0f0d008c, XBOX), (0x0f0d0092, NINTENDO), (0x0f0d0097, XBOX),
    (0x0f0d009c, PS), (0x0f0d00a0, PS), (0x0f0d00a4, XBOX),
    (0x0f0d00aa, NINTENDO), (0x0f0d00ae, XBOX), (0x0f0d00b1, XBOX),
    (0x0f0d00ba, XBOX), (0x0f0d00c0, XBOX), (0x0f0d00c1, NINTENDO),
    (0x0f0d00c5, XBOX), (0x0f0d00d8, XBOX), (0x0f0d00db, XBOX),
    (0x0f0d00ee, PS), (0x0f0d00f6, NINTENDO), (0x0f0d011c, PS),
    (0x0f0d011e, XBOX), (0x0f0d0123, PS), (0x0f0d0150, XBOX),
    (0x0f0d0162, PS), (0x0f0d0163, PS), (0x0f0d0184, PS),
    (0x0f301100, PS), (0x0fff02a1, XBOX), (0x10381430, XBOX),
    (0x10381431, XBOX), (0x1038b360, XBOX), (0x10f57009, XBOX),
    (0x10f57013, XBOX), (0x11c04001, PS), (0x11c955f0, XBOX),
    (0x11ff0511, XBOX), (0x11ff3331, PS), (0x12ab0004, XBOX),
    (0x12ab0301, XBOX), (0x12ab0303, XBOX), (0x12ab0304, XBOX),
    (0x13451000, PS), (0x13456005, PS), (0x13456006, XBOX),
    (0x14300291, XBOX), (0x143002a0, XBOX), (0x143002a9, XBOX),
    (0x1430070b, XBOX), (0x14300719, XBOX), (0x14304748, XBOX),
    (0x1430f801, XBOX), (0x146b0601, XBOX), (0x146b0602, XBOX),
    (0x146b0611, XBOX), (0x146b0d01, PS), (0x146b0d02, PS),
    (0x146b0d06, PS), (0x146b0d08, PS), (0x146b0d09, PS),
    (0x146b0d10, PS), (0x146b0d13, PS), (0x146b1103, PS),
    (0x146b5500, PS), (0x15320a00, XBOX), (0x15320a03, XBOX),
    (0x15320a14, XBOX), (0x15320a15, XBOX), (0x15321000, PS),
    (0x15321004, PS), (0x15321007, PS), (0x15321008, PS),
    (0x15321009, PS), (0x1532100a, PS), (0x1532100b, PS),
    (0x1532100c, PS), (0x15321100, PS), (0x15e43f00, XBOX),
    (0x15e43f0a, XBOX), (0x15e43f10, XBOX), (0x162ebeef, XBOX),
    (0x1689fd00, XBOX), (0x1689fd01, XBOX), (0x1689fe00, XBOX),
    (0x16d00f3f, XBOX), (0x1949041a, XBOX), (0x1a340836, PS),
    (0x1bad0002, XBOX), (0x1bad0003, XBOX), (0x1bad028e, XBOX),
    (0x1bad02a0, XBOX), (0x1bad5500, XBOX), (0x1badf016, XBOX),
    (0x1badf018, XBOX), (0x1badf019, XBOX), (0x1badf021, XBOX),
    (0x1badf023, XBOX), (0x1badf025, XBOX), (0x1badf027, XBOX),
    (0x1badf028, XBOX), (0x1badf02e, XBOX), (0x1badf036, XBOX),
    (0x1badf038, XBOX), (0x1badf039, XBOX), (0x1badf03a, XBOX),
    (0x1badf03d, XBOX), (0x1badf03e, XBOX), (0x1badf03f, XBOX),
    (0x1badf042, XBOX), (0x1badf080, XBOX), (0x1badf501, XBOX),
    (0x1badf502, XBOX), (0x1badf503, XBOX), (0x1badf504, XBOX),
    (0x1badf505, XBOX), (0x1badf506, XBOX), (0x1badf900, XBOX),
    (0x1badf901, XBOX), (0x1badf902, XBOX), (0x1badf903, XBOX),
    (0x1badf904, XBOX), (0x1badf906, XBOX), (0x1badfa01, XBOX),
    (0x1badfd00, XBOX), (0x1badfd01, XBOX), (0x20ab55ef, XBOX),
    (0x20bc5500, PS), (0x20d62001, XBOX), (0x20d62002, XBOX),
    (0x20d62003, XBOX), (0x20d62004, XBOX), (0x20d62005, XBOX),
    (0x20d62006, XBOX), (0x20d62009, XBOX), (0x20d6200a, XBOX),
    (0x20d6200b, XBOX), (0x20d6200c, XBOX), (0x20d6200d, XBOX),
    (0x20d6200e, XBOX), (0x20d6200f, XBOX), (0x20d62011, XBOX),
    (0x20d62012, XBOX), (0x20d62015, XBOX), (0x20d62016, XBOX),
    (0x20d62017, XBOX), (0x20d62018, XBOX), (0x20d62019, XBOX),
    (0x20d6201a, XBOX), (0x20d64001, XBOX), (0x20d64002, XBOX),
    (0x20d6576d, PS), (0x20d6792a, PS), (0x20d6890b, XBOX),
    (0x20d6a711, NINTENDO), (0x20d6a712, NINTENDO), (0x20d6a713, NINTENDO),
    (0x20d6a714, NINTENDO), (0x20d6a715, NINTENDO), (0x20d6a716, NINTENDO),
    (0x20d6ca6d, PS), (0x24c65000, XBOX), (0x24c65300, XBOX),
    (0x24c65303, XBOX), (0x24c6530a, XBOX), (0x24c6531a, XBOX),
    (0x24c65397, XBOX), (0x24c6541a, XBOX), (0x24c6542a, XBOX),
    (0x24c6543a, XBOX), (0x24c65500, XBOX), (0x24c65501, XBOX),
    (0x24c65502, XBOX), (0x24c65503, XBOX), (0x24c65506, XBOX),
    (0x24c65508, XBOX), (0x24c65509, XBOX), (0x24c6550d, XBOX),
    (0x24c6550e, XBOX), (0x24c65510, XBOX), (0x24c6551a, XBOX),
    (0x24c6561a, XBOX), (0x24c6581a, XBOX), (0x24c6591a, XBOX),
    (0x24c6592a, XBOX), (0x24c65b00, XBOX), (0x24c65b02, XBOX),
    (0x24c65b03, XBOX), (0x24c65d04, XBOX), (0x24c6791a, XBOX),
    (0x24c6fafa, XBOX), (0x24c6fafb, XBOX), (0x24c6fafc, XBOX),
    (0x24c6fafd, XBOX), (0x24c6fafe, XBOX), (0x24c6faff, XBOX),
    (0x25160069, XBOX), (0x25630523, PS), (0x25630575, PS),
    (0x25b10360, XBOX), (0x25f083c3, PS), (0x25f0c121, PS),
    (0x2c222000, PS), (0x2c222003, PS), (0x2c222203, XBOX),
    (0x2c222300, PS), (0x2c222302, PS), (0x2c222500, PS),
    (0x2c222502, PS), (0x2dc82002, XBOX), (0x2e240652, XBOX),
    (0x2e241618, XBOX), (0x2e241688, XBOX), (0x2f240011, XBOX),
    (0x2f24002e, XBOX), (0x2f240050, XBOX), (0x2f240053, XBOX),
    (0x2f24008f, XBOX), (0x2f240091, XBOX), (0x2f2400b7, XBOX),
    (0x358a0104, PS), (0x75450104, PS), (0x83800003, PS),
    (0x88880308, PS), (0x98860025, PS), (0xd2d2d2d2, XBOX),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_sorted_for_its_search() {
        assert!(CONTROLLERS.windows(2).all(|pair| pair[0].0 < pair[1].0));
    }

    #[test]
    fn known_controllers_have_their_kind() {
        // Sony DualShock 4, Microsoft Xbox One S, Nintendo Switch Pro, and
        // an Xbox-protocol pad of another vendor (PowerA).
        assert_eq!(kind(0x054c, 0x05c4), PS);
        assert_eq!(kind(0x045e, 0x02ea), XBOX);
        assert_eq!(kind(0x057e, 0x2009), NINTENDO);
        assert_eq!(kind(0x24c6, 0x543a), XBOX);
        assert_eq!(kind(0x1209, 0x0001), UNKNOWN);
    }

    #[test]
    fn paddles_and_share_follow_sdl() {
        assert!(has_paddles(0x045e, 0x0b00));
        assert!(has_paddles(0x054c, 0x0df2));
        assert!(!has_paddles(0x045e, 0x0b12));
        assert!(has_share(0x045e, 0x0b13));
        assert!(has_share(0x20d6, 0x2005));
        assert!(!has_share(0x045e, 0x02ea));
    }
}
