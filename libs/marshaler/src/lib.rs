use serde::de::DeserializeOwned;
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MarshalError {
    #[error("json error")]
    Json(#[from] serde_json::Error),
}

pub fn to_json<T: Serialize>(value: &T) -> Result<String, MarshalError> {
    Ok(serde_json::to_string(value)?)
}

pub fn from_json<T: DeserializeOwned>(s: &str) -> Result<T, MarshalError> {
    Ok(serde_json::from_str::<T>(s)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct Sample {
        id: u8,
    }

    #[test]
    fn roundtrip_json() {
        let s = to_json(&Sample { id: 7 }).expect("encode");
        let out: Sample = from_json(&s).expect("decode");
        assert_eq!(out, Sample { id: 7 });
    }
}
