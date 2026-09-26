use soroban_sdk::{contracttype, Bytes, BytesN, Env};

use crate::error::HyperionError;
use crate::route::RouteKind;

/// Tag byte for a classic ed25519 account, the kind that starts with G.
pub const KIND_ACCOUNT: u8 = 0;
/// Tag byte for a Soroban contract, the kind that starts with C.
pub const KIND_CONTRACT: u8 = 1;
/// Tag byte for a muxed account, the kind that starts with M. Carries an extra 8 byte id.
pub const KIND_MUXED: u8 = 2;

/// Length of a tagged destination with no muxed id: one tag byte plus a 32 byte key.
pub const TAGGED_LEN_PLAIN: u32 = 33;
/// Length of a tagged muxed destination: tag, 32 byte key, then a big-endian u64 id.
pub const TAGGED_LEN_MUXED: u32 = 41;

/// Which flavour of Stellar address a destination refers to.
///
/// This distinction has no equivalent on the EVM side, where every account is a plain 20
/// byte value. A raw 32 byte cross-chain field carries no type information whatsoever, so
/// something has to say which of the three this is. That something is this enum, carried
/// explicitly in Hyperion's own tagged encoding.
#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum AddressKind {
    /// Classic ed25519 account, G prefix. Needs a trustline before it can hold an asset.
    Account = 0,
    /// Soroban contract, C prefix. No trustline concept applies.
    Contract = 1,
    /// Muxed account, M prefix. One of many virtual sub-accounts sharing a single G
    /// account, per SEP-23. Exchanges lean on these heavily.
    MuxedAccount = 2,
}

impl AddressKind {
    pub fn tag(&self) -> u8 {
        match self {
            AddressKind::Account => KIND_ACCOUNT,
            AddressKind::Contract => KIND_CONTRACT,
            AddressKind::MuxedAccount => KIND_MUXED,
        }
    }

    pub fn from_tag(tag: u8) -> Option<AddressKind> {
        match tag {
            KIND_ACCOUNT => Some(AddressKind::Account),
            KIND_CONTRACT => Some(AddressKind::Contract),
            KIND_MUXED => Some(AddressKind::MuxedAccount),
            _ => None,
        }
    }

    /// Whether an account of this kind has to opt into an asset before it can receive it.
    /// Contracts do not; classic and muxed accounts do.
    pub fn needs_trustline(&self) -> bool {
        match self {
            AddressKind::Account | AddressKind::MuxedAccount => true,
            AddressKind::Contract => false,
        }
    }
}

/// A fully qualified Stellar destination.
///
/// `key` is the 32 byte ed25519 public key for an account or muxed account, and the 32 byte
/// contract id for a contract. `muxed_id` is meaningful only when `kind` is
/// [`AddressKind::MuxedAccount`] and must be zero otherwise, which [`StellarDestination::validate`]
/// enforces.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StellarDestination {
    pub kind: AddressKind,
    pub key: BytesN<32>,
    pub muxed_id: u64,
}

impl StellarDestination {
    pub fn account(key: BytesN<32>) -> Self {
        Self {
            kind: AddressKind::Account,
            key,
            muxed_id: 0,
        }
    }

    pub fn contract(key: BytesN<32>) -> Self {
        Self {
            kind: AddressKind::Contract,
            key,
            muxed_id: 0,
        }
    }

    pub fn muxed(key: BytesN<32>, muxed_id: u64) -> Self {
        Self {
            kind: AddressKind::MuxedAccount,
            key,
            muxed_id,
        }
    }

    /// Reject the destinations that would mint into a hole.
    ///
    /// An all-zero key is never a real account and is the shape a truncated or
    /// default-initialised field takes, which is exactly what you want to catch before the
    /// burn on the far side has already happened. A nonzero muxed id on a non-muxed kind
    /// means the sender and the encoder disagree about what they are describing, and
    /// guessing which one is right is not a decision a bridge gets to make.
    pub fn validate(&self) -> Result<(), HyperionError> {
        if self.key == BytesN::from_array(self.key.env(), &[0u8; 32]) {
            return Err(HyperionError::ZeroAddressKey);
        }
        if self.kind != AddressKind::MuxedAccount && self.muxed_id != 0 {
            return Err(HyperionError::InvalidDestination);
        }
        Ok(())
    }

