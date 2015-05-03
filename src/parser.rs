extern crate rustc_serialize;

pub use self::Ast::*;

use lexer::Lexer;
use lexer::Token;
use self::rustc_serialize::json::{Json};

use std::iter::Peekable;

/// Parses a JMESPath expression into an AST
pub fn parse(expr: &str) -> Result<Ast, ParseError> {
    Parser::new(expr).parse()
}

/// Represents the abstract syntax tree of a JMESPath expression.
#[derive(Clone, PartialEq, Debug)]
pub enum Ast {
    Comparison(Comparator, Box<Ast>, Box<Ast>),
    CurrentNode,
    Expref(Box<Ast>),
    Flatten(Box<Ast>),
    Function(char, Vec<Box<Ast>>),
    Identifier(String),
    Index(i32),
    Literal(Json),
    MultiList(Vec<Box<Ast>>),
    MultiHash(Vec<KeyValuePair>),
    ArrayProjection(Box<Ast>, Box<Ast>),
    ObjectProjection(Box<Ast>, Box<Ast>),
    Or(Box<Ast>, Box<Ast>),
    Slice(Option<i32>, Option<i32>, Option<i32>),
    Subexpr(Box<Ast>, Box<Ast>),
}

/// Represents a key value pair in a multi-hash
#[derive(Clone, PartialEq, Debug)]
pub struct KeyValuePair {
    key: Box<Ast>,
    value: Box<Ast>
}

/// Comparators (i.e., less than, greater than, etc.)
#[derive(Clone, PartialEq, Debug)]
pub enum Comparator { Eq, Lt, Le, Ne, Ge, Gt }

/// Encountered when an invalid JMESPath expression is parsed.
#[derive(Clone, PartialEq, Debug)]
pub struct ParseError {
    /// The error message.
    msg: String,
    /// The line number of the error.
    line: usize,
    /// The column of the error.
    col: usize,
}

/// JMESPath parser. Returns an Ast
pub struct Parser<'a> {
    /// Peekable token stream
    stream: Peekable<Lexer<'a>>,
    /// Expression being parsed
    expr: String,
    /// The current token
    token: Token,
    /// The current character offset in the expression
    pos: usize,
}

