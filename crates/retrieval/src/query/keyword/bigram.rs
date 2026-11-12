use tantivy::tokenizer::{Token, TokenStream, Tokenizer};

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn words(text: &str) -> Vec<(usize, Vec<(usize, char)>)> {
    let mut out: Vec<(usize, Vec<(usize, char)>)> = Vec::new();
    let mut current: Vec<(usize, char)> = Vec::new();
    for (at, c) in text.char_indices() {
        if is_word(c) {
            current.push((at, c));
        } else if !current.is_empty() {
            out.push((current[0].0, std::mem::take(&mut current)));
        }
    }
    if !current.is_empty() {
        out.push((current[0].0, current));
    }
    out
}

fn end_of(chars: &[(usize, char)], i: usize) -> usize {
    chars[i].0 + chars[i].1.len_utf8()
}

fn pieces(text: &str) -> Vec<Token> {
    let mut out = Vec::new();
    for (_, chars) in words(text) {
        let spans: Vec<(usize, usize)> = if chars.len() == 1 { vec![(0, 0)] } else { (0..chars.len() - 1).map(|i| (i, i + 1)).collect() };
        for (a, b) in spans {
            let text: String = chars[a..=b].iter().flat_map(|(_, c)| c.to_lowercase()).collect();
            out.push(Token { offset_from: chars[a].0, offset_to: end_of(&chars, b), position: out.len(), text, position_length: 1 });
        }
    }
    out
}

pub fn tokens(text: &str) -> Vec<String> {
    pieces(text).into_iter().map(|t| t.text).collect()
}

#[derive(Debug, Clone, Default)]
pub struct BigramTokenizer;

pub struct BigramStream {
    tokens: Vec<Token>,
    next: usize,
    current: Token,
}

impl Tokenizer for BigramTokenizer {
    type TokenStream<'a> = BigramStream;

    fn token_stream<'a>(&'a mut self, text: &'a str) -> BigramStream {
        BigramStream { tokens: pieces(text), next: 0, current: Token::default() }
    }
}

impl TokenStream for BigramStream {
    fn advance(&mut self) -> bool {
        match self.tokens.get_mut(self.next) {
            Some(t) => {
                self.current = std::mem::take(t);
                self.next += 1;
                true
            }
            None => false,
        }
    }

    fn token(&self) -> &Token {
        &self.current
    }

    fn token_mut(&mut self) -> &mut Token {
        &mut self.current
    }
}

#[cfg(test)]
mod tests {
    use super::{BigramTokenizer, tokens};
    use tantivy::tokenizer::{TokenStream, Tokenizer};

    #[test]
    fn words_become_overlapping_pairs_and_single_letters_stay() {
        assert_eq!(tokens("처분의 사전 통지"), ["처분", "분의", "사전", "통지"]);
        assert_eq!(tokens("「행정절차법」 제21조 및 A"), ["행정", "정절", "절차", "차법", "제2", "21", "1조", "및", "a"]);
        assert_eq!(tokens(" ,. "), Vec::<String>::new());
    }

    #[test]
    fn tokenizer_stream_matches_token_list_with_byte_offsets() {
        let mut t = BigramTokenizer;
        let mut stream = t.token_stream("가나다 라");
        let mut got = Vec::new();
        while stream.advance() {
            let k = stream.token();
            got.push((k.text.clone(), k.offset_from, k.offset_to, k.position));
        }
        assert_eq!(got, [("가나".to_string(), 0, 6, 0), ("나다".to_string(), 3, 9, 1), ("라".to_string(), 10, 13, 2)]);
    }
}
