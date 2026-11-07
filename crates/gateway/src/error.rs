#[derive(Debug, thiserror::Error)]
pub enum GatewayError {
    #[error("password does not meet the policy")]
    WeakPassword,
    #[error("login failed")]
    LoginFailed,
    #[error("account locked")]
    Locked,
    #[error("not signed in")]
    Unauthenticated,
    #[error("csrf token mismatch")]
    Csrf,
    #[error("not found")]
    NotFound,
    #[error("too many requests")]
    TooMany,
    #[error("internal error")]
    Internal,
}
