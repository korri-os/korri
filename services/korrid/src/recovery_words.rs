//! Recovery words (ADR 0072): the 12 words a person key is made from.
//!
//! The words are BIP39 English words. The person key is made from them as
//! Nostr's NIP-06 makes a key: account 0, with no passphrase (ADR 0082).
//! The person confirms the words by typing some of them back.
//!
//! Every value here that holds words or a key erases its memory when it is
//! dropped. The libraries erase what they can: `bip39` erases a parsed
//! mnemonic and `nostr` erases a secret key. The seed and the BIP32 steps
//! inside `nostr`'s NIP-06 code are not erased; the library gives no way to.

use bip39::{Language, Mnemonic};
use nostr::{key::Keys, nips::nip06::FromMnemonic};
use rand::{seq::index, CryptoRng, Rng, TryRngCore};
use zeroize::Zeroizing;

/// How many recovery words make one person key.
pub const WORD_COUNT: usize = 12;
/// How many words the person types back to confirm the words.
pub const CONFIRMATION_WORDS: usize = 3;
/// 128 bits of entropy make 12 BIP39 words.
const ENTROPY_BYTES: usize = 16;
/// The longest BIP39 English word has 8 letters. This holds 24 words, so the
/// 24-word NIP-06 test vector fits too.
const MAX_PHRASE_BYTES: usize = 24 * 9;

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum RecoveryWordsError {
    #[error("Recovery words are 12 words. These are {found}.")]
    WordCount { found: usize },
    /// The word itself is never in the message: it can be one of the words.
    #[error("Word {position} is not a recovery word. Check its spelling.")]
    UnknownWord { position: usize },
    #[error("These 12 words are not a set of recovery words. Check each word and its place.")]
    Checksum,
    #[error("Korri could not make a person key from these words.")]
    Derivation,
}

/// Twelve recovery words. Debug never shows them, and the memory that holds
/// them is erased when they are dropped.
pub struct RecoveryWords(Mnemonic);

impl std::fmt::Debug for RecoveryWords {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RecoveryWords")
            .finish_non_exhaustive()
    }
}

impl RecoveryWords {
    /// New words from the operating system's random source.
    pub fn generate() -> Self {
        Self::generate_with(&mut rand::rngs::OsRng.unwrap_err())
    }

    fn generate_with<R: CryptoRng + ?Sized>(rng: &mut R) -> Self {
        let mut entropy = Zeroizing::new([0u8; ENTROPY_BYTES]);
        rng.fill_bytes(entropy.as_mut());
        Self::from_entropy(&entropy)
    }

    fn from_entropy(entropy: &[u8; ENTROPY_BYTES]) -> Self {
        Self(
            Mnemonic::from_entropy_in(Language::English, entropy)
                .expect("16 bytes of entropy always make 12 words"),
        )
    }

    /// Words as a person types or scans them: any case, any spaces between
    /// words. Only 12 English BIP39 words with a valid checksum are words.
    pub fn parse(text: &str) -> Result<Self, RecoveryWordsError> {
        // Lowercasing ASCII and joining with one space never makes the text
        // longer, so this buffer never moves and leaves no copy behind.
        let mut normalized = Zeroizing::new(String::with_capacity(text.len()));
        let mut found = 0;
        for word in text.split_whitespace() {
            if found > 0 {
                normalized.push(' ');
            }
            normalized.extend(word.chars().map(|letter| letter.to_ascii_lowercase()));
            found += 1;
        }
        if found != WORD_COUNT {
            return Err(RecoveryWordsError::WordCount { found });
        }
        match Mnemonic::parse_in_normalized(Language::English, &normalized) {
            Ok(mnemonic) => Ok(Self(mnemonic)),
            Err(bip39::Error::UnknownWord(index)) => Err(RecoveryWordsError::UnknownWord {
                position: index + 1,
            }),
            Err(bip39::Error::InvalidChecksum) => Err(RecoveryWordsError::Checksum),
            Err(_) => Err(RecoveryWordsError::WordCount { found }),
        }
    }

    /// The words in order, lowercase, one space between words and nothing
    /// after the last. The recovery QR code holds exactly this text.
    pub fn phrase(&self) -> Zeroizing<String> {
        phrase_of(&self.0)
    }

