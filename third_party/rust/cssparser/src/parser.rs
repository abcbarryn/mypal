/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use cow_rc_str::CowRcStr;
use std::ops::Range;
use std::ascii::AsciiExt;
use std::ops::BitOr;
use tokenizer::{self, Token, Tokenizer, SourceLocation};


/// A capture of the internal state of a `Parser` (including the position within the input),
/// obtained from the `Parser::position` method.
///
/// Can be used with the `Parser::reset` method to restore that state.
/// Should only be used with the `Parser` instance it came from.
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub struct SourcePosition {
    position: tokenizer::SourcePosition,
    at_start_of: Option<BlockType>,
}

/// The funamental parsing errors that can be triggered by built-in parsing routines.
#[derive(Clone, Debug, PartialEq)]
pub enum BasicParseError<'a> {
    /// An unexpected token was encountered.
    UnexpectedToken(Token<'a>),
    /// The end of the input was encountered unexpectedly.
    EndOfInput,
    /// An `@` rule was encountered that was invalid.
    AtRuleInvalid(CowRcStr<'a>),
    /// The body of an '@' rule was invalid.
    AtRuleBodyInvalid,
    /// A qualified rule was encountered that was invalid.
    QualifiedRuleInvalid,
}

impl<'a, T> From<BasicParseError<'a>> for ParseError<'a, T> {
    fn from(this: BasicParseError<'a>) -> ParseError<'a, T> {
        ParseError::Basic(this)
    }
}

/// Extensible parse errors that can be encountered by client parsing implementations.
#[derive(Clone, Debug, PartialEq)]
pub enum ParseError<'a, T: 'a> {
    /// A fundamental parse error from a built-in parsing routine.
    Basic(BasicParseError<'a>),
    /// A parse error reported by downstream consumer code.
    Custom(T),
}

impl<'a, T> ParseError<'a, T> {
    /// Extract the fundamental parse error from an extensible error.
    pub fn basic(self) -> BasicParseError<'a> {
        match self {
            ParseError::Basic(e) => e,
            ParseError::Custom(_) => panic!("Not a basic parse error"),
        }
    }
}

/// The owned input for a parser.
pub struct ParserInput<'i> {
    tokenizer: Tokenizer<'i>,
    cached_token: Option<CachedToken<'i>>,
}

struct CachedToken<'i> {
    token: Token<'i>,
    start_position: tokenizer::SourcePosition,
    end_position: tokenizer::SourcePosition,
}

impl<'i> ParserInput<'i> {
    /// Create a new input for a parser.
    pub fn new(input: &'i str) -> ParserInput<'i> {
        ParserInput {
            tokenizer: Tokenizer::new(input),
            cached_token: None,
        }
    }

    #[inline]
    fn cached_token_ref(&self) -> &Token<'i> {
        &self.cached_token.as_ref().unwrap().token
    }
}

/// A CSS parser that borrows its `&str` input,
/// yields `Token`s,
/// and keeps track of nested blocks and functions.
pub struct Parser<'i: 't, 't> {
    input: &'t mut ParserInput<'i>,
    /// If `Some(_)`, .parse_nested_block() can be called.
    at_start_of: Option<BlockType>,
    /// For parsers from `parse_until` or `parse_nested_block`
    stop_before: Delimiters,
}


#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum BlockType {
    Parenthesis,
    SquareBracket,
    CurlyBracket,
}


impl BlockType {
    fn opening(token: &Token) -> Option<BlockType> {
        match *token {
            Token::Function(_) |
            Token::ParenthesisBlock => Some(BlockType::Parenthesis),
            Token::SquareBracketBlock => Some(BlockType::SquareBracket),
            Token::CurlyBracketBlock => Some(BlockType::CurlyBracket),
            _ => None
        }
    }

    fn closing(token: &Token) -> Option<BlockType> {
        match *token {
            Token::CloseParenthesis => Some(BlockType::Parenthesis),
            Token::CloseSquareBracket => Some(BlockType::SquareBracket),
            Token::CloseCurlyBracket => Some(BlockType::CurlyBracket),
            _ => None
        }
    }
}


