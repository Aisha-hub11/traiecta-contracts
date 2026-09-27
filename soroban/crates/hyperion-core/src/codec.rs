//! Turning a raw thirty two byte key back into an address you can actually pay.
//!
//! Every rail here eventually hands us a bare key. Circle's mint recipient is thirty two bytes
//! with no room for anything else, and an EVM contract encoding a Stellar destination has no
//! `Address` type to reach for. Somewhere that has to become a real Soroban `Address`, and the
//! only route the host offers is through a strkey string.
//!
//! So this module builds one: version byte, payload, CRC16 over both, base32 over the lot. That
//! is the whole SEP-23 format. Doing it here rather than trusting a caller to pass the string
//! matters because the checksum is the only thing standing between a flipped bit and a payment
//! to an address nobody holds the secret for. We compute it ourselves, the host verifies it on
//! the way in, and a key that survives both is a key that came through intact.

use soroban_sdk::{xdr::ToXdr, Address, Env, MuxedAddress, String};

use crate::address::{AddressKind, StellarDestination};
use crate::error::HyperionError;

/// RFC 4648 base32 alphabet. No padding, because strkeys do not use any.
const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// Version byte for a `G` account key, per SEP-23.
pub const VERSION_ACCOUNT: u8 = 6 << 3;
/// Version byte for a `C` contract key.
pub const VERSION_CONTRACT: u8 = 2 << 3;
/// Version byte for an `M` muxed account key.
pub const VERSION_MUXED: u8 = 12 << 3;

/// Version byte, thirty two byte key, eight byte muxed id, two byte checksum.
const MAX_RAW: usize = 1 + 32 + 8 + 2;
/// Every five bits of the above becomes one character, rounded up.
const MAX_CHARS: usize = 69;

/// CRC16 with the XModem parameters: polynomial 0x1021, zero initial value, no final xor.
///
/// This is the checksum Stellar chose for strkeys, and it is the reason a mistyped or corrupted
/// address is rejected rather than silently pointed somewhere else.
fn crc16(data: &[u8]) -> u16 {
    let mut crc: u16 = 0;
    for &byte in data {
        crc ^= (byte as u16) << 8;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
            bit += 1;
        }
    }
    crc
}

/// Base32 encode `input` into `out`, returning how many characters were written.
fn base32_encode(input: &[u8], out: &mut [u8]) -> usize {
    let mut accumulator: u32 = 0;
    let mut pending: u32 = 0;
    let mut written = 0usize;

    for &byte in input {
        accumulator = (accumulator << 8) | byte as u32;
        pending += 8;
        while pending >= 5 {
            pending -= 5;
            out[written] = ALPHABET[((accumulator >> pending) & 0x1F) as usize];
            written += 1;
        }
    }
    // A trailing group shorter than five bits is padded with zeroes on the right, which is what
    // makes a muxed strkey sixty nine characters rather than sixty eight and a bit.
    if pending > 0 {
        out[written] = ALPHABET[((accumulator << (5 - pending)) & 0x1F) as usize];
        written += 1;
    }
    written
}

/// Assemble a strkey from its version byte and payload.
fn strkey(env: &Env, version: u8, key: &[u8; 32], muxed_id: Option<u64>) -> String {
    let mut raw = [0u8; MAX_RAW];
    raw[0] = version;
    raw[1..33].copy_from_slice(key);

    let body_len = match muxed_id {
        Some(id) => {
            raw[33..41].copy_from_slice(&id.to_be_bytes());
            41
        }
        None => 33,
    };

    let checksum = crc16(&raw[..body_len]);
    // Little endian, which is the one part of the format that surprises people.
    raw[body_len] = checksum as u8;
    raw[body_len + 1] = (checksum >> 8) as u8;

    let mut chars = [0u8; MAX_CHARS];
    let written = base32_encode(&raw[..body_len + 2], &mut chars);
    String::from_bytes(env, &chars[..written])
}

/// The `G...` strkey for a raw account key.
pub fn account_strkey(env: &Env, key: &[u8; 32]) -> String {
    strkey(env, VERSION_ACCOUNT, key, None)
}

/// The `C...` strkey for a raw contract key.
pub fn contract_strkey(env: &Env, key: &[u8; 32]) -> String {
    strkey(env, VERSION_CONTRACT, key, None)
}

