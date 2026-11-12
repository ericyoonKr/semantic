use std::future::Future;
use std::pin::Pin;

use crate::error::RetrievalError;

pub const DIM: usize = 768;

pub type EmbedFuture<'a> = Pin<Box<dyn Future<Output = Result<Vec<f32>, RetrievalError>> + Send + 'a>>;

pub trait Embedder: Send + Sync {
    fn registry_name(&self) -> &str;
    fn embed<'a>(&'a self, text: &'a str) -> EmbedFuture<'a>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct NoEmbedder;

impl Embedder for NoEmbedder {
    fn registry_name(&self) -> &str {
        ""
    }

    fn embed<'a>(&'a self, _text: &'a str) -> EmbedFuture<'a> {
        Box::pin(async { Err(RetrievalError::EmbedderDown) })
    }
}

pub fn vector_literal(v: &[f32]) -> Result<String, RetrievalError> {
    if v.len() != DIM {
        return Err(RetrievalError::Dimension(v.len()));
    }
    if v.iter().any(|x| !x.is_finite()) {
        return Err(RetrievalError::EmbedMalformed);
    }
    let parts: Vec<String> = v.iter().map(f32::to_string).collect();
    Ok(format!("[{}]", parts.join(",")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_checks_length_and_finiteness() {
        assert!(matches!(vector_literal(&[0.5; 3]), Err(RetrievalError::Dimension(3))));
        let mut v = vec![0.25_f32; DIM];
        let text = vector_literal(&v).unwrap_or_default();
        assert!(text.starts_with("[0.25,0.25"));
        assert!(text.ends_with("0.25]"));
        v[7] = f32::NAN;
        assert!(matches!(vector_literal(&v), Err(RetrievalError::EmbedMalformed)));
    }
}