/// A set of characters, to be used with the `Parser::parse_until*` methods.
///
/// The union of two sets can be obtained with the `|` operator. Example:
///
/// ```{rust,ignore}
/// input.parse_until_before(Delimiter::CurlyBracketBlock | Delimiter::Semicolon)
/// ```
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Delimiters {
    bits: u8,
}

/// `Delimiters` constants.
#[allow(non_upper_case_globals, non_snake_case)]
pub mod Delimiter {
    use super::Delimiters;

    /// The empty delimiter set
    pub const None: Delimiters = Delimiters { bits: 0 };
    /// The delimiter set with only the `{` opening curly bracket
    pub const CurlyBracketBlock: Delimiters = Delimiters { bits: 1 << 1 };
    /// The delimiter set with only the `;` semicolon
    pub const Semicolon: Delimiters = Delimiters { bits: 1 << 2 };
    /// The delimiter set with only the `!` exclamation point
    pub const Bang: Delimiters = Delimiters { bits: 1 << 3 };
    /// The delimiter set with only the `,` comma
    pub const Comma: Delimiters = Delimiters { bits: 1 << 4 };
}

#[allow(non_upper_case_globals, non_snake_case)]
mod ClosingDelimiter {
    use super::Delimiters;

    pub const CloseCurlyBracket: Delimiters = Delimiters { bits: 1 << 5 };
    pub const CloseSquareBracket: Delimiters = Delimiters { bits: 1 << 6 };
    pub const CloseParenthesis: Delimiters = Delimiters { bits: 1 << 7 };
}

impl BitOr<Delimiters> for Delimiters {
    type Output = Delimiters;

    fn bitor(self, other: Delimiters) -> Delimiters {
        Delimiters { bits: self.bits | other.bits }
    }
}

impl Delimiters {
    fn contains(self, other: Delimiters) -> bool {
        (self.bits & other.bits) != 0
    }

    fn from_byte(byte: Option<u8>) -> Delimiters {
        match byte {
            Some(b';') => Delimiter::Semicolon,
            Some(b'!') => Delimiter::Bang,
            Some(b',') => Delimiter::Comma,
            Some(b'{') => Delimiter::CurlyBracketBlock,
            Some(b'}') => ClosingDelimiter::CloseCurlyBracket,
            Some(b']') => ClosingDelimiter::CloseSquareBracket,
            Some(b')') => ClosingDelimiter::CloseParenthesis,
            _ => Delimiter::None,
        }
    }
}

