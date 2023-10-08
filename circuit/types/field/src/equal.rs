// Copyright (C) 2019-2023 Aleo Systems Inc.
// This file is part of the snarkVM library.

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at:
// http://www.apache.org/licenses/LICENSE-2.0

// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use super::*;
use snarkvm_circuit_environment::Private;

impl<E: Environment> Equal<Self> for Field<E> {
    type Output = Boolean<E>;

    ///
    /// Returns `true` if `self` and `other` are equal.
    ///
    /// This method costs 2 constraints.
    ///
    fn is_equal(&self, other: &Self) -> Self::Output {
        !self.is_not_equal(other)
    }

    ///
    /// Returns `true` if `self` and `other` are *not* equal.
    ///
    /// This method constructs a boolean that indicates if
    /// `self` and `other ` are *not* equal to each other.
    ///
    /// This method costs 2 constraints.
    ///
    fn is_not_equal(&self, other: &Self) -> Self::Output {
        // In all cases, the witness ("ejected") value is calculated from the ejected values.
        let is_neq_ejected = self.eject_value() != other.eject_value();

        match (self.is_constant(), other.is_constant()) {
            // If both operands are constant, the result is also constant.
            (true, true) => Boolean::new(Mode::Constant, is_neq_ejected),

            // Otherwise, we introduce a private field variable is_neq for the result,
            // along with an auxiliary variable multiplier for the inverse of the difference between the operands,
            // and enforce the following constraints:
            //   (self - other) (multiplier) = (is_neq)
            //   (self - other) (1 - is_neq) = (0)
            // These constraints imply that is_neq is boolean, i.e. either 0 or 1;
            // so we avoid creating is_neq as a Boolean, which would generate an unneeded boolean constraint.
            // See the comments just after the code for more detailed explanations.
            _ => {
                let is_neq = Boolean::from_variable(E::new_variable(Mode::Private, match is_neq_ejected {
                    true => E::BaseField::one(),
                    false => E::BaseField::zero(),
                }));
                let delta = self - other;
                let multiplier: Field<E> = witness!(|delta| {
                    match delta.inverse() {
                        Ok(inverse) => inverse,
                        _ => console::Field::one(), // exact value is irrelevant, because (0) (anything) = (0)
                    }
                });
                let is_eq = !is_neq.clone(); // 1 - is_neq
                E::enforce(|| (&delta, &multiplier, &is_neq)); // 1st constraint
                E::enforce(|| (delta, is_eq, E::zero())); // 2nd constraint
                is_neq

                // Inequality Enforcement
                // ----------------------------------------------------------------
                // Check 1:  (a - b) * multiplier = is_neq
                // Check 2:  (a - b) * not(is_neq) = 0
                //
                //
                // Case 1: a == b AND is_neq == 0 (honest)
                // ----------------------------------------------------------------
                // Check 1:  (a - b) * 1 = 0
                //                 a - b = 0
                // => As a == b, is_neq is correct.
                //
                // Check 2:  (a - b) * not(0) = 0
                //                      a - b = 0
                // => As a == b, is_neq is correct.
                //
                // Remark: While the multiplier = 1 here, letting multiplier := n,
                //         for n as any field element, also holds.
                //
                //
                // Case 2: a == b AND is_neq == 1 (dishonest)
                // ----------------------------------------------------------------
                // Check 1:  (a - b) * 1 = 1
                //                 a - b = 1
                // => As a == b, the is_neq is incorrect.
                //
                // Remark: While the multiplier = 1 here, letting multiplier := n,
                //         for n as any field element, also holds.
                //
                //
                // Case 3a: a != b AND is_neq == 0 AND multiplier = 0 (dishonest)
                // ----------------------------------------------------------------
                // Check 2:  (a - b) * not(0) = 0
                //                      a - b = 0
                // => As a != b, is_neq is incorrect.
                //
                // Case 3b: a != b AND is_neq == 0 AND multiplier = 1 (dishonest)
                // ----------------------------------------------------------------
                // Check 1:  (a - b) * 1 = 0
                //                 a - b = 0
                // => As a != b, is_neq is incorrect.
                //
                // Remark: While the multiplier = 1 here, letting multiplier = n,
                //         for n as any field element (n != 0), also holds.
                //
                //
                // Case 4a: a != b AND is_neq == 1 AND multiplier = n [!= (a - b)^(-1)] (dishonest)
                // ---------------------------------------------------------------------------------
                // Check 1:  (a - b) * n = 1
                // => As n != (a - b)^(-1), is_neq is incorrect.
                //
                // Case 4b: a != b AND is_neq == 1 AND multiplier = (a - b)^(-1) (honest)
                // ---------------------------------------------------------------------------------
                // Check 1:  (a - b) * (a - b)^(-1) = 1
                //                                1 = 1
                // => is_neq is trivially correct.
                //
                // Check 2:  (a - b) * not(1) = 0
                //                          0 = 0
                // => is_neq is trivially correct.
            }
        }
    }
}

