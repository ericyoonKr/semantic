use crate::error::ApiError;

const MAX_LEN: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionId(String);

impl SessionId {
    pub fn parse(raw: &str) -> Result<Self, ApiError> {
        let ok = !raw.is_empty()
            && raw.len() <= MAX_LEN
            && raw
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        if ok {
            Ok(Self(raw.to_string()))
        } else {
            Err(ApiError::bad_request("세션 ID 형식이 맞지 않습니다"))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::SessionId;

    #[test]
    fn only_plain_ids_are_accepted() {
        assert!(SessionId::parse("6f1c2a9e-0b7d-4a51-9c3e-2d8f1a7b4c60").is_ok());
        for bad in ["", "a/b", "a b", "..", "가", &"x".repeat(65)] {
            assert!(SessionId::parse(bad).is_err(), "{bad}");
        }
    }
}
