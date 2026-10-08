use hyperion_core::amount::{convert_decimals, pow10, scale_amount, MAX_DECIMALS};
use hyperion_core::error::HyperionError;
use hyperion_core::{EVM_USDC_DECIMALS, STELLAR_DECIMALS};
use proptest::prelude::*;

const EVM_ETH_DECIMALS: u32 = 18;
const SUPPORTED_DECIMALS: [u32; 3] = [EVM_USDC_DECIMALS, STELLAR_DECIMALS, EVM_ETH_DECIMALS];

fn arb_core_decimal_pair() -> impl Strategy<Value = (u32, u32)> {
    (
        prop_oneof![
            Just(EVM_USDC_DECIMALS),
            Just(STELLAR_DECIMALS),
            Just(EVM_ETH_DECIMALS),
        ],
        prop_oneof![
            Just(EVM_USDC_DECIMALS),
            Just(STELLAR_DECIMALS),
            Just(EVM_ETH_DECIMALS),
        ],
    )
}

fn arb_wide_decimal_pair() -> impl Strategy<Value = (u32, u32)> {
    (0u32..=MAX_DECIMALS, 0u32..=MAX_DECIMALS)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Invariant 1: Round-trip scaling never inflates the original amount.
    /// scale_amount(scale_amount(x, from, to), to, from) <= x
    #[test]
    fn prop_round_trip_never_inflates_core_decimals(
        amount in 0i128..=1_000_000_000_000_000_000i128,
        (from, to) in arb_core_decimal_pair(),
    ) {
        if let Ok(scaled) = scale_amount(amount, from, to) {
            let round_trip = scale_amount(scaled, to, from).expect("reverse scaling should not fail");
            prop_assert!(
                round_trip <= amount,
                "unintended inflation: scaled from {} to {} and back: original={}, round_trip={}",
                from,
                to,
                amount,
                round_trip
            );
        }
    }

    /// Invariant 1 (Extended): Round-trip scaling never inflates across any valid decimal pairs (0..=38).
    #[test]
    fn prop_round_trip_never_inflates_arbitrary_decimals(
        amount in 0i128..=1_000_000_000_000_000_000i128,
        (from, to) in arb_wide_decimal_pair(),
    ) {
        if let Ok(scaled) = scale_amount(amount, from, to) {
            if let Ok(round_trip) = scale_amount(scaled, to, from) {
                prop_assert!(
                    round_trip <= amount,
                    "unintended inflation across arbitrary decimals ({}, {}): original={}, round_trip={}",
                    from,
                    to,
                    amount,
                    round_trip
                );
            }
        }
    }

    /// Invariant 2: Monotonicity.
    /// If x1 <= x2, then scale_amount(x1, from, to) <= scale_amount(x2, from, to).
    #[test]
    fn prop_scaling_is_monotonic(
        x1 in 0i128..=1_000_000_000_000_000_000i128,
        delta in 0i128..=1_000_000_000_000_000_000i128,
        (from, to) in arb_core_decimal_pair(),
    ) {
        let x2 = x1 + delta;
        if let (Ok(s1), Ok(s2)) = (scale_amount(x1, from, to), scale_amount(x2, from, to)) {
            prop_assert!(
                s1 <= s2,
                "monotonicity violated for from={}, to={}: x1={}, x2={}, s1={}, s2={}",
                from,
                to,
                x1,
                x2,
                s1,
                s2
            );
        }
    }

    /// Invariant 3: Precision loss (dust) is strictly bounded by the scaling factor when downscaling.
    #[test]
    fn prop_dust_is_strictly_bounded(
        amount in 0i128..=1_000_000_000_000_000_000i128,
        (from, to) in arb_core_decimal_pair(),
    ) {
        let conversion = convert_decimals(amount, from, to).unwrap();
        if from >= to {
            let diff = from - to;
            let factor = pow10(diff).unwrap();
            prop_assert_eq!(conversion.amount * factor + conversion.dust, amount);
            prop_assert!(conversion.dust >= 0 && conversion.dust < factor);
        } else {
            // Scaling up has zero dust
            prop_assert_eq!(conversion.dust, 0);
        }
    }

    /// Invariant 4: No unhandled panics occur on any arbitrary i128 and arbitrary u32 decimals.
    #[test]
    fn prop_no_unhandled_panics_on_arbitrary_input(
        amount in any::<i128>(),
        from in any::<u32>(),
        to in any::<u32>(),
    ) {
        let res = scale_amount(amount, from, to);
        if amount < 0 {
            prop_assert_eq!(res, Err(HyperionError::InvalidAmount));
        } else if from > MAX_DECIMALS || to > MAX_DECIMALS {
            prop_assert_eq!(res, Err(HyperionError::InvalidDecimals));
        } else {
            match res {
                Ok(val) => {
                    prop_assert!(val >= 0);
                }
                Err(HyperionError::DecimalOverflow) => {
                    // Valid overflow condition
                }
                Err(err) => {
                    prop_assert!(false, "unexpected error variant: {:?}", err);
                }
            }
        }
    }
}

