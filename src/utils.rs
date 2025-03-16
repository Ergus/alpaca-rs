// Copyright (C) 2025  Jimmy Aguilar Mena

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

// Serialize a HeaderMap to a map of string keys and string values


use std::{fmt, str::FromStr};

use serde::{Serialize, Deserialize, Serializer};
use serde::ser::SerializeMap;
use reqwest::header::HeaderMap;

pub fn serialize_headers<S>(headers: &HeaderMap, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut map = serializer.serialize_map(Some(headers.len()))?;

    for (name, value) in headers.iter() {
        let name_str = name.as_str();
        if let Ok(value_str) = value.to_str() {
            map.serialize_entry(name_str, value_str)?;
        }
    }

    map.end()
}


#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Hash, std::cmp::Eq)]
pub enum PriceType {
    Trades,
    Quotes,
    Bars,
}

impl fmt::Display for PriceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Trades => write!(f, "trades"),
            Self::Quotes => write!(f, "quotes"),
            Self::Bars => write!(f, "bars"),
        }
    }
}

impl FromStr for PriceType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "trades" => Ok(Self::Trades),
            "quotes" => Ok(Self::Quotes),
            "bars" => Ok(Self::Bars),
            _ => Err(format!("Invalid value: {}. Expected one of: trades, quotes, bars", s)),
        }
    }
}


#[derive(Debug, Serialize, Deserialize)]
pub struct AtomicF64 {
    storage: atomic::AtomicU64,
}
impl AtomicF64 {
    pub fn new(value: f64) -> Self {
        let as_u64 = value.to_bits();
        Self { storage: atomic::AtomicU64::new(as_u64) }
    }
    pub fn store(&self, value: f64, ordering: atomic::Ordering) {
        let as_u64 = value.to_bits();
        self.storage.store(as_u64, ordering)
    }
    pub fn load(&self, ordering: atomic::Ordering) -> f64 {
        let as_u64 = self.storage.load(ordering);
        f64::from_bits(as_u64)
    }
}

impl Default for AtomicF64 {
    fn default() -> Self {
        Self::new(0.0)
    }
}