impl<'a> Parser<'a> {
    // Constructs a new lexer using the given expression string.
    pub fn new(expr: &'a str) -> Parser<'a> {
        let mut stream = Lexer::new(expr).peekable();
        let tok0 = stream.next().unwrap();
        Parser {
            stream: stream,
            expr: expr.to_string(),
            token: tok0,
            pos: 0,
        }
    }

    /// Parses the expression into result containing an AST or ParseError.
    pub fn parse(&mut self) -> Result<Ast, ParseError> {
        let result = self.expr(0);
        let token = self.stream.next();
        // After parsing the expr, we should reach the end of the stream.
        if result.is_err() || token.is_none() || token.unwrap() == Token::Eof {
            result
        } else {
            Err(self.err(&"Did not reach token stream EOF"))
        }
    }

    /// Ensures that the next token in the token stream is one of the pipe
    /// separated token named provided as the edible argument (e.g.,
    /// "Identifier|Eof").
    fn expect(&mut self, edible: &str) -> Result<Ast, ParseError> {
        self.advance();
        // Get the string name of the token.
        if edible.contains(&self.token.token_to_string()) {
            Ok(CurrentNode)
        } else {
            Err(self.err(&format!("Expected one of the following tokens: {:?}", edible)))
        }
    }

    /// Advances the cursor position, skipping any whitespace encountered.
    #[inline]
    fn advance(&mut self) {
        loop {
            self.pos += self.token.size();
            match self.stream.next() {
                None => break,
                Some(Token::Whitespace) => continue,
                tok @ _ => { self.token = tok.unwrap(); break }
            }
        }
    }

    /// Main parse function of the Pratt parser that parses while RBP < LBP
    pub fn expr(&mut self, rbp: usize) -> Result<Ast, ParseError> {
        // Parse the nud token.
        let mut left = match self.token.clone() {
            Token::At               => self.nud_at(),
            Token::Identifier(s, _) => self.nud_identifier(s),
            Token::Star             => self.nud_star(),
            Token::Lbracket         => self.nud_lbracket(),
            Token::Flatten          => self.nud_flatten(),
            Token::Literal(v, _)    => self.nud_literal(v),
            Token::Lbrace           => self.nud_lbrace(),
            // Token::Ampersand        => self.nud_ampersand(),
            // Token::Filter           => self.nud_filter(),
            Token::Eof => return Err(self.err(&"Unexpected EOF")),
            _ => return Err(self.err(&"Unexpected nud token"))
        };

        // Parse any led tokens with a higher binding power.
        while rbp < self.token.lbp() {
            left = match self.token {
                Token::Dot      => self.led_dot(left.unwrap()),
                Token::Lbracket => self.led_lbracket(left.unwrap()),
                Token::Flatten  => self.led_flatten(left.unwrap()),
                Token::Or       => self.led_or(left.unwrap()),
                Token::Pipe     => self.led_pipe(left.unwrap()),
                _ => return Err(self.err(&"Unexpected led token")),
            };
        }

        left
    }

    /// Returns a formatted ParseError with the given message.
    fn err(&self, msg: &str) -> ParseError {
        // Find each new line and create a formatted error message.
        let mut line = 0;
        let mut col = self.pos;
        ParseError {
            msg: format!("Error at {:?} token, {}: {}",
                         self.token, self.pos, msg),
            col: col,
            line: line
        }
    }

    /// Examples: "@"
    fn nud_at(&mut self) -> Result<Ast, ParseError> {
        self.advance();
        Ok(CurrentNode)
    }

    /// Examples: "Foo"
    fn nud_identifier(&mut self, s: String) -> Result<Ast, ParseError> {
        self.advance();
        Ok(Identifier(s))
    }

    /// Examples: "[0]", "[*]", "[a, b]", "[0:1]", etc...
    fn nud_lbracket(&mut self) -> Result<Ast, ParseError> {
        self.advance();
        match self.token {
            Token::Number(_, _) | Token::Colon => self.parse_array_index(),
            Token::Star => {
                if self.stream.peek() != Some(&Token::Rbracket) {
                    return self.parse_multi_list();
                }
                try!(self.expect("Star"));
                self.parse_wildcard_index()
            },
            _ => self.parse_multi_list()
        }
    }

    /// Examples: foo[*], foo[0], foo[:-1], etc.
    fn led_lbracket(&mut self, lhs: Ast) -> Result<Ast, ParseError> {
        try!(self.expect("Number|Colon|Star"));
        match self.token {
            Token::Number(_, _) | Token::Colon => self.parse_array_index(),
            _ => self.parse_wildcard_index()
        }
    }

    fn nud_literal(&mut self, value: Json) -> Result<Ast, ParseError> {
        self.advance();
        Ok(Literal(value))
    }

    /// Examples: "*" (e.g., "* | *" would be a pipe containing two nud stars)
    fn nud_star(&mut self) -> Result<Ast, ParseError> {
        self.advance();
        self.parse_wildcard_values(CurrentNode)
    }

    /// Examples: "[]". Turns it into a led flatten (i.e., "@[]").
    fn nud_flatten(&mut self) -> Result<Ast, ParseError> {
        self.led_flatten(CurrentNode)
    }

    /// Example "{foo: bar, baz: `12`}"
    fn nud_lbrace(&mut self) -> Result<Ast, ParseError> {
        let mut pairs = vec![];
        loop {
            // Skip the opening brace and any encountered commas.
            self.advance();
            // Requires at least on key value pair.
            pairs.push(try!(self.parse_kvp()));
            match self.token {
                // Terminal condition is the Rbrace token "}".
                Token::Rbrace => { self.advance(); break; },
                // Skip commas as they are used to delineate kvps
                Token::Comma => continue,
                _ => return Err(self.err("Expected Rbrace or Comma"))
            }
        }
        Ok(MultiHash(pairs))
    }

    fn parse_kvp(&mut self) -> Result<KeyValuePair, ParseError> {
        match self.token.clone() {
            Token::Identifier(name, _) => {
                self.expect("Colon");
                self.advance();
                Ok(KeyValuePair {
                    key: Box::new(Literal(Json::String(name))),
                    value: Box::new(try!(self.expr(0)))
                })
            },
            _ => Err(self.err("Expected Identifier to start key value pair"))
        }
    }

    /// Creates a Projection AST node for a flatten token.
    fn led_flatten(&mut self, lhs: Ast) -> Result<Ast, ParseError> {
        let rhs = try!(self.projection_rhs(Token::Flatten.lbp()));
        Ok(ArrayProjection(
            Box::new(Flatten(Box::new(lhs))),
            Box::new(rhs)
        ))
    }

    fn led_dot(&mut self, left: Ast) -> Result<Ast, ParseError> {
        let rhs = try!(self.parse_dot(Token::Dot.lbp()));
        Ok(Ast::Subexpr(Box::new(left), Box::new(rhs)))
    }

    fn led_or(&mut self, left: Ast) -> Result<Ast, ParseError> {
        self.advance();
        let rhs = try!(self.expr(Token::Or.lbp()));
        Ok(Or(Box::new(left), Box::new(rhs)))
    }

    fn led_pipe(&mut self, left: Ast) -> Result<Ast, ParseError> {
        self.advance();
        let rhs = try!(self.expr(Token::Pipe.lbp()));
        Ok(Subexpr(Box::new(left), Box::new(rhs)))
    }

    /// Parses the right hand side of a dot expression.
    fn parse_dot(&mut self, lbp: usize) -> Result<Ast, ParseError> {
        try!(self.expect("Identifier|Star|Lbrace|Lbracket|Ampersand|Filter"));
        match self.token {
            Token::Lbracket => { self.advance(); self.parse_multi_list() },
            _ => self.expr(lbp)
        }
    }

    /// Parses the right hand side of a projection, using the given LBP to
    /// determine when to stop consuming tokens.
    fn projection_rhs(&mut self, lbp: usize) -> Result<Ast, ParseError> {
        let lbp = self.token.lbp();
        match self.token {
            Token::Dot      => self.parse_dot(lbp),
            Token::Lbracket => self.expr(lbp),
            Token::Filter   => self.expr(lbp),
            _ if lbp < 10   => Ok(CurrentNode),
            _               => Err(self.err("Syntax error found in projection"))
        }
    }

    /// Creates a projection for "[*]"
    fn parse_wildcard_index(&mut self) -> Result<Ast, ParseError> {
        try!(self.expect("Rbracket"));
        let lhs = Box::new(CurrentNode);
        let rhs = try!(self.projection_rhs(Token::Star.lbp()));
        Ok(ArrayProjection(lhs, Box::new(rhs)))
    }

    /// Creates a projection for "*"
    fn parse_wildcard_values(&mut self, lhs: Ast) -> Result<Ast, ParseError> {
        let rhs = try!(self.projection_rhs(Token::Star.lbp()));
        Ok(ObjectProjection(Box::new(lhs), Box::new(rhs)))
    }

    /// Parses [0], [::-1], [0:-1], [0:1], etc...
    fn parse_array_index(&mut self) -> Result<Ast, ParseError> {
        let mut parts = [None, None, None];
        let mut pos = 0;
        loop {
            match self.token {
                Token::Colon => {
                    pos += 1;
                    if pos > 2 {
                        return Err(self.err("Too many colons in slice expr"));
                    }
                    try!(self.expect("Number|Colon|Rbracket"));
                },
                Token::Number(value, _) => {
                    parts[pos] = Some(value);
                    try!(self.expect("Colon|Rbracket"));
                },
                Token::Rbracket => { self.advance(); break; },
                _ => return Err(self.err("Unexpected token")),
            }
        }

        if pos == 0 {
            // No colons were found, so this is a simple index extraction.
            Ok(Index(parts[0].unwrap()))
        } else {
            // Sliced array from start (e.g., [2:])
            let lhs = Slice(parts[0], parts[1], parts[2]);
            let rhs = try!(self.projection_rhs(Token::Star.lbp()));
            Ok(ArrayProjection(Box::new(lhs), Box::new(rhs)))
        }
    }

    /// Parses multi-select lists (e.g., "[foo, bar, baz]")
    fn parse_multi_list(&mut self) -> Result<Ast, ParseError> {
        let mut nodes = vec!();
        loop {
            nodes.push(Box::new(try!(self.expr(0))));
            match self.token {
                Token::Comma    => self.advance(),
                Token::Rbracket => break,
                _               => continue,
            }
        }
        Ok(MultiList(nodes))
    }
}

#[cfg(test)]
mod test {
    extern crate rustc_serialize;