#[test]
fn boundary_value_zero_scales_to_zero_deterministically() {
    for &from in &SUPPORTED_DECIMALS {
        for &to in &SUPPORTED_DECIMALS {
            assert_eq!(scale_amount(0, from, to), Ok(0));
            let c = convert_decimals(0, from, to).unwrap();
            assert_eq!(c.amount, 0);
            assert_eq!(c.dust, 0);
        }
    }
}

#[test]
fn boundary_value_one_scales_deterministically() {
    for &from in &SUPPORTED_DECIMALS {
        for &to in &SUPPORTED_DECIMALS {
            let res = scale_amount(1, from, to).unwrap();
            if from <= to {
                let factor = pow10(to - from).unwrap();
                assert_eq!(res, factor);
            } else {
                // Integer division 1 / 10^(from - to) == 0
                assert_eq!(res, 0);
                let c = convert_decimals(1, from, to).unwrap();
                assert_eq!(c.dust, 1);
            }
        }
    }
}

#[test]
fn boundary_value_i128_max_handles_overflow_without_panicking() {
    for &from in &SUPPORTED_DECIMALS {
        for &to in &SUPPORTED_DECIMALS {
            let res = scale_amount(i128::MAX, from, to);
            if from < to {
                // Scaling up i128::MAX must deterministically return DecimalOverflow
                assert_eq!(res, Err(HyperionError::DecimalOverflow));
            } else if from == to {
                assert_eq!(res, Ok(i128::MAX));
            } else {
                let factor = pow10(from - to).unwrap();
                assert_eq!(res, Ok(i128::MAX / factor));
            }
        }
    }
}

#[test]
fn negative_amounts_consistently_rejected() {
    let negatives = [-1, -100, -1_000_000, i128::MIN];
    for &val in &negatives {
        for &from in &SUPPORTED_DECIMALS {
            for &to in &SUPPORTED_DECIMALS {
                assert_eq!(
                    scale_amount(val, from, to),
                    Err(HyperionError::InvalidAmount)
                );
            }
        }
    }
}

#[test]
fn multi_precision_stellar_to_evm_usdc_and_dai() {
    // 100 Stellar SAC units (7 decimals) = 100 * 10^7 = 1_000_000_000
    let stellar_amount = 1_000_000_000i128;

    // Stellar (7) -> EVM USDC (6): 100 * 10^6 = 100_000_000
    let usdc = scale_amount(stellar_amount, STELLAR_DECIMALS, EVM_USDC_DECIMALS).unwrap();
    assert_eq!(usdc, 100_000_000);

    // EVM USDC (6) -> EVM DAI (18): 100 * 10^18 = 100_000_000_000_000_000_000
    let dai = scale_amount(usdc, EVM_USDC_DECIMALS, EVM_ETH_DECIMALS).unwrap();
    assert_eq!(dai, 100_000_000_000_000_000_000);

    // EVM DAI (18) -> Stellar (7): back to 100 * 10^7
    let back_to_stellar = scale_amount(dai, EVM_ETH_DECIMALS, STELLAR_DECIMALS).unwrap();
    assert_eq!(back_to_stellar, stellar_amount);
}
