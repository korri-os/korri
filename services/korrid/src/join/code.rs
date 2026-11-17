//! The join confirmation code (ADR 0070). It has 10 symbols, and each symbol
//! is a digit from 1 to 6. The first 3 symbols are the public nameplate. The
//! last 7 symbols are the secret. A shown code allows one guess and lasts
//! for a fixed time.

use rand::{seq::IndexedRandom, Rng};
use std::collections::BTreeSet;
use zeroize::Zeroizing;

const NAMEPLATE_SYMBOLS: usize = 3;
const SECRET_SYMBOLS: usize = 7;
const SYMBOLS: u8 = 6;

/// The public part of a code. It tells the new device where on the relays
/// the owned device waits. There are 6^3 = 216 nameplates.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct Nameplate([u8; NAMEPLATE_SYMBOLS]);

impl Nameplate {
    pub(crate) fn all() -> impl Iterator<Item = Self> {
        (0..u16::from(SYMBOLS).pow(NAMEPLATE_SYMBOLS as u32)).map(|index| {
            let symbol = |place: u32| (index / u16::from(SYMBOLS).pow(place) % 6) as u8 + 1;
            Self([symbol(2), symbol(1), symbol(0)])
        })
    }

    pub(crate) fn parse(digits: &str) -> Option<Self> {
        let symbols = parse_symbols::<NAMEPLATE_SYMBOLS>(digits)?;
        Some(Self(symbols))
    }

    pub(crate) fn digits(&self) -> String {
        self.0
            .iter()
            .map(|symbol| char::from(b'0' + symbol))
            .collect()
    }
}

/// One join confirmation code. Its secret never leaves this device except
/// as the person reads it; Debug shows only the nameplate.
#[derive(Clone, Eq, PartialEq)]
pub(crate) struct JoinCode {
    nameplate: Nameplate,
    secret: Zeroizing<[u8; SECRET_SYMBOLS]>,
}

impl std::fmt::Debug for JoinCode {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("JoinCode")
            .field("nameplate", &self.nameplate)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("A join code has 10 digits, each from 1 to 6.")]
pub(crate) struct InvalidCode;

impl JoinCode {
    /// A new code at this nameplate, with a uniformly random secret.
    pub(crate) fn generate(nameplate: Nameplate, rng: &mut impl Rng) -> Self {
        let mut secret = Zeroizing::new([0; SECRET_SYMBOLS]);
        for symbol in secret.iter_mut() {
            *symbol = rng.random_range(1..=SYMBOLS);
        }
        Self { nameplate, secret }
    }

    /// The 10 digits the person typed. Spaces and dashes between them are
    /// allowed, because the code is shown in two groups.
    pub(crate) fn parse(input: &str) -> Result<Self, InvalidCode> {
        let digits: Zeroizing<String> = Zeroizing::new(
            input
                .chars()
                .filter(|character| !matches!(character, ' ' | '-'))
                .collect(),
        );
        if digits.len() != NAMEPLATE_SYMBOLS + SECRET_SYMBOLS || !digits.is_ascii() {
            return Err(InvalidCode);
        }
        let (nameplate, secret) = digits.split_at(NAMEPLATE_SYMBOLS);
        Ok(Self {
            nameplate: Nameplate::parse(nameplate).ok_or(InvalidCode)?,
            secret: Zeroizing::new(parse_symbols::<SECRET_SYMBOLS>(secret).ok_or(InvalidCode)?),
        })
    }

    pub(crate) fn nameplate(&self) -> Nameplate {
        self.nameplate
    }

    /// The secret as CPace's password: its 7 digits as ASCII.
    pub(crate) fn secret(&self) -> Zeroizing<Vec<u8>> {
        Zeroizing::new(self.secret.iter().map(|symbol| b'0' + symbol).collect())
    }

    /// The code as the owned device shows it: the nameplate, a space, and
    /// the secret.
    pub(crate) fn shown(&self) -> String {
        format!(
            "{} {}",
            self.nameplate.digits(),
            String::from_utf8_lossy(&self.secret())
        )
    }
}

/// A free nameplate, picked at random, or None when every one is in use.
pub(crate) fn choose_nameplate(
    in_use: &BTreeSet<Nameplate>,
    rng: &mut impl Rng,
) -> Option<Nameplate> {
    let free: Vec<_> = Nameplate::all()
        .filter(|nameplate| !in_use.contains(nameplate))
        .collect();
    free.choose(rng).copied()
}

/// Why a shown code can take no guess.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CodeEnded {
    Expired,
    Guessed,
}

/// A code the owned device shows. It allows one guess, before it expires.
#[derive(Debug)]
pub(crate) struct ShownCode {
    code: JoinCode,
    expires_at: u64,
    guessed: bool,
}

impl ShownCode {
    pub(crate) fn new(code: JoinCode, expires_at: u64) -> Self {
        Self {
            code,
            expires_at,
            guessed: false,
        }
    }

