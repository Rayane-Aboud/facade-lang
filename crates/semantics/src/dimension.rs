//! Dimensions (`docs/feature-1-dimensions.md`, section 2.2): an integer
//! exponent for each declared base dimension. Multiplying quantities adds
//! exponents, so dimensions form an abelian group.

use std::collections::BTreeMap;
use std::fmt;
use std::ops::{Div, Mul};

/// A dimension such as `Length^2` or `Length * Mass * Time^-2`.
///
/// Invariant: no exponent is zero. So equal dimensions are always stored the
/// same way, and `==` means "same dimension". The field is private so that
/// only the methods below, which keep the invariant, can build a value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dimension {
    exponents: BTreeMap<String, i32>,
}

impl Dimension {
    /// The dimensionless dimension `1`: every exponent is 0.
    pub fn one() -> Self {
        Dimension {
            exponents: BTreeMap::new(),
        }
    }

    /// A base dimension with exponent 1: `Dimension::base("Length")` is `L`.
    pub fn base(name: &str) -> Self {
        Dimension {
            exponents: BTreeMap::from([(name.to_string(), 1)]),
        }
    }

    /// `dⁿ`: multiply every exponent by `n`. `d⁰` is `1`.
    pub fn pow(&self, n: i32) -> Dimension {
        if n == 0 {
            return Dimension::one();
        }
        // e ≠ 0 and n ≠ 0, so e * n ≠ 0: the invariant holds.
        let exponents = self
            .exponents
            .iter()
            .map(|(base, e)| (base.clone(), e * n))
            .collect();
        Dimension { exponents }
    }

    /// `d⁻¹`: negate every exponent.
    pub fn inv(&self) -> Dimension {
        self.pow(-1)
    }

    pub fn is_dimensionless(&self) -> bool {
        self.exponents.is_empty()
    }
}

/// `d₁ · d₂`: add the exponents, removing any that become 0.
impl Mul<&Dimension> for &Dimension {
    type Output = Dimension;

    fn mul(self, other: &Dimension) -> Dimension {
        let mut exponents = self.exponents.clone();
        for (base, e) in &other.exponents {
            let sum = exponents.get(base).copied().unwrap_or(0) + e;
            if sum == 0 {
                exponents.remove(base);
            } else {
                exponents.insert(base.clone(), sum);
            }
        }
        Dimension { exponents }
    }
}

/// `d₁ / d₂ = d₁ · d₂⁻¹`.
impl Div<&Dimension> for &Dimension {
    type Output = Dimension;

    fn div(self, other: &Dimension) -> Dimension {
        Mul::mul(self, &other.inv())
    }
}

/// `Length * Mass * Time^-2`; the dimensionless dimension is `1`.
impl fmt::Display for Dimension {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_dimensionless() {
            return write!(f, "1");
        }
        let parts: Vec<String> = self
            .exponents
            .iter()
            .map(|(base, e)| match e {
                1 => base.clone(),
                _ => format!("{base}^{e}"),
            })
            .collect();
        write!(f, "{}", parts.join(" * "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn length() -> Dimension {
        Dimension::base("Length")
    }

    fn time() -> Dimension {
        Dimension::base("Time")
    }

    // Examples from section 2.2

    #[test]
    fn multiplying_adds_exponents() {
        assert_eq!(&length() * &length(), length().pow(2));
        assert_eq!((&length() * &length()).to_string(), "Length^2");
    }

    #[test]
    fn force_is_mass_length_per_time_squared() {
        let force = &(&Dimension::base("Mass") * &length()) / &time().pow(2);
        assert_eq!(force.to_string(), "Length * Mass * Time^-2");
    }

    #[test]
    fn area_divided_by_length_is_length() {
        assert_eq!(&length().pow(2) / &length(), length());
    }

    #[test]
    fn dividing_by_itself_is_exactly_one() {
        // The invariant: `Length: 0` is removed, so this equals `one()`.
        let d = &length() / &length();
        assert_eq!(d, Dimension::one());
        assert!(d.is_dimensionless());
        assert_eq!(d.to_string(), "1");
    }

    #[test]
    fn different_dimensions_are_not_equal() {
        assert_ne!(length(), time());
        assert_ne!(length(), length().pow(2));
        assert_ne!(length().inv(), length());
    }

    // The group laws, checked on random dimensions

    const BASES: &[&str] = &["Length", "Mass", "Time", "Temperature"];

    /// Random dimensions over a few base dimensions, exponents from -4 to 4.
    /// Built only with the public operations, so they keep the invariant.
    fn dimension() -> impl Strategy<Value = Dimension> {
        prop::collection::btree_map(prop::sample::select(BASES), -4..5i32, 0..BASES.len()).prop_map(
            |exponents| {
                exponents
                    .into_iter()
                    .fold(Dimension::one(), |d, (base, e)| {
                        &d * &Dimension::base(base).pow(e)
                    })
            },
        )
    }

    proptest! {
        #[test]
        fn mul_is_associative(a in dimension(), b in dimension(), c in dimension()) {
            prop_assert_eq!(&(&a * &b) * &c, &a * &(&b * &c));
        }

        #[test]
        fn mul_is_commutative(a in dimension(), b in dimension()) {
            prop_assert_eq!(&a * &b, &b * &a);
        }

        #[test]
        fn one_is_the_identity(a in dimension()) {
            prop_assert_eq!(&a * &Dimension::one(), a.clone());
        }

        #[test]
        fn inverse_cancels(a in dimension()) {
            prop_assert_eq!(&a * &a.inv(), Dimension::one());
        }

        #[test]
        fn pow_is_repeated_multiplication(a in dimension(), n in 0..5i32) {
            let repeated = (0..n).fold(Dimension::one(), |acc, _| &acc * &a);
            prop_assert_eq!(a.pow(n), repeated);
        }

        #[test]
        fn div_is_mul_by_inverse(a in dimension(), b in dimension()) {
            prop_assert_eq!(&a / &b, &a * &b.inv());
        }
    }
}
