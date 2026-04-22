use serde::de::DeserializeOwned;
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CopyError {
    #[error("serialize/deserialize error")]
    Serde(#[from] serde_json::Error),
}

pub fn clone_via_json<TIn, TOut>(input: &TIn) -> Result<TOut, CopyError>
where
    TIn: Serialize,
    TOut: DeserializeOwned,
{
    let json = serde_json::to_value(input)?;
    Ok(serde_json::from_value(json)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize)]
    struct In {
        id: u32,
    }

    #[derive(Debug, Serialize, Deserialize)]
    struct Out {
        id: u32,
    }

    #[test]
    fn copies_between_compatible_structs() {
        let out: Out = clone_via_json(&In { id: 9 }).expect("copy");
        assert_eq!(out.id, 9);
    }
}