    pub(crate) fn code(&self) -> &JoinCode {
        &self.code
    }

    pub(crate) fn expires_at(&self) -> u64 {
        self.expires_at
    }

    pub(crate) fn is_expired(&self, now: u64) -> bool {
        now >= self.expires_at
    }

    /// Spend the one guess. A second guess, or a guess after the code
    /// expired, is refused, whatever the first guess was.
    pub(crate) fn take_guess(&mut self, now: u64) -> Result<(), CodeEnded> {
        if self.guessed {
            return Err(CodeEnded::Guessed);
        }
        if self.is_expired(now) {
            return Err(CodeEnded::Expired);
        }
        self.guessed = true;
        Ok(())
    }
}

fn parse_symbols<const N: usize>(digits: &str) -> Option<[u8; N]> {
    let bytes = digits.as_bytes();
    if bytes.len() != N {
        return None;
    }
    let mut symbols = [0; N];
    for (symbol, byte) in symbols.iter_mut().zip(bytes) {
        *symbol = match byte {
            b'1'..=b'6' => byte - b'0',
            _ => return None,
        };
    }
    Some(symbols)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{rngs::StdRng, SeedableRng};

    #[test]
    fn a_generated_code_has_ten_symbols_from_one_to_six() {
        let mut rng = StdRng::seed_from_u64(7);
        let nameplate = Nameplate::parse("163").unwrap();
        let mut seen = BTreeSet::new();
        for _ in 0..500 {
            let code = JoinCode::generate(nameplate, &mut rng);
            let digits: String = code.shown().split(' ').collect();
            assert_eq!(digits.len(), 10);
            assert!(digits.bytes().all(|byte| (b'1'..=b'6').contains(&byte)));
            assert!(digits.starts_with("163"));
            assert_eq!(JoinCode::parse(&digits).unwrap(), code);
            seen.extend(digits[3..].bytes());
        }
        // Every symbol appears in the secrets, so no digit is left out.
        assert_eq!(seen, (b'1'..=b'6').collect());
    }

    #[test]
    fn a_code_parses_only_ten_digits_from_one_to_six() {
        let code = JoinCode::parse("123 4561234").unwrap();
        assert_eq!(code.nameplate().digits(), "123");
        assert_eq!(&**code.secret(), b"4561234");
        assert_eq!(JoinCode::parse("123-456-1234").unwrap(), code);
        assert_eq!(code.shown(), "123 4561234");
        for wrong in [
            "",
            "123456123",
            "12345612345",
            "1234561237",
            "0234561234",
            "123456123a",
            "１２３４５６１２３４",
            "123\t4561234",
        ] {
            assert_eq!(JoinCode::parse(wrong), Err(InvalidCode), "{wrong:?}");
        }
    }

    #[test]
    fn debug_never_shows_the_secret() {
        let code = JoinCode::parse("1234561234").unwrap();
        let shown = format!("{code:?}");
        assert!(!shown.contains("4561234"), "{shown}");
    }

    #[test]
    fn there_are_216_distinct_nameplates() {
        let all: BTreeSet<_> = Nameplate::all().collect();
        assert_eq!(all.len(), 216);
        assert!(all
            .iter()
            .all(|nameplate| Nameplate::parse(&nameplate.digits()) == Some(*nameplate)));
    }

    #[test]
    fn the_nameplate_is_one_that_no_other_join_uses() {
        let mut rng = StdRng::seed_from_u64(3);
        let mut in_use: BTreeSet<_> = Nameplate::all().collect();
        let free = Nameplate::parse("425").unwrap();
        in_use.remove(&free);
        for _ in 0..20 {
            assert_eq!(choose_nameplate(&in_use, &mut rng), Some(free));
        }
        in_use.insert(free);
        assert_eq!(choose_nameplate(&in_use, &mut rng), None);
        let chosen: BTreeSet<_> = (0..200)
            .filter_map(|_| choose_nameplate(&BTreeSet::new(), &mut rng))
            .collect();
        assert!(
            chosen.len() > 100,
            "nameplates are spread: {}",
            chosen.len()
        );
    }

    #[test]
    fn a_shown_code_allows_one_guess() {
        let mut shown = ShownCode::new(JoinCode::parse("1234561234").unwrap(), 400);
        assert_eq!(shown.take_guess(100), Ok(()));
        assert_eq!(shown.take_guess(101), Err(CodeEnded::Guessed));
        assert_eq!(shown.take_guess(500), Err(CodeEnded::Guessed));
    }

    #[test]
    fn a_shown_code_expires() {
        let mut shown = ShownCode::new(JoinCode::parse("1234561234").unwrap(), 400);
        assert!(!shown.is_expired(399));
        assert!(shown.is_expired(400));
        assert_eq!(shown.take_guess(400), Err(CodeEnded::Expired));
        // An expired code spent no guess; it stays expired.
        assert_eq!(shown.take_guess(401), Err(CodeEnded::Expired));
    }
}