    /// Serialise into Hyperion's tagged wire format.
    ///
    /// Layout is one tag byte, then the 32 byte key, then, for muxed accounts only, a
    /// big-endian u64. This rides inside a rail's arbitrary payload field, never in a fixed
    /// 32 byte slot, precisely because 32 bytes has no room for the tag.
    pub fn encode(&self, env: &Env) -> Bytes {
        let mut out = Bytes::new(env);
        out.push_back(self.kind.tag());
        out.append(&Bytes::from_slice(env, &self.key.to_array()));
        if self.kind == AddressKind::MuxedAccount {
            out.append(&Bytes::from_slice(env, &self.muxed_id.to_be_bytes()));
        }
        out
    }

    /// Parse Hyperion's tagged wire format back out.
    pub fn decode(env: &Env, raw: &Bytes) -> Result<Self, HyperionError> {
        let len = raw.len();
        if len != TAGGED_LEN_PLAIN && len != TAGGED_LEN_MUXED {
            return Err(HyperionError::InvalidDestination);
        }

        let tag = raw.get(0).ok_or(HyperionError::InvalidDestination)?;
        let kind = AddressKind::from_tag(tag).ok_or(HyperionError::InvalidDestination)?;

        let mut key = [0u8; 32];
        let mut i = 0u32;
        while i < 32 {
            key[i as usize] = raw.get(1 + i).ok_or(HyperionError::InvalidDestination)?;
            i += 1;
        }

        let muxed_id = if kind == AddressKind::MuxedAccount {
            if len != TAGGED_LEN_MUXED {
                return Err(HyperionError::InvalidDestination);
            }
            let mut id = [0u8; 8];
            let mut j = 0u32;
            while j < 8 {
                id[j as usize] = raw.get(33 + j).ok_or(HyperionError::InvalidDestination)?;
                j += 1;
            }
            u64::from_be_bytes(id)
        } else {
            if len != TAGGED_LEN_PLAIN {
                return Err(HyperionError::InvalidDestination);
            }
            0
        };

        let dest = Self {
            kind,
            key: BytesN::from_array(env, &key),
            muxed_id,
        };
        dest.validate()?;
        Ok(dest)
    }
}

/// Refuse a destination the chosen rail physically cannot deliver to.
///
/// CCTP's mint recipient is a bare 32 byte field with no room for a muxed id, so a muxed
/// destination on that route would arrive as the underlying G account with the sub-account
/// information gone. For an exchange deposit that means a credited-to-nobody transfer. The
/// documented fix is a forwarder contract that resolves the destination before the mint, and
/// until Hyperion's forwarder is the mint recipient on a given lane, the honest answer is to
/// decline the route rather than deliver somewhere unclaimable.
pub fn assert_route_supports(route: RouteKind, kind: AddressKind) -> Result<(), HyperionError> {
    match (route, kind) {
        (RouteKind::Cctp, AddressKind::MuxedAccount) => Err(HyperionError::MuxedNotSupported),
        (RouteKind::Allbridge, AddressKind::MuxedAccount) => Err(HyperionError::MuxedNotSupported),
        _ => Ok(()),
    }
}

/// Widen a 20 byte EVM address into the 32 byte slot cross-chain messages use.
///
/// EVM convention is `bytes32(uint256(uint160(addr)))`, which puts the twenty real bytes at
/// the *end* of the word and leaves the first twelve zero. Getting that backwards produces a
/// value that looks plausible and points nowhere.
pub fn evm_to_bytes32(env: &Env, addr: &BytesN<20>) -> BytesN<32> {
    let mut out = [0u8; 32];
    let src = addr.to_array();
    let mut i = 0usize;
    while i < 20 {
        out[12 + i] = src[i];
        i += 1;
    }
    BytesN::from_array(env, &out)
}