impl<E: Environment> Metrics<dyn Equal<Field<E>, Output = Boolean<E>>> for Field<E> {
    type Case = (Mode, Mode);

    // TODO: How to deal where both operands are the same field element, since it changes the number of gates produced? We could use upper bounds.
    fn count(case: &Self::Case) -> Count {
        match case {
            (Mode::Constant, Mode::Constant) => Count::is(1, 0, 0, 0),
            _ => Count::is(0, 0, 2, 2),
        }
    }
}

impl<E: Environment> OutputMode<dyn Equal<Field<E>, Output = Boolean<E>>> for Field<E> {
    type Case = (Mode, Mode);

    fn output_mode(case: &Self::Case) -> Mode {
        match case {
            (Mode::Constant, Mode::Constant) => Mode::Constant,
            _ => Mode::Private,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use snarkvm_circuit_environment::Circuit;

    const ITERATIONS: u64 = 200;

    fn check_is_equal(name: &str, expected: bool, a: &Field<Circuit>, b: &Field<Circuit>) {
        Circuit::scope(name, || {
            let candidate = a.is_equal(b);
            assert_eq!(expected, candidate.eject_value(), "({} == {})", a.eject_value(), b.eject_value());
            assert_count!(Equal(Field, Field) => Boolean, &(a.eject_mode(), b.eject_mode()));
            assert_output_mode!(Equal(Field, Field) => Boolean, &(a.eject_mode(), b.eject_mode()), candidate);
        });
    }

    fn check_is_not_equal(name: &str, expected: bool, a: &Field<Circuit>, b: &Field<Circuit>) {
        Circuit::scope(name, || {
            let candidate = a.is_not_equal(b);
            assert_eq!(expected, candidate.eject_value(), "({} != {})", a.eject_value(), b.eject_value());
            assert_count!(Equal(Field, Field) => Boolean, &(a.eject_mode(), b.eject_mode()));
            assert_output_mode!(Equal(Field, Field) => Boolean, &(a.eject_mode(), b.eject_mode()), candidate);
        });
    }

    fn run_test(mode_a: Mode, mode_b: Mode) {
        let mut rng = TestRng::default();

        for i in 0..ITERATIONS {
            let first = Uniform::rand(&mut rng);
            let second = Uniform::rand(&mut rng);

            let a = Field::<Circuit>::new(mode_a, first);
            let b = Field::<Circuit>::new(mode_b, second);

            let name = format!("Equal: a == b {i}");
            check_is_equal(&name, first == second, &a, &b);

            let name = format!("Not Equal: a != b {i}");
            check_is_not_equal(&name, first != second, &a, &b);

            // Check first is equal to first.
            let a = Field::<Circuit>::new(mode_a, first);
            let b = Field::<Circuit>::new(mode_b, first);
            let name = format!("{first} == {first}");
            check_is_equal(&name, true, &a, &b);

            // Check second is equal to second.
            let a = Field::<Circuit>::new(mode_a, second);
            let b = Field::<Circuit>::new(mode_b, second);
            let name = format!("{second} == {second}");
            check_is_equal(&name, true, &a, &b);
        }
    }

    #[test]
    fn test_constant_is_equal_to_constant() {
        run_test(Mode::Constant, Mode::Constant);
    }

    #[test]
    fn test_constant_is_not_equal_to_public() {
        run_test(Mode::Constant, Mode::Public);
    }

    #[test]
    fn test_constant_is_not_equal_to_private() {
        run_test(Mode::Constant, Mode::Private);
    }

    #[test]
    fn test_public_is_equal_to_constant() {
        run_test(Mode::Public, Mode::Constant);
    }

    #[test]
    fn test_private_is_equal_to_constant() {
        run_test(Mode::Private, Mode::Constant);
    }

    #[test]
    fn test_public_is_equal_to_public() {
        run_test(Mode::Public, Mode::Public);
    }

    #[test]
    fn test_public_is_not_equal_to_private() {
        run_test(Mode::Public, Mode::Private);
    }

    #[test]
    fn test_private_is_equal_to_public() {
        run_test(Mode::Private, Mode::Public);
    }

    #[test]
    fn test_private_is_equal_to_private() {
        run_test(Mode::Private, Mode::Private);
    }

    #[test]
    fn test_is_eq_cases() {
        let one = console::Field::<<Circuit as Environment>::Network>::one();

        // Basic `true` and `false` cases
        {
            let mut accumulator = one + one;

            for _ in 0..ITERATIONS {
                let a = Field::<Circuit>::new(Mode::Private, accumulator);
                let b = Field::<Circuit>::new(Mode::Private, accumulator);
                let is_eq = a.is_equal(&b);
                assert!(is_eq.eject_value()); // true

                let a = Field::<Circuit>::new(Mode::Private, one);
                let b = Field::<Circuit>::new(Mode::Private, accumulator);
                let is_eq = a.is_equal(&b);
                assert!(!is_eq.eject_value()); // false

                let a = Field::<Circuit>::new(Mode::Private, accumulator);
                let b = Field::<Circuit>::new(Mode::Private, accumulator - one);
                let is_eq = a.is_equal(&b);
                assert!(!is_eq.eject_value()); // false

                accumulator += one;
            }
        }
    }

    #[test]
    fn test_is_neq_cases() {
        let zero = console::Field::<<Circuit as Environment>::Network>::zero();
        let one = console::Field::<<Circuit as Environment>::Network>::one();
        let two = one + one;
        let five = two + two + one;

        // Inequality Enforcement
        // ----------------------------------------------------------------
        // Check 1:  (a - b) * multiplier = is_neq
        // Check 2:  (a - b) * not(is_neq) = 0

        let enforce = |a: Field<Circuit>, b: Field<Circuit>, multiplier: Field<Circuit>, is_neq: Boolean<Circuit>| {
            // Compute `self` - `other`.
            let delta = &a - &b;

            // Negate `is_neq`.
            let is_eq = !is_neq.clone();

            // Check 1: (a - b) * multiplier = is_neq
            Circuit::enforce(|| (delta.clone(), multiplier, is_neq.clone()));

            // Check 2: (a - b) * not(is_neq) = 0
            Circuit::enforce(|| (delta, is_eq, Circuit::zero()));
        };

        //
        // Case 1: a == b AND is_neq == 0 (honest)
        // ----------------------------------------------------------------

        let a = Field::<Circuit>::new(Mode::Private, five);
        let b = Field::<Circuit>::new(Mode::Private, five);
        let multiplier = Field::<Circuit>::new(Mode::Private, one);
        let is_neq = Boolean::new(Mode::Private, false);

        assert!(Circuit::is_satisfied());
        enforce(a, b, multiplier, is_neq);
        assert!(Circuit::is_satisfied());
        Circuit::reset();

        //
        // Case 2: a == b AND is_neq == 1 (dishonest)
        // ----------------------------------------------------------------

        let a = Field::<Circuit>::new(Mode::Private, five);
        let b = Field::<Circuit>::new(Mode::Private, five);
        let multiplier = Field::<Circuit>::new(Mode::Private, one);
        let is_neq = Boolean::new(Mode::Private, true);

        assert!(Circuit::is_satisfied());
        enforce(a, b, multiplier, is_neq);
        assert!(!Circuit::is_satisfied());
        Circuit::reset();

        // Case 3a: a != b AND is_neq == 0 AND multiplier = 0 (dishonest)
        // ----------------------------------------------------------------

        let a = Field::<Circuit>::new(Mode::Private, five);
        let b = Field::<Circuit>::new(Mode::Private, two);
        let multiplier = Field::<Circuit>::new(Mode::Private, zero);
        let is_neq = Boolean::new(Mode::Private, false);

        assert!(Circuit::is_satisfied());
        enforce(a, b, multiplier, is_neq);
        assert!(!Circuit::is_satisfied());
        Circuit::reset();

        //
        // Case 3b: a != b AND is_neq == 0 AND multiplier = 1 (dishonest)
        // ----------------------------------------------------------------

        let a = Field::<Circuit>::new(Mode::Private, five);
        let b = Field::<Circuit>::new(Mode::Private, two);
        let multiplier = Field::<Circuit>::new(Mode::Private, one);
        let is_neq = Boolean::new(Mode::Private, false);

        assert!(Circuit::is_satisfied());
        enforce(a, b, multiplier, is_neq);
        assert!(!Circuit::is_satisfied());
        Circuit::reset();

        //
        // Case 4a: a != b AND is_neq == 1 AND multiplier = n [!= (a - b)^(-1)] (dishonest)
        // ---------------------------------------------------------------------------------

        let a = Field::<Circuit>::new(Mode::Private, five);
        let b = Field::<Circuit>::new(Mode::Private, two);
        let multiplier = Field::<Circuit>::new(Mode::Private, two);
        let is_neq = Boolean::new(Mode::Private, true);

        assert!(Circuit::is_satisfied());
        enforce(a, b, multiplier, is_neq);
        assert!(!Circuit::is_satisfied());
        Circuit::reset();

        //
        // Case 4b: a != b AND is_neq == 1 AND multiplier = (a - b)^(-1) (honest)
        // ---------------------------------------------------------------------------------

        let a = Field::<Circuit>::new(Mode::Private, five);
        let b = Field::<Circuit>::new(Mode::Private, two);
        let multiplier =
            Field::<Circuit>::new(Mode::Private, (five - two).inverse().expect("Failed to compute a native inverse"));
        let is_neq = Boolean::new(Mode::Private, true);

        assert!(Circuit::is_satisfied());
        enforce(a, b, multiplier, is_neq);
        assert!(Circuit::is_satisfied());
        Circuit::reset();
    }
}