/// The `M...` strkey for a raw account key carrying a muxed id.
pub fn muxed_strkey(env: &Env, key: &[u8; 32], muxed_id: u64) -> String {
    strkey(env, VERSION_MUXED, key, Some(muxed_id))
}

impl StellarDestination {
    /// The strkey this destination describes.
    ///
    /// Validated first, so an all-zero key or a muxed id hiding on a plain account never gets as
    /// far as producing a string that looks legitimate.
    pub fn to_strkey(&self, env: &Env) -> Result<String, HyperionError> {
        self.validate()?;
        let key = self.key.to_array();
        Ok(match self.kind {
            AddressKind::Account => account_strkey(env, &key),
            AddressKind::Contract => contract_strkey(env, &key),
            AddressKind::MuxedAccount => muxed_strkey(env, &key, self.muxed_id),
        })
    }

    /// The address funds should actually be sent to.
    ///
    /// A muxed destination collapses to its underlying account here. The muxed id is a routing
    /// hint for whoever operates that account, not a separate balance, so the transfer goes to
    /// the base account either way and the id travels in the event trail.
    pub fn to_address(&self, env: &Env) -> Result<Address, HyperionError> {
        let encoded = self.to_strkey(env)?;
        Ok(match self.kind {
            AddressKind::MuxedAccount => MuxedAddress::from_string(&encoded).address(),
            _ => Address::from_string(&encoded),
        })
    }

