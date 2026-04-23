use rand::distributions::{Alphanumeric, DistString};
use uuid::Uuid;

pub fn random_string(len: usize) -> String {
    Alphanumeric.sample_string(&mut rand::thread_rng(), len.max(1))
}

pub fn uuid_v4() -> String {
    Uuid::new_v4().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_random_string() {
        let s = random_string(12);
        assert_eq!(s.len(), 12);
    }

    #[test]
    fn generates_uuid() {
        let id = uuid_v4();
        assert_eq!(id.len(), 36);
    }
}
