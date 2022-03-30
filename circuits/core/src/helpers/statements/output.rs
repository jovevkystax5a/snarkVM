// Copyright (C) 2019-2022 Aleo Systems Inc.
// This file is part of the snarkVM library.

// The snarkVM library is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// The snarkVM library is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with the snarkVM library. If not, see <https://www.gnu.org/licenses/>.

use crate::{Annotation, Locator, Register};
use snarkvm_circuits_types::prelude::*;

use core::fmt;

/// The output statement defines an output of a function, and may refer to the value
/// in either a register or a register member. The output statement is of the form
/// `output {register} as {annotation}`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Output<E: Environment> {
    /// The output register.
    register: Register<E>,
    /// The output annotation.
    annotation: Annotation<E>,
}

impl<E: Environment> Output<E> {
    /// Initializes a new output.
    #[inline]
    pub fn new(register: Register<E>, annotation: Annotation<E>) -> Self {
        Self { register, annotation }
    }

    /// Returns the output register.
    #[inline]
    pub fn register(&self) -> &Register<E> {
        &self.register
    }

    /// Returns the output register locator.
    #[inline]
    pub fn locator(&self) -> &Locator {
        self.register.locator()
    }

    /// Returns the output annotation.
    #[inline]
    pub fn annotation(&self) -> &Annotation<E> {
        &self.annotation
    }

    /// Returns `true` if the output is a literal.
    /// Returns `false` if the output is a composite or record.
    pub fn is_literal(&self) -> bool {
        self.annotation.is_literal()
    }

    /// Returns `true` if the output is a composite.
    /// Returns `false` if the output is a literal or record.
    pub fn is_composite(&self) -> bool {
        self.annotation.is_composite()
    }

    /// Returns `true` if the output is a record.
    /// Returns `false` if the output is a literal or composite.
    pub fn is_record(&self) -> bool {
        self.annotation.is_record()
    }
}

impl<E: Environment> TypeName for Output<E> {
    /// Returns the type name as a string.
    #[inline]
    fn type_name() -> &'static str {
        "output"
    }
}

impl<E: Environment> Parser for Output<E> {
    type Environment = E;

    /// Parses a string into an output statement.
    /// The output statement is of the form `output {register} as {annotation}`.
    #[inline]
    fn parse(string: &str) -> ParserResult<Self> {
        // Parse the output keyword from the string.
        let (string, _) = tag(Self::type_name())(string)?;
        // Parse the space from the string.
        let (string, _) = tag(" ")(string)?;
        // Parse the register from the string.
        let (string, register) = Register::parse(string)?;
        // Parse the " as " from the string.
        let (string, _) = tag(" as ")(string)?;
        // Parse the annotation from the string.
        let (string, annotation) = Annotation::parse(string)?;
        // Parse the semicolon from the string.
        let (string, _) = tag(";")(string)?;
        // Return the output statement.
        Ok((string, Self { register, annotation }))
    }
}

impl<E: Environment> fmt::Display for Output<E> {
    /// Prints the output statement as a string.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{type_} {register} as {annotation};",
            type_ = Self::type_name(),
            register = self.register,
            annotation = self.annotation
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Identifier, LiteralType};
    use snarkvm_circuits_types::environment::Circuit;

    type E = Circuit;

    #[test]
    fn test_output_type_name() {
        assert_eq!(Output::<E>::type_name(), "output");
    }

    #[test]
    fn test_output_parse() {
        // Literal
        let output = Output::<E>::parse("output r0 as field.private;").unwrap().1;
        assert_eq!(output.register(), &Register::<E>::Locator(0));
        assert_eq!(output.annotation(), &Annotation::<E>::Literal(LiteralType::Field(Mode::Private)));

        // Composite
        let output = Output::<E>::parse("output r1 as signature;").unwrap().1;
        assert_eq!(output.register(), &Register::<E>::Locator(1));
        assert_eq!(output.annotation(), &Annotation::<E>::Composite(Identifier::new("signature")));

        // Record
        let output = Output::<E>::parse("output r2 as record;").unwrap().1;
        assert_eq!(output.register(), &Register::<E>::Locator(2));
        assert_eq!(output.annotation(), &Annotation::<E>::Record);
    }

    #[test]
    fn test_output_display() {
        // Literal
        let output = Output::<E>::parse("output r0 as field.private;").unwrap().1;
        assert_eq!(format!("{}", output), "output r0 as field.private;");

        // Composite
        let output = Output::<E>::parse("output r1 as signature;").unwrap().1;
        assert_eq!(format!("{}", output), "output r1 as signature;");

        // Record
        let output = Output::<E>::parse("output r2 as record;").unwrap().1;
        assert_eq!(format!("{}", output), "output r2 as record;");
    }

    #[test]
    fn test_output_locator() {
        // Literal
        let output = Output::<E>::parse("output r0 as field.private;").unwrap().1;
        assert_eq!(output.locator(), &0);

        // Composite
        let output = Output::<E>::parse("output r1 as signature;").unwrap().1;
        assert_eq!(output.locator(), &1);

        // Record
        let output = Output::<E>::parse("output r2 as record;").unwrap().1;
        assert_eq!(output.locator(), &2);
    }

    #[test]
    fn test_output_is_literal() {
        // Literal
        let output = Output::<E>::parse("output r0 as field.private;").unwrap().1;
        assert!(output.is_literal());

        // Composite
        let output = Output::<E>::parse("output r1 as signature;").unwrap().1;
        assert!(!output.is_literal());

        // Record
        let output = Output::<E>::parse("output r2 as record;").unwrap().1;
        assert!(!output.is_literal());
    }

    #[test]
    fn test_output_is_composite() {
        // Literal
        let output = Output::<E>::parse("output r0 as field.private;").unwrap().1;
        assert!(!output.is_composite());

        // Composite
        let output = Output::<E>::parse("output r1 as signature;").unwrap().1;
        assert!(output.is_composite());

        // Record
        let output = Output::<E>::parse("output r2 as record;").unwrap().1;
        assert!(!output.is_composite());
    }

    #[test]
    fn test_output_is_record() {
        // Literal
        let output = Output::<E>::parse("output r0 as field.private;").unwrap().1;
        assert!(!output.is_record());

        // Composite
        let output = Output::<E>::parse("output r1 as signature;").unwrap().1;
        assert!(!output.is_record());

        // Record
        let output = Output::<E>::parse("output r2 as record;").unwrap().1;
        assert!(output.is_record());
    }
}