    /// The muxed form, for the rails whose payload is wide enough to carry one.
    pub fn to_muxed(&self, env: &Env) -> Result<MuxedAddress, HyperionError> {
        let encoded = self.to_strkey(env)?;
        Ok(match self.kind {
            AddressKind::MuxedAccount => MuxedAddress::from_string(&encoded),
            _ => Address::from_string(&encoded).into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, BytesN, Env};

    #[test]
    fn a_known_account_key_encodes_to_the_strkey_stellar_publishes_for_it() {
        let env = Env::default();
        // SEP-23's own worked example: a payload of all zeroes.
        let encoded = account_strkey(&env, &[0u8; 32]);
        assert_eq!(
            encoded,
            String::from_str(
                &env,
                "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF"
            )
        );
    }

    #[test]
    fn the_host_reencodes_our_strkeys_to_exactly_the_same_characters() {
        let env = Env::default();
        // The host has Stellar's own strkey implementation behind it, and it refuses a bad
        // checksum outright. Handing it ours and asking it to write the address back out is
        // therefore a real cross-check against the reference encoder rather than against a
        // literal somebody typed in from a spec page, which is where most of these tests go
        // wrong. If our bit packing or our CRC were off by anything at all, one of these three
        // would either panic on the way in or come back different.
        for key in [[0u8; 32], [0xFFu8; 32], [0x2Cu8; 32]] {
            let account = account_strkey(&env, &key);
            assert_eq!(Address::from_string(&account).to_string(), account);

            let contract = contract_strkey(&env, &key);
            assert_eq!(Address::from_string(&contract).to_string(), contract);

            for id in [0u64, 1, u64::MAX] {
                let muxed = muxed_strkey(&env, &key, id);
                let parsed = MuxedAddress::from_string(&muxed);
                assert_eq!(parsed.to_strkey(), muxed);
                assert_eq!(parsed.id(), Some(id));
            }
        }
    }

    #[test]
    fn the_version_byte_decides_the_first_character() {
        let env = Env::default();
        let key = [0x11u8; 32];
        let account = account_strkey(&env, &key);
        let contract = contract_strkey(&env, &key);
        let muxed = muxed_strkey(&env, &key, 7);

        let mut buf = [0u8; 69];
        account.copy_into_slice(&mut buf[..56]);
        assert_eq!(buf[0], b'G');
        contract.copy_into_slice(&mut buf[..56]);
        assert_eq!(buf[0], b'C');
        muxed.copy_into_slice(&mut buf[..69]);
        assert_eq!(buf[0], b'M');
    }

    #[test]
    fn plain_strkeys_are_fifty_six_characters_and_muxed_ones_are_sixty_nine() {
        let env = Env::default();
        let key = [0x5Au8; 32];
        assert_eq!(account_strkey(&env, &key).len(), 56);
        assert_eq!(contract_strkey(&env, &key).len(), 56);
        assert_eq!(muxed_strkey(&env, &key, u64::MAX).len(), 69);
    }

    #[test]
    fn an_address_survives_a_round_trip_through_thirty_two_bytes() {
        let env = Env::default();
        // Generated addresses are contracts in the test environment, which is the case that
        // matters most: a C key arriving in a rail's recipient field has to come back as the
        // same contract or an inbound delivery pays a stranger.
        let original = Address::generate(&env);
        let as_string = original.to_string();

        let mut chars = [0u8; 56];
        as_string.copy_into_slice(&mut chars);
        assert_eq!(chars[0], b'C');

        // Take the raw key back out of the strkey the host produced, then rebuild it ourselves.
        let key = decode_payload(&chars);
        let rebuilt = Address::from_string(&contract_strkey(&env, &key));
        assert_eq!(rebuilt, original);
    }

    #[test]
    fn a_destination_becomes_the_address_it_describes() {
        let env = Env::default();
        let original = Address::generate(&env);
        let mut chars = [0u8; 56];
        original.to_string().copy_into_slice(&mut chars);
        let key = decode_payload(&chars);

        let dest = StellarDestination::contract(BytesN::from_array(&env, &key));
        assert_eq!(dest.to_address(&env).unwrap(), original);
    }

    #[test]
    fn a_muxed_destination_pays_the_account_underneath_it() {
        let env = Env::default();
        let key = [0x2Cu8; 32];
        let muxed = StellarDestination::muxed(BytesN::from_array(&env, &key), 900);
        let plain = StellarDestination::account(BytesN::from_array(&env, &key));

        // Same money, same account. The id is a label on the payment, not a different pocket.
        assert_eq!(
            muxed.to_address(&env).unwrap(),
            plain.to_address(&env).unwrap()
        );
        assert_eq!(muxed.to_muxed(&env).unwrap().id(), Some(900));
        assert_eq!(plain.to_muxed(&env).unwrap().id(), None);
    }

    #[test]
    fn a_destination_that_fails_validation_never_produces_a_string() {
        let env = Env::default();
        let zero = StellarDestination::account(BytesN::from_array(&env, &[0u8; 32]));
        assert_eq!(zero.to_strkey(&env), Err(HyperionError::ZeroAddressKey));
    }

    #[test]
    fn the_checksum_covers_the_muxed_id_as_well_as_the_key() {
        let env = Env::default();
        let key = [0x99u8; 32];
        // If the id were outside the checksum these two would differ only in the middle and
        // share their last two characters.
        let one = muxed_strkey(&env, &key, 1);
        let two = muxed_strkey(&env, &key, 2);
        let mut a = [0u8; 69];
        let mut b = [0u8; 69];
        one.copy_into_slice(&mut a);
        two.copy_into_slice(&mut b);
        assert_ne!(a[60..], b[60..]);
    }

    /// Base32 decode the 32 byte payload out of a 56 character plain strkey.
    fn decode_payload(chars: &[u8; 56]) -> [u8; 32] {
        let mut accumulator: u32 = 0;
        let mut pending: u32 = 0;
        let mut raw = [0u8; 35];
        let mut written = 0usize;
        for &c in chars.iter() {
            let value = ALPHABET.iter().position(|&a| a == c).expect("base32") as u32;
            accumulator = (accumulator << 5) | value;
            pending += 5;
            if pending >= 8 {
                pending -= 8;
                raw[written] = ((accumulator >> pending) & 0xFF) as u8;
                written += 1;
            }
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(&raw[1..33]);
        key
    }
}

// ------------------------------------------------------------------------------------------
// The other direction
// ------------------------------------------------------------------------------------------

/// XDR discriminant for `ScVal::Address`.
const SCV_ADDRESS: u8 = 18;
/// XDR discriminant for `ScAddress::Account`.
const SC_ADDRESS_ACCOUNT: u8 = 0;
/// XDR discriminant for `ScAddress::Contract`.
const SC_ADDRESS_CONTRACT: u8 = 1;
/// XDR discriminant for `PublicKey::PublicKeyTypeEd25519`.
const PUBLIC_KEY_ED25519: u8 = 0;

/// Read the raw 32 byte key out of an address.
///
/// The inverse of [`contract_strkey`] and [`account_strkey`], and the thing you need whenever a
/// rail wants a bare key rather than a strkey: CCTP's mint recipient, Axelar's raw destination
/// bytes, Allbridge's recipient word.
///
/// This goes through the host's own XDR rather than decoding base32 by hand, so the host stays the
/// authority on what an address is. The layout is a tagged union all the way down, which is what
/// the offsets below are walking: an `ScVal` tag, then an `ScAddress` tag, then for an account one
/// more tag for the key algorithm before the key itself. Anything that is not a plain ed25519
/// account or a contract is refused rather than trimmed to 32 bytes and hoped over.
pub fn address_key(env: &Env, addr: &Address) -> Result<(AddressKind, [u8; 32]), HyperionError> {
    let raw = addr.to_xdr(env);
    let tag = |i: u32| -> Result<u8, HyperionError> {
        // Every discriminant here is a four byte big-endian integer whose value is small, so the
        // low byte is the whole story and the three above it have to be zero.
        for j in i..i + 3 {
            if raw.get(j).ok_or(HyperionError::InvalidDestination)? != 0 {
                return Err(HyperionError::InvalidDestination);
            }
        }
        raw.get(i + 3).ok_or(HyperionError::InvalidDestination)
    };
    if tag(0)? != SCV_ADDRESS {
        return Err(HyperionError::InvalidDestination);
    }
    let (kind, offset) = match tag(4)? {
        SC_ADDRESS_CONTRACT => (AddressKind::Contract, 8u32),
        SC_ADDRESS_ACCOUNT => {
            if tag(8)? != PUBLIC_KEY_ED25519 {
                return Err(HyperionError::InvalidDestination);
            }
            (AddressKind::Account, 12u32)
        }
        _ => return Err(HyperionError::InvalidDestination),
    };
    if raw.len() != offset + 32 {
        return Err(HyperionError::InvalidDestination);
    }
    let mut key = [0u8; 32];
    for (i, slot) in key.iter_mut().enumerate() {
        *slot = raw
            .get(offset + i as u32)
            .ok_or(HyperionError::InvalidDestination)?;
    }
    Ok((kind, key))
}

/// The 32 byte key of an address that has to be a contract.
pub fn contract_key(env: &Env, addr: &Address) -> Result<[u8; 32], HyperionError> {
    match address_key(env, addr)? {
        (AddressKind::Contract, key) => Ok(key),
        _ => Err(HyperionError::NotEvmAddress),
    }
}

#[cfg(test)]
mod key_tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    #[test]
    fn a_contract_address_survives_the_trip_out_to_raw_bytes_and_back() {
        let env = Env::default();
        // `Address::generate` hands back a contract address, which is the common case here
        // anyway: every address Hyperion puts on a wire is a contract or an account it was
        // given, never one it invented.
        let contract = Address::generate(&env);
        let (kind, key) = address_key(&env, &contract).unwrap();
        assert_eq!(kind, AddressKind::Contract);
        assert_eq!(Address::from_string(&contract_strkey(&env, &key)), contract);
        assert_eq!(contract_key(&env, &contract).unwrap(), key);
    }

    #[test]
    fn an_account_address_survives_the_same_trip() {
        let env = Env::default();
        let raw = [0x3Cu8; 32];
        let account = Address::from_string(&account_strkey(&env, &raw));
        let (kind, key) = address_key(&env, &account).unwrap();
        assert_eq!(kind, AddressKind::Account);
        assert_eq!(key, raw);
    }

    #[test]
    fn an_account_is_not_mistaken_for_a_contract() {
        let env = Env::default();
        let account = Address::from_string(&account_strkey(&env, &[9u8; 32]));
        assert_eq!(
            contract_key(&env, &account),
            Err(HyperionError::NotEvmAddress)
        );
    }

    #[test]
    fn the_two_kinds_of_address_do_not_collide_on_the_same_key() {
        let env = Env::default();
        let raw = [0x77u8; 32];
        let as_account = Address::from_string(&account_strkey(&env, &raw));
        let as_contract = Address::from_string(&contract_strkey(&env, &raw));
        // Same thirty two bytes, two different addresses. This is exactly the ambiguity a bare
        // key field carries, and the reason the tagged encoding in `address` exists at all.
        assert_ne!(as_account, as_contract);
        assert_eq!(address_key(&env, &as_account).unwrap().1, raw);
        assert_eq!(address_key(&env, &as_contract).unwrap().1, raw);
    }
}