    /// The word at a position from 1 to 12.
    pub fn word(&self, position: usize) -> Option<&'static str> {
        position
            .checked_sub(1)
            .and_then(|index| self.0.words().nth(index))
    }

    /// The person key these words make: NIP-06, account 0, no passphrase.
    /// The caller drops it as soon as it has signed; `Keys` erases its
    /// secret key on drop.
    pub fn person_keys(&self) -> Result<Keys, RecoveryWordsError> {
        derive_person_keys(&self.0)
    }
}

fn phrase_of(mnemonic: &Mnemonic) -> Zeroizing<String> {
    let mut phrase = Zeroizing::new(String::with_capacity(MAX_PHRASE_BYTES));
    for (index, word) in mnemonic.words().enumerate() {
        if index > 0 {
            phrase.push(' ');
        }
        phrase.push_str(word);
    }
    phrase
}

/// NIP-06: the key at `m/44'/1237'/0'/0/0` of the BIP39 seed, with an empty
/// passphrase.
fn derive_person_keys(mnemonic: &Mnemonic) -> Result<Keys, RecoveryWordsError> {
    let phrase = phrase_of(mnemonic);
    Keys::from_mnemonic_with_account(phrase.as_str(), None, Some(0))
        .map_err(|_| RecoveryWordsError::Derivation)
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ConfirmationError {
    #[error("Choose 3 different word positions from 1 to 12, smallest first.")]
    Positions,
    #[error("Type {expected} words to confirm. These are {found}.")]
    AnswerCount { expected: usize, found: usize },
    /// The positions of the words that do not match, from 1 to 12.
    #[error("{}", mismatch_message(.positions))]
    Mismatch { positions: Vec<usize> },
}

fn mismatch_message(positions: &[usize]) -> String {
    let list = positions
        .iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    match positions {
        [_] => format!("Word {list} does not match. Check the words you wrote down."),
        _ => format!("Words {list} do not match. Check the words you wrote down."),
    }
}

/// The word positions, from 1 to 12, that the person types back to show
/// that they wrote the words down (ADR 0082). Setup cannot end before the
/// answers match.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Confirmation {
    positions: [usize; CONFIRMATION_WORDS],
}

impl Confirmation {
    /// Three different positions, chosen at random, smallest first.
    pub fn choose() -> Self {
        Self::choose_with(&mut rand::rng())
    }

    fn choose_with<R: Rng + ?Sized>(rng: &mut R) -> Self {
        let mut chosen = index::sample(rng, WORD_COUNT, CONFIRMATION_WORDS).into_vec();
        chosen.sort_unstable();
        let mut positions = [0; CONFIRMATION_WORDS];
        for (position, index) in positions.iter_mut().zip(chosen) {
            *position = index + 1;
        }
        Self { positions }
    }

    /// Positions that came from elsewhere, checked: 3 different positions
    /// from 1 to 12, smallest first.
    pub fn from_positions(
        positions: [usize; CONFIRMATION_WORDS],
    ) -> Result<Self, ConfirmationError> {
        let in_range = positions
            .iter()
            .all(|position| (1..=WORD_COUNT).contains(position));
        let increasing = positions.windows(2).all(|pair| pair[0] < pair[1]);
        if in_range && increasing {
            Ok(Self { positions })
        } else {
            Err(ConfirmationError::Positions)
        }
    }

    pub fn positions(&self) -> [usize; CONFIRMATION_WORDS] {
        self.positions
    }

    /// Check the person's answers, one for each position in order. Case and
    /// the spaces around a word do not matter.
    pub fn check(&self, words: &RecoveryWords, answers: &[&str]) -> Result<(), ConfirmationError> {
        if answers.len() != CONFIRMATION_WORDS {
            return Err(ConfirmationError::AnswerCount {
                expected: CONFIRMATION_WORDS,
                found: answers.len(),
            });
        }
        let positions: Vec<usize> = self
            .positions
            .iter()
            .zip(answers)
            .filter(|(position, answer)| {
                !words
                    .word(**position)
                    .is_some_and(|word| word.eq_ignore_ascii_case(answer.trim()))
            })
            .map(|(position, _)| *position)
            .collect();
        if positions.is_empty() {
            Ok(())
        } else {
            Err(ConfirmationError::Mismatch { positions })
        }
    }
}

#[cfg(test)]
#[path = "recovery_words_tests.rs"]
mod tests;