impl<'i: 't, 't> Parser<'i, 't> {
    /// Create a new parser
    #[inline]
    pub fn new(input: &'t mut ParserInput<'i>) -> Parser<'i, 't> {
        Parser {
            input: input,
            at_start_of: None,
            stop_before: Delimiter::None,
        }
    }

    /// Return the current line that is being parsed.
    pub fn current_line(&self) -> &'i str {
        self.input.tokenizer.current_source_line()
    }

    /// Check whether the input is exhausted. That is, if `.next()` would return a token.
    ///
    /// This ignores whitespace and comments.
    #[inline]
    pub fn is_exhausted(&mut self) -> bool {
        self.expect_exhausted().is_ok()
    }

    /// Check whether the input is exhausted. That is, if `.next()` would return a token.
    /// Return a `Result` so that the `try!` macro can be used: `try!(input.expect_exhausted())`
    ///
    /// This ignores whitespace and comments.
    #[inline]
    pub fn expect_exhausted(&mut self) -> Result<(), BasicParseError<'i>> {
        let start_position = self.position();
        let result = match self.next() {
            Err(BasicParseError::EndOfInput) => Ok(()),
            Err(e) => unreachable!("Unexpected error encountered: {:?}", e),
            Ok(t) => Err(BasicParseError::UnexpectedToken(t.clone())),
        };
        self.reset(start_position);
        result
    }

    /// Return the current internal state of the parser (including position within the input).
    ///
    /// This state can later be restored with the `Parser::reset` method.
    #[inline]
    pub fn position(&self) -> SourcePosition {
        SourcePosition {
            position: self.input.tokenizer.position(),
            at_start_of: self.at_start_of,
        }
    }

    /// Restore the internal state of the parser (including position within the input)
    /// to what was previously saved by the `Parser::position` method.
    ///
    /// Should only be used with `SourcePosition` values from the same `Parser` instance.
    #[inline]
    pub fn reset(&mut self, new_position: SourcePosition) {
        self.input.tokenizer.reset(new_position.position);
        self.at_start_of = new_position.at_start_of;
    }

    /// Start looking for `var()` functions. (See the `.seen_var_functions()` method.)
    #[inline]
    pub fn look_for_var_functions(&mut self) {
        self.input.tokenizer.look_for_var_functions()
    }

    /// Return whether a `var()` function has been seen by the tokenizer since
    /// either `look_for_var_functions` was called, and stop looking.
    #[inline]
    pub fn seen_var_functions(&mut self) -> bool {
        self.input.tokenizer.seen_var_functions()
    }

    /// Start looking for viewport percentage lengths. (See the `seen_viewport_percentages`
    /// method.)
    #[inline]
    pub fn look_for_viewport_percentages(&mut self) {
        self.input.tokenizer.look_for_viewport_percentages()
    }

    /// Return whether a `vh`, `vw`, `vmin`, or `vmax` dimension has been seen by the tokenizer
    /// since `look_for_viewport_percentages` was called, and stop looking.
    #[inline]
    pub fn seen_viewport_percentages(&mut self) -> bool {
        self.input.tokenizer.seen_viewport_percentages()
    }

    /// Execute the given closure, passing it the parser.
    /// If the result (returned unchanged) is `Err`,
    /// the internal state of the parser  (including position within the input)
    /// is restored to what it was before the call.
    #[inline]
    pub fn try<F, T, E>(&mut self, thing: F) -> Result<T, E>
    where F: FnOnce(&mut Parser<'i, 't>) -> Result<T, E> {
        let start_position = self.position();
        let result = thing(self);
        if result.is_err() {
            self.reset(start_position)
        }
        result
    }

    /// Return a slice of the CSS input
    #[inline]
    pub fn slice(&self, range: Range<SourcePosition>) -> &'i str {
        self.input.tokenizer.slice(range.start.position..range.end.position)
    }

    /// Return a slice of the CSS input, from the given position to the current one.
    #[inline]
    pub fn slice_from(&self, start_position: SourcePosition) -> &'i str {
        self.input.tokenizer.slice_from(start_position.position)
    }

    /// Return the line and column number within the input for the current position.
    #[inline]
    pub fn current_source_location(&self) -> SourceLocation {
        self.input.tokenizer.current_source_location()
    }

    /// Return the line and column number within the input for the given position.
    #[inline]
    pub fn source_location(&self, target: SourcePosition) -> SourceLocation {
        self.input.tokenizer.source_location(target.position)
    }

    /// Return the next token in the input that is neither whitespace or a comment,
    /// and advance the position accordingly.
    ///
    /// After returning a `Function`, `ParenthesisBlock`,
    /// `CurlyBracketBlock`, or `SquareBracketBlock` token,
    /// the next call will skip until after the matching `CloseParenthesis`,
    /// `CloseCurlyBracket`, or `CloseSquareBracket` token.
    ///
    /// See the `Parser::parse_nested_block` method to parse the content of functions or blocks.
    ///
    /// This only returns a closing token when it is unmatched (and therefore an error).
    pub fn next(&mut self) -> Result<&Token<'i>, BasicParseError<'i>> {
        loop {
            match self.next_including_whitespace_and_comments() {
                Err(e) => return Err(e),
                Ok(&Token::WhiteSpace(_)) | Ok(&Token::Comment(_)) => {},
                _ => break
            }
        }
        Ok(self.input.cached_token_ref())
    }

    /// Same as `Parser::next`, but does not skip whitespace tokens.
    pub fn next_including_whitespace(&mut self) -> Result<&Token<'i>, BasicParseError<'i>> {
        loop {
            match self.next_including_whitespace_and_comments() {
                Err(e) => return Err(e),
                Ok(&Token::Comment(_)) => {},
                _ => break
            }
        }
        Ok(self.input.cached_token_ref())
    }

    /// Same as `Parser::next`, but does not skip whitespace or comment tokens.
    ///
    /// **Note**: This should only be used in contexts like a CSS pre-processor
    /// where comments are preserved.
    /// When parsing higher-level values, per the CSS Syntax specification,
    /// comments should always be ignored between tokens.
    pub fn next_including_whitespace_and_comments(&mut self) -> Result<&Token<'i>, BasicParseError<'i>> {
        if let Some(block_type) = self.at_start_of.take() {
            consume_until_end_of_block(block_type, &mut self.input.tokenizer);
        }

        let byte = self.input.tokenizer.next_byte();
        if self.stop_before.contains(Delimiters::from_byte(byte)) {
            return Err(BasicParseError::EndOfInput)
        }

        let token_start_position = self.input.tokenizer.position();
        let token;
        match self.input.cached_token {
            Some(ref cached_token) if cached_token.start_position == token_start_position => {
                self.input.tokenizer.reset(cached_token.end_position);
                match cached_token.token {
                    Token::Dimension { ref unit, .. } => self.input.tokenizer.see_dimension(unit),
                    Token::Function(ref name) => self.input.tokenizer.see_function(name),
                    _ => {}
                }
                token = &cached_token.token
            }
            _ => {
                let new_token = self.input.tokenizer.next().map_err(|()| BasicParseError::EndOfInput)?;
                self.input.cached_token = Some(CachedToken {
                    token: new_token,
                    start_position: token_start_position,
                    end_position: self.input.tokenizer.position(),
                });
                token = self.input.cached_token_ref()
            }
        }

        if let Some(block_type) = BlockType::opening(token) {
            self.at_start_of = Some(block_type);
        }
        Ok(token)
    }

    /// Have the given closure parse something, then check the the input is exhausted.
    /// The result is overridden to `Err(())` if some input remains.
    ///
    /// This can help tell e.g. `color: green;` from `color: green 4px;`
    #[inline]
    pub fn parse_entirely<F, T, E>(&mut self, parse: F) -> Result<T, ParseError<'i, E>>
    where F: FnOnce(&mut Parser<'i, 't>) -> Result<T, ParseError<'i, E>> {
        let result = parse(self)?;
        self.expect_exhausted()?;
        Ok(result)
    }

    /// Parse a list of comma-separated values, all with the same syntax.
    ///
    /// The given closure is called repeatedly with a "delimited" parser
    /// (see the `Parser::parse_until_before` method)
    /// so that it can over consume the input past a comma at this block/function nesting level.
    ///
    /// Successful results are accumulated in a vector.
    ///
    /// This method retuns `Err(())` the first time that a closure call does,
    /// or if a closure call leaves some input before the next comma or the end of the input.
    #[inline]
    pub fn parse_comma_separated<F, T, E>(&mut self, mut parse_one: F) -> Result<Vec<T>, ParseError<'i, E>>
    where F: for<'tt> FnMut(&mut Parser<'i, 'tt>) -> Result<T, ParseError<'i, E>> {
        let mut values = vec![];
        loop {
            values.push(self.parse_until_before(Delimiter::Comma, &mut parse_one)?);
            match self.next() {
                Err(_) => return Ok(values),
                Ok(&Token::Comma) => continue,
                Ok(_) => unreachable!(),
            }
        }
    }

    /// Parse the content of a block or function.
    ///
    /// This method panics if the last token yielded by this parser
    /// (from one of the `next*` methods)
    /// is not a on that marks the start of a block or function:
    /// a `Function`, `ParenthesisBlock`, `CurlyBracketBlock`, or `SquareBracketBlock`.
    ///
    /// The given closure is called with a "delimited" parser
    /// that stops at the end of the block or function (at the matching closing token).
    ///
    /// The result is overridden to `Err(())` if the closure leaves some input before that point.
    #[inline]
    pub fn parse_nested_block<F, T, E>(&mut self, parse: F) -> Result <T, ParseError<'i, E>>
    where F: for<'tt> FnOnce(&mut Parser<'i, 'tt>) -> Result<T, ParseError<'i, E>> {
        parse_nested_block(self, parse)
    }

    /// Limit parsing to until a given delimiter or the end of the input. (E.g.
    /// a semicolon for a property value.)
    ///
    /// The given closure is called with a "delimited" parser
    /// that stops before the first character at this block/function nesting level
    /// that matches the given set of delimiters, or at the end of the input.
    ///
    /// The result is overridden to `Err(())` if the closure leaves some input before that point.
    #[inline]
    pub fn parse_until_before<F, T, E>(&mut self, delimiters: Delimiters, parse: F)
                                    -> Result <T, ParseError<'i, E>>
    where F: for<'tt> FnOnce(&mut Parser<'i, 'tt>) -> Result<T, ParseError<'i, E>> {
        parse_until_before(self, delimiters, parse)
    }

    /// Like `parse_until_before`, but also consume the delimiter token.
    ///
    /// This can be useful when you don???t need to know which delimiter it was
    /// (e.g. if these is only one in the given set)
    /// or if it was there at all (as opposed to reaching the end of the input).
    #[inline]
    pub fn parse_until_after<F, T, E>(&mut self, delimiters: Delimiters, parse: F)
                                   -> Result <T, ParseError<'i, E>>
    where F: for<'tt> FnOnce(&mut Parser<'i, 'tt>) -> Result<T, ParseError<'i, E>> {
        parse_until_after(self, delimiters, parse)
    }

    /// Parse a <whitespace-token> and return its value.
    #[inline]
    pub fn expect_whitespace(&mut self) -> Result<&'i str, BasicParseError<'i>> {
        match *self.next_including_whitespace()? {
            Token::WhiteSpace(value) => Ok(value),
            ref t => Err(BasicParseError::UnexpectedToken(t.clone()))
        }
    }

    /// Parse a <ident-token> and return the unescaped value.
    #[inline]
    pub fn expect_ident(&mut self) -> Result<&CowRcStr<'i>, BasicParseError<'i>> {
        match *self.next()? {
            Token::Ident(ref value) => Ok(value),
            ref t => Err(BasicParseError::UnexpectedToken(t.clone()))
        }
    }

    /// expect_ident, but clone the CowRcStr
    #[inline]
    pub fn expect_ident_cloned(&mut self) -> Result<CowRcStr<'i>, BasicParseError<'i>> {
        self.expect_ident().map(|s| s.clone())
    }

    /// Parse a <ident-token> whose unescaped value is an ASCII-insensitive match for the given value.
    #[inline]
    pub fn expect_ident_matching(&mut self, expected_value: &str) -> Result<(), BasicParseError<'i>> {
        match *self.next()? {
            Token::Ident(ref value) if value.eq_ignore_ascii_case(expected_value) => Ok(()),
            ref t => Err(BasicParseError::UnexpectedToken(t.clone()))
        }
    }

    /// Parse a <string-token> and return the unescaped value.
    #[inline]
    pub fn expect_string(&mut self) -> Result<&CowRcStr<'i>, BasicParseError<'i>> {
        match *self.next()? {
            Token::QuotedString(ref value) => Ok(value),
            ref t => Err(BasicParseError::UnexpectedToken(t.clone()))
        }
    }

    /// expect_string, but clone the CowRcStr
    #[inline]
    pub fn expect_string_cloned(&mut self) -> Result<CowRcStr<'i>, BasicParseError<'i>> {
        self.expect_string().map(|s| s.clone())
    }

    /// Parse either a <ident-token> or a <string-token>, and return the unescaped value.
    #[inline]
    pub fn expect_ident_or_string(&mut self) -> Result<&CowRcStr<'i>, BasicParseError<'i>> {
        match *self.next()? {
            Token::Ident(ref value) => Ok(value),
            Token::QuotedString(ref value) => Ok(value),
            ref t => Err(BasicParseError::UnexpectedToken(t.clone()))
        }
    }

    /// Parse a <url-token> and return the unescaped value.
    #[inline]
    pub fn expect_url(&mut self) -> Result<CowRcStr<'i>, BasicParseError<'i>> {
        // FIXME: revert early returns when lifetimes are non-lexical
        match *self.next()? {
            Token::UnquotedUrl(ref value) => return Ok(value.clone()),
            Token::Function(ref name) if name.eq_ignore_ascii_case("url") => {}
            ref t => return Err(BasicParseError::UnexpectedToken(t.clone()))
        }
        self.parse_nested_block(|input| input.expect_string().map_err(ParseError::Basic).map(|s| s.clone()))
            .map_err(ParseError::<()>::basic)
    }

    /// Parse either a <url-token> or a <string-token>, and return the unescaped value.
    #[inline]
    pub fn expect_url_or_string(&mut self) -> Result<CowRcStr<'i>, BasicParseError<'i>> {
        // FIXME: revert early returns when lifetimes are non-lexical
        match *self.next()? {
            Token::UnquotedUrl(ref value) => return Ok(value.clone()),
            Token::QuotedString(ref value) => return Ok(value.clone()),
            Token::Function(ref name) if name.eq_ignore_ascii_case("url") => {}
            ref t => return Err(BasicParseError::UnexpectedToken(t.clone()))
        }
        self.parse_nested_block(|input| input.expect_string().map_err(ParseError::Basic).map(|s| s.clone()))
            .map_err(ParseError::<()>::basic)
    }

    /// Parse a <number-token> and return the integer value.
    #[inline]
    pub fn expect_number(&mut self) -> Result<f32, BasicParseError<'i>> {
        match *self.next()? {
            Token::Number { value, .. } => Ok(value),
            ref t => Err(BasicParseError::UnexpectedToken(t.clone()))
        }
    }

    /// Parse a <number-token> that does not have a fractional part, and return the integer value.
    #[inline]
    pub fn expect_integer(&mut self) -> Result<i32, BasicParseError<'i>> {
        match *self.next()? {
            Token::Number { int_value: Some(int_value), .. } => {
                Ok(int_value)
            }
            ref t => Err(BasicParseError::UnexpectedToken(t.clone()))
        }
    }

    /// Parse a <percentage-token> and return the value.
    /// `0%` and `100%` map to `0.0` and `1.0` (not `100.0`), respectively.
    #[inline]
    pub fn expect_percentage(&mut self) -> Result<f32, BasicParseError<'i>> {
        match *self.next()? {
            Token::Percentage { unit_value, .. } => Ok(unit_value),
            ref t => Err(BasicParseError::UnexpectedToken(t.clone()))
        }
    }

    /// Parse a `:` <colon-token>.
    #[inline]
    pub fn expect_colon(&mut self) -> Result<(), BasicParseError<'i>> {
        match *self.next()? {
            Token::Colon => Ok(()),
            ref t => Err(BasicParseError::UnexpectedToken(t.clone()))
        }
    }

    /// Parse a `;` <semicolon-token>.
    #[inline]
    pub fn expect_semicolon(&mut self) -> Result<(), BasicParseError<'i>> {
        match *self.next()? {
            Token::Semicolon => Ok(()),
            ref t => Err(BasicParseError::UnexpectedToken(t.clone()))
        }
    }

    /// Parse a `,` <comma-token>.
    #[inline]
    pub fn expect_comma(&mut self) -> Result<(), BasicParseError<'i>> {
        match *self.next()? {
            Token::Comma => Ok(()),
            ref t => Err(BasicParseError::UnexpectedToken(t.clone()))
        }
    }

    /// Parse a <delim-token> with the given value.
    #[inline]
    pub fn expect_delim(&mut self, expected_value: char) -> Result<(), BasicParseError<'i>> {
        match *self.next()? {
            Token::Delim(value) if value == expected_value => Ok(()),
            ref t => Err(BasicParseError::UnexpectedToken(t.clone()))
        }
    }

    /// Parse a `{ /* ... */ }` curly brackets block.
    ///
    /// If the result is `Ok`, you can then call the `Parser::parse_nested_block` method.
    #[inline]
    pub fn expect_curly_bracket_block(&mut self) -> Result<(), BasicParseError<'i>> {
        match *self.next()? {
            Token::CurlyBracketBlock => Ok(()),
            ref t => Err(BasicParseError::UnexpectedToken(t.clone()))
        }
    }

    /// Parse a `[ /* ... */ ]` square brackets block.
    ///
    /// If the result is `Ok`, you can then call the `Parser::parse_nested_block` method.
    #[inline]
    pub fn expect_square_bracket_block(&mut self) -> Result<(), BasicParseError<'i>> {
        match *self.next()? {
            Token::SquareBracketBlock => Ok(()),
            ref t => Err(BasicParseError::UnexpectedToken(t.clone()))
        }
    }

    /// Parse a `( /* ... */ )` parenthesis block.
    ///
    /// If the result is `Ok`, you can then call the `Parser::parse_nested_block` method.
    #[inline]
    pub fn expect_parenthesis_block(&mut self) -> Result<(), BasicParseError<'i>> {
        match *self.next()? {
            Token::ParenthesisBlock => Ok(()),
            ref t => Err(BasicParseError::UnexpectedToken(t.clone()))
        }
    }

    /// Parse a <function> token and return its name.
    ///
    /// If the result is `Ok`, you can then call the `Parser::parse_nested_block` method.
    #[inline]
    pub fn expect_function(&mut self) -> Result<&CowRcStr<'i>, BasicParseError<'i>> {
        match *self.next()? {
            Token::Function(ref name) => Ok(name),
            ref t => Err(BasicParseError::UnexpectedToken(t.clone()))
        }
    }

    /// Parse a <function> token whose name is an ASCII-insensitive match for the given value.
    ///
    /// If the result is `Ok`, you can then call the `Parser::parse_nested_block` method.
    #[inline]
    pub fn expect_function_matching(&mut self, expected_name: &str) -> Result<(), BasicParseError<'i>> {
        match *self.next()? {
            Token::Function(ref name) if name.eq_ignore_ascii_case(expected_name) => Ok(()),
            ref t => Err(BasicParseError::UnexpectedToken(t.clone()))
        }
    }

    /// Parse the input until exhaustion and check that it contains no ???error??? token.
    ///
    /// See `Token::is_parse_error`. This also checks nested blocks and functions recursively.
    #[inline]
    pub fn expect_no_error_token(&mut self) -> Result<(), BasicParseError<'i>> {
        loop {
            match self.next_including_whitespace_and_comments() {
                Ok(&Token::Function(_)) |
                Ok(&Token::ParenthesisBlock) |
                Ok(&Token::SquareBracketBlock) |
                Ok(&Token::CurlyBracketBlock) => {}
                Ok(token) => {
                    if token.is_parse_error() {
                        //FIXME: maybe these should be separate variants of BasicParseError instead?
                        return Err(BasicParseError::UnexpectedToken(token.clone()))
                    }
                    continue
                }
                Err(_) => return Ok(())
            }
            let result = self.parse_nested_block(|input| input.expect_no_error_token()
                                                 .map_err(|e| ParseError::Basic(e)));
            result.map_err(ParseError::<()>::basic)?
        }
    }
}