/// Narrow a 32 byte cross-chain value back to a 20 byte EVM address.
///
/// Fails if any of the leading twelve bytes is set, because that is not an EVM address. It is
/// either a Stellar key that took a wrong turn or a deliberately malformed destination, and
/// both deserve a revert rather than a truncation.
pub fn bytes32_to_evm(env: &Env, raw: &BytesN<32>) -> Result<BytesN<20>, HyperionError> {
    let src = raw.to_array();
    let mut i = 0usize;
    while i < 12 {
        if src[i] != 0 {
            return Err(HyperionError::NotEvmAddress);
        }
        i += 1;
    }
    let mut out = [0u8; 20];
    let mut j = 0usize;
    while j < 20 {
        out[j] = src[12 + j];
        j += 1;
    }
    if out == [0u8; 20] {
        return Err(HyperionError::ZeroAddressKey);
    }
    Ok(BytesN::from_array(env, &out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use soroban_sdk::Env;

    fn key(env: &Env, seed: u8) -> BytesN<32> {
        BytesN::from_array(env, &[seed; 32])
    }

    #[test]
    fn a_classic_account_round_trips_through_the_wire_format() {
        let env = Env::default();
        let dest = StellarDestination::account(key(&env, 7));
        let wire = dest.encode(&env);
        assert_eq!(wire.len(), TAGGED_LEN_PLAIN);
        assert_eq!(wire.get(0).unwrap(), KIND_ACCOUNT);
        assert_eq!(StellarDestination::decode(&env, &wire).unwrap(), dest);
    }

    #[test]
    fn a_contract_round_trips_and_keeps_its_own_tag() {
        let env = Env::default();
        let dest = StellarDestination::contract(key(&env, 9));
        let wire = dest.encode(&env);
        assert_eq!(wire.get(0).unwrap(), KIND_CONTRACT);
        let back = StellarDestination::decode(&env, &wire).unwrap();
        assert_eq!(back.kind, AddressKind::Contract);
        assert_eq!(back, dest);
    }

    #[test]
    fn a_muxed_account_keeps_its_sub_account_id() {
        let env = Env::default();
        let dest = StellarDestination::muxed(key(&env, 3), 9_007_199_254_740_993);
        let wire = dest.encode(&env);
        assert_eq!(wire.len(), TAGGED_LEN_MUXED);
        let back = StellarDestination::decode(&env, &wire).unwrap();
        assert_eq!(back.muxed_id, 9_007_199_254_740_993);
        assert_eq!(back, dest);
    }

    #[test]
    fn the_three_kinds_do_not_collide_on_the_wire() {
        let env = Env::default();
        let k = key(&env, 42);
        let a = StellarDestination::account(k.clone()).encode(&env);
        let c = StellarDestination::contract(k.clone()).encode(&env);
        let m = StellarDestination::muxed(k, 0).encode(&env);
        assert_ne!(a, c);
        assert_ne!(a, m);
        assert_ne!(c, m);
    }

    #[test]
    fn an_all_zero_key_is_refused() {
        let env = Env::default();
        let dest = StellarDestination::account(BytesN::from_array(&env, &[0u8; 32]));
        assert_eq!(dest.validate(), Err(HyperionError::ZeroAddressKey));
    }

    #[test]
    fn a_muxed_id_on_a_non_muxed_kind_is_refused() {
        let env = Env::default();
        let dest = StellarDestination {
            kind: AddressKind::Account,
            key: key(&env, 1),
            muxed_id: 5,
        };
        assert_eq!(dest.validate(), Err(HyperionError::InvalidDestination));
    }

    #[test]
    fn a_truncated_destination_is_refused_rather_than_zero_padded() {
        let env = Env::default();
        let mut short = Bytes::new(&env);
        short.push_back(KIND_ACCOUNT);
        short.append(&Bytes::from_slice(&env, &[1u8; 20]));
        assert_eq!(
            StellarDestination::decode(&env, &short),
            Err(HyperionError::InvalidDestination)
        );
    }

    #[test]
    fn an_unknown_kind_tag_is_refused() {
        let env = Env::default();
        let mut wire = Bytes::new(&env);
        wire.push_back(99);
        wire.append(&Bytes::from_slice(&env, &[1u8; 32]));
        assert_eq!(
            StellarDestination::decode(&env, &wire),
            Err(HyperionError::InvalidDestination)
        );
    }

    #[test]
    fn a_muxed_tag_without_its_id_bytes_is_refused() {
        let env = Env::default();
        let mut wire = Bytes::new(&env);
        wire.push_back(KIND_MUXED);
        wire.append(&Bytes::from_slice(&env, &[1u8; 32]));
        assert_eq!(
            StellarDestination::decode(&env, &wire),
            Err(HyperionError::InvalidDestination)
        );
    }

    #[test]
    fn a_plain_tag_carrying_extra_id_bytes_is_refused() {
        let env = Env::default();
        let mut wire = Bytes::new(&env);
        wire.push_back(KIND_ACCOUNT);
        wire.append(&Bytes::from_slice(&env, &[1u8; 32]));
        wire.append(&Bytes::from_slice(&env, &[0u8; 8]));
        assert_eq!(
            StellarDestination::decode(&env, &wire),
            Err(HyperionError::InvalidDestination)
        );
    }

    #[test]
    fn cctp_declines_a_muxed_destination() {
        assert_eq!(
            assert_route_supports(RouteKind::Cctp, AddressKind::MuxedAccount),
            Err(HyperionError::MuxedNotSupported)
        );
        assert!(assert_route_supports(RouteKind::Cctp, AddressKind::Account).is_ok());
        assert!(assert_route_supports(RouteKind::Cctp, AddressKind::Contract).is_ok());
    }

    #[test]
    fn allbridge_declines_a_muxed_destination_too() {
        assert_eq!(
            assert_route_supports(RouteKind::Allbridge, AddressKind::MuxedAccount),
            Err(HyperionError::MuxedNotSupported)
        );
    }

    #[test]
    fn the_axelar_routes_carry_a_muxed_destination_fine() {
        assert!(assert_route_supports(RouteKind::AxelarGmp, AddressKind::MuxedAccount).is_ok());
        assert!(assert_route_supports(RouteKind::AxelarIts, AddressKind::MuxedAccount).is_ok());
    }

    #[test]
    fn an_evm_address_lands_in_the_low_twenty_bytes() {
        let env = Env::default();
        let addr = BytesN::from_array(&env, &[0xAB; 20]);
        let wide = evm_to_bytes32(&env, &addr);
        let raw = wide.to_array();
        assert_eq!(&raw[0..12], &[0u8; 12]);
        assert_eq!(&raw[12..32], &[0xABu8; 20]);
        assert_eq!(bytes32_to_evm(&env, &wide).unwrap(), addr);
    }

    #[test]
    fn a_stellar_key_is_not_mistaken_for_an_evm_address() {
        let env = Env::default();
        let stellar_key = BytesN::from_array(&env, &[0x5Cu8; 32]);
        assert_eq!(
            bytes32_to_evm(&env, &stellar_key),
            Err(HyperionError::NotEvmAddress)
        );
    }

    #[test]
    fn a_right_padded_evm_address_is_caught() {
        // The classic mistake: twenty bytes written at the front instead of the back.
        let env = Env::default();
        let mut wrong = [0u8; 32];
        wrong[0..20].copy_from_slice(&[0xCD; 20]);
        assert_eq!(
            bytes32_to_evm(&env, &BytesN::from_array(&env, &wrong)),
            Err(HyperionError::NotEvmAddress)
        );
    }

    #[test]
    fn the_evm_zero_address_is_refused() {
        let env = Env::default();
        let zero = BytesN::from_array(&env, &[0u8; 32]);
        assert_eq!(
            bytes32_to_evm(&env, &zero),
            Err(HyperionError::ZeroAddressKey)
        );
    }

    proptest! {
        #[test]
        fn every_evm_address_round_trips(bytes in proptest::array::uniform20(1u8..=255u8)) {
            let env = Env::default();
            let addr = BytesN::from_array(&env, &bytes);
            let wide = evm_to_bytes32(&env, &addr);
            prop_assert_eq!(bytes32_to_evm(&env, &wide).unwrap(), addr);
        }

        #[test]
        fn every_tagged_destination_round_trips(
            seed in 1u8..=255u8,
            kind_tag in 0u8..=2u8,
            muxed_id in any::<u64>(),
        ) {
            let env = Env::default();
            let kind = AddressKind::from_tag(kind_tag).unwrap();
            let dest = StellarDestination {
                kind,
                key: BytesN::from_array(&env, &[seed; 32]),
                muxed_id: if kind == AddressKind::MuxedAccount { muxed_id } else { 0 },
            };
            let wire = dest.encode(&env);
            prop_assert_eq!(StellarDestination::decode(&env, &wire).unwrap(), dest);
        }
    }
}