    use super::*;
    use self::rustc_serialize::json::{Json};

    #[test] fn indentifier_test() {
        assert_eq!(parse("foo").unwrap(), Identifier("foo".to_string()));
    }

    #[test] fn current_node_test() {
        assert_eq!(parse("@").unwrap(), CurrentNode);
    }

    #[test] fn wildcard_values_test() {
        assert_eq!(parse("*").unwrap(),
                   ObjectProjection(Box::new(CurrentNode),
                                    Box::new(CurrentNode)));
    }

    #[test] fn dot_test() {
        assert!(parse("@.b").unwrap() == Subexpr(Box::new(CurrentNode),
                                                 Box::new(Identifier("b".to_string()))));
    }

    #[test] fn ensures_nud_token_is_valid_test() {
        let result = parse(",");
        assert!(result.is_err());
        assert!(result.err().unwrap().msg.contains("Unexpected nud token"));
    }

    #[test] fn multi_list_test() {
        let l = MultiList(vec![Box::new(Identifier("a".to_string())),
                               Box::new(Identifier("b".to_string()))]);
        assert_eq!(parse("[a, b]").unwrap(), l);
    }

    #[test] fn multi_list_unclosed() {
        let result = parse("[a, b");
        assert!(result.is_err());
        assert!(result.err().unwrap().msg.contains("Unexpected EOF"));
    }

    #[test] fn multi_list_unclosed_after_comma() {
        let result = parse("[a,");
        assert!(result.is_err());
        assert!(result.err().unwrap().msg.contains("Unexpected EOF"));
    }

    #[test] fn multi_list_after_dot_test() {
        let l = MultiList(vec![Box::new(Identifier("a".to_string())),
                               Box::new(Identifier("b".to_string()))]);
        assert_eq!(parse("@.[a, b]").unwrap(), Subexpr(Box::new(CurrentNode), Box::new(l)));
    }

    #[test] fn parses_simple_index_extractions_test() {
        assert_eq!(parse("[0]").unwrap(), Index(0));
    }

    #[test] fn parses_single_element_slice_test() {
        assert_eq!(parse("[-1:]").unwrap(),
                   ArrayProjection(Box::new(Slice(Some(-1), None, None)),
                                   Box::new(CurrentNode)));
    }

    #[test] fn parses_double_element_slice_test() {
        assert_eq!(parse("[1:-1].a").unwrap(),
                   ArrayProjection(Box::new(Slice(Some(1), Some(-1), None)),
                                   Box::new(Identifier("a".to_string()))));
    }

    #[test] fn parses_revese_slice_test() {
        assert_eq!(parse("[::-1].a").unwrap(),
                   ArrayProjection(Box::new(Slice(None, None, Some(-1))),
                                   Box::new(Identifier("a".to_string()))));
    }

    #[test] fn parses_or_test() {
        assert_eq!(parse("a || b").unwrap(),
                   Or(Box::new(Identifier("a".to_string())),
                      Box::new(Identifier("b".to_string()))));
    }

    #[test] fn parses_pipe_test() {
        assert_eq!(parse("a | b").unwrap(),
                   Subexpr(Box::new(Identifier("a".to_string())),
                      Box::new(Identifier("b".to_string()))));
    }

    #[test] fn parses_literal_token_test() {
        assert_eq!(parse("`\"foo\"`").unwrap(),
                   Literal(Json::String("foo".to_string())))
    }

    #[test] fn parses_multi_hash() {
        let result = MultiHash(vec![
            KeyValuePair {
                key: Box::new(Literal(Json::String("foo".to_string()))),
                value: Box::new(Identifier("bar".to_string()))
            },
            KeyValuePair {
                key: Box::new(Literal(Json::String("baz".to_string()))),
                value: Box::new(Identifier("bam".to_string()))
            }
        ]);
        assert_eq!(parse("{foo: bar, baz: bam}").unwrap(), result);
    }
}
