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

use crate::{Annotation, Identifier, Sanitizer};
use snarkvm_circuits_types::prelude::*;

use core::fmt;

/// An member statement defines a name for an annotation, and is of the form
/// `{identifier} as {annotation};`.
#[derive(Clone, Debug)]
pub struct Member<E: Environment> {
    /// The name of the member.
    name: Identifier<E>,
    /// The annotation of the member.
    annotation: Annotation<E>,
}

impl<E: Environment> Member<E> {
    /// Returns the name of the member.
    #[inline]
    pub fn name(&self) -> &Identifier<E> {
        &self.name
    }

    /// Returns the annotation of the member.
    #[inline]
    pub fn annotation(&self) -> &Annotation<E> {
        &self.annotation
    }
}

impl<E: Environment> Parser for Member<E> {
    type Environment = E;

    /// Parses a string into an member.
    fn parse(string: &str) -> ParserResult<Self> {
        // Parse the whitespace and comments from the string.
        let (string, _) = Sanitizer::parse(string)?;
        // Parse the name from the string.
        let (string, name) = Identifier::parse(string)?;
        // Parse the " as " from the string.
        let (string, _) = tag(" as ")(string)?;
        // Parse the annotation from the string.
        let (string, annotation) = Annotation::parse(string)?;
        // Parse the semicolon ';' keyword from the string.
        let (string, _) = tag(";")(string)?;

        Ok((string, Self { name, annotation }))
    }
}

impl<E: Environment> fmt::Display for Member<E> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{name} as {annotation};", name = self.name, annotation = self.annotation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use snarkvm_circuits_types::environment::Circuit;

    type E = Circuit;

    #[test]
    fn test_member_parse() {
        let member_string = "owner as address.public;";
        let member = Member::<E>::parse(member_string).unwrap().1;
        assert_eq!(member.name(), &Identifier::from_str("owner"));
        assert_eq!(member.annotation(), &Annotation::from_str("address.public"));
    }

    #[test]
    fn test_member_display() {
        let member_string = "owner as address.public;";
        let member = Member::<E>::parse(member_string).unwrap().1;
        assert_eq!(member_string, format!("{member}"));
    }
}