pub fn parse_until_before<'i: 't, 't, F, T, E>(parser: &mut Parser<'i, 't>,
                                               delimiters: Delimiters,
                                               parse: F)
                                               -> Result <T, ParseError<'i, E>>
    where F: for<'tt> FnOnce(&mut Parser<'i, 'tt>) -> Result<T, ParseError<'i, E>> {
    let delimiters = parser.stop_before | delimiters;
    let result;
    // Introduce a new scope to limit duration of nested_parser???s borrow
    {
        let mut delimited_parser = Parser {
            input: parser.input,
            at_start_of: parser.at_start_of.take(),
            stop_before: delimiters,
        };
        result = delimited_parser.parse_entirely(parse);
        if let Some(block_type) = delimited_parser.at_start_of {
            consume_until_end_of_block(block_type, &mut delimited_parser.input.tokenizer);
        }
    }
    // FIXME: have a special-purpose tokenizer method for this that does less work.
    loop {
        if delimiters.contains(Delimiters::from_byte((parser.input.tokenizer).next_byte())) {
            break
        }
        if let Ok(token) = (parser.input.tokenizer).next() {
            if let Some(block_type) = BlockType::opening(&token) {
                consume_until_end_of_block(block_type, &mut parser.input.tokenizer);
            }
        } else {
            break
        }
    }
    result
}

