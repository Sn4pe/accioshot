// SPDX-License-Identifier: GPL-3.0-or-later

use serde::{Serialize, Serializer};

#[derive(Debug)]
pub struct Error(String);

impl<E: std::fmt::Display> From<E> for Error {
    fn from(error: E) -> Self {
        Error(error.to_string())
    }
}

impl Serialize for Error {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
