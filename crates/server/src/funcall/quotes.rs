use serde::{Deserialize, Serialize};

pub const MAX_ITEMS: usize = 50;

#[derive(Debug, Deserialize)]
pub struct QuoteItem {
    pub body: String,
    pub quote: String,
}

#[derive(Debug, Deserialize)]
pub struct QuotesRequest {
    pub items: Vec<QuoteItem>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct QuotesReply {
    pub matched: Vec<bool>,
}

pub fn match_quotes(req: &QuotesRequest) -> QuotesReply {
    QuotesReply { matched: req.items.iter().take(MAX_ITEMS).map(|i| citegate::quote_match(&i.body, &i.quote)).collect() }
}

#[cfg(test)]
mod tests {
    use super::{QuoteItem, QuotesRequest, match_quotes};

    #[test]
    fn each_quote_is_matched_against_its_own_body() {
        let req = QuotesRequest {
            items: vec![
                QuoteItem { body: "영업허가를 취소하거나 영업의 전부 또는 일부를 정지할 수 있다.".into(), quote: "영업허가를 취소하거나 … 정지할 수 있다".into() },
                QuoteItem { body: "영업허가를 취소하거나 영업의 전부 또는 일부를 정지할 수 있다.".into(), quote: "과태료를 부과한다는 지어낸 인용문".into() },
            ],
        };
        assert_eq!(match_quotes(&req).matched, vec![true, false]);
    }
}