pub fn parse_until_after<'i: 't, 't, F, T, E>(parser: &mut Parser<'i, 't>,
                                              delimiters: Delimiters,
                                              parse: F)
                                              -> Result <T, ParseError<'i, E>>
    where F: for<'tt> FnOnce(&mut Parser<'i, 'tt>) -> Result<T, ParseError<'i, E>> {
    let result = parser.parse_until_before(delimiters, parse);
    let next_byte = (parser.input.tokenizer).next_byte();
    if next_byte.is_some() && !parser.stop_before.contains(Delimiters::from_byte(next_byte)) {
        debug_assert!(delimiters.contains(Delimiters::from_byte(next_byte)));
        (parser.input.tokenizer).advance(1);
        if next_byte == Some(b'{') {
            consume_until_end_of_block(BlockType::CurlyBracket, &mut parser.input.tokenizer);
        }
    }
    result
}

pub fn parse_nested_block<'i: 't, 't, F, T, E>(parser: &mut Parser<'i, 't>, parse: F)
                                               -> Result <T, ParseError<'i, E>>
    where F: for<'tt> FnOnce(&mut Parser<'i, 'tt>) -> Result<T, ParseError<'i, E>> {
    let block_type = parser.at_start_of.take().expect("\
        A nested parser can only be created when a Function, \
        ParenthesisBlock, SquareBracketBlock, or CurlyBracketBlock \
        token was just consumed.\
        ");
    let closing_delimiter = match block_type {
        BlockType::CurlyBracket => ClosingDelimiter::CloseCurlyBracket,
        BlockType::SquareBracket => ClosingDelimiter::CloseSquareBracket,
        BlockType::Parenthesis => ClosingDelimiter::CloseParenthesis,
    };
    let result;
    // Introduce a new scope to limit duration of nested_parser???s borrow
    {
        let mut nested_parser = Parser {
            input: parser.input,
            at_start_of: None,
            stop_before: closing_delimiter,
        };
        result = nested_parser.parse_entirely(parse);
        if let Some(block_type) = nested_parser.at_start_of {
            consume_until_end_of_block(block_type, &mut nested_parser.input.tokenizer);
        }
    }
    consume_until_end_of_block(block_type, &mut parser.input.tokenizer);
    result
}

fn consume_until_end_of_block(block_type: BlockType, tokenizer: &mut Tokenizer) {
    let mut stack = vec![block_type];

    // FIXME: have a special-purpose tokenizer method for this that does less work.
    while let Ok(ref token) = tokenizer.next() {
        if let Some(b) = BlockType::closing(token) {
            if *stack.last().unwrap() == b {
                stack.pop();
                if stack.is_empty() {
                    return;
                }
            }
        }

        if let Some(block_type) = BlockType::opening(token) {
            stack.push(block_type);
        }
    }
}
