//! A small `.env` parser for `hush import`. It follows dotenvy's quoting and
//! escape rules, but it never expands `$VAR` or `${VAR}`. Values come in
//! exactly as written, and nothing is read from the environment.

use std::iter::Peekable;
use std::str::Chars;

use anyhow::{Result, bail};

/// Parses `.env` text into (key, value) pairs in file order. A key that shows
/// up twice is returned twice, so the last one wins when inserted.
pub fn parse(text: &str) -> Result<Vec<(String, String)>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut parser = Parser { chars: text.chars().peekable(), line: 1 };
    let mut pairs = Vec::new();
    while let Some(pair) = parser.entry()? {
        pairs.push(pair);
    }
    Ok(pairs)
}

struct Parser<'a> {
    chars: Peekable<Chars<'a>>,
    line: usize,
}

impl Parser<'_> {
    fn next(&mut self) -> Option<char> {
        let c = self.chars.next();
        if c == Some('\n') {
            self.line += 1;
        }
        c
    }

    fn peek(&mut self) -> Option<char> {
        self.chars.peek().copied()
    }

    fn skip_blanks(&mut self) {
        while matches!(self.peek(), Some(' ' | '\t')) {
            self.next();
        }
    }

    fn skip_line(&mut self) {
        while self.next().is_some_and(|c| c != '\n') {}
    }

    /// One `KEY=value` entry, skipping blank lines and comments before it.
    fn entry(&mut self) -> Result<Option<(String, String)>> {
        loop {
            match self.peek() {
                None => return Ok(None),
                Some('#') => self.skip_line(),
                Some(c) if c.is_whitespace() => {
                    self.next();
                }
                Some(_) => break,
            }
        }
        let line = self.line;
        let mut key = self.key(line)?;
        self.skip_blanks();
        // `export KEY=value` is allowed, and so is a variable named `export`.
        if key == "export" && self.peek() != Some('=') {
            key = self.key(line)?;
            self.skip_blanks();
        }
        if self.next() != Some('=') {
            bail!("line {line}: expected `=` after {key}");
        }
        self.skip_blanks();
        let value = self.value(line)?;
        Ok(Some((key, value)))
    }

    fn key(&mut self, line: usize) -> Result<String> {
        let mut key = String::new();
        if self.peek().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') {
            while let Some(c) = self.peek().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.')) {
                key.push(c);
                self.next();
            }
        }
        if key.is_empty() {
            bail!("line {line}: expected a variable name");
        }
        Ok(key)
    }

    /// Unquoted, single-quoted, and double-quoted parts can sit side by side
    /// (`a'b c'd` is `ab cd`). Quoted parts may span lines.
    fn value(&mut self, line: usize) -> Result<String> {
        let mut value = String::new();
        if self.peek() == Some('#') {
            self.skip_line();
            return Ok(value);
        }
        loop {
            match self.next() {
                None | Some('\n') => break,
                Some('\r') if matches!(self.peek(), None | Some('\n')) => {}
                Some(' ' | '\t') => {
                    self.end_of_value(line)?;
                    break;
                }
                Some('\'') => loop {
                    match self.next() {
                        Some('\'') => break,
                        Some(c) => value.push(c),
                        None => bail!("line {line}: missing closing '"),
                    }
                },
                Some('"') => loop {
                    match self.next() {
                        Some('"') => break,
                        Some('\\') => value.push(self.escape(line)?),
                        Some(c) => value.push(c),
                        None => bail!("line {line}: missing closing \""),
                    }
                },
                Some('\\') => value.push(self.escape(line)?),
                Some(c) => value.push(c),
            }
        }
        Ok(value)
    }

    fn escape(&mut self, line: usize) -> Result<char> {
        match self.next() {
            Some(c @ ('\\' | '\'' | '"' | '$' | ' ')) => Ok(c),
            Some('n') => Ok('\n'),
            _ => bail!("line {line}: unsupported escape after `\\`"),
        }
    }

    /// After whitespace in an unquoted value, only a comment may follow.
    fn end_of_value(&mut self, line: usize) -> Result<()> {
        self.skip_blanks();
        match self.next() {
            None | Some('\n') => Ok(()),
            Some('#') => {
                self.skip_line();
                Ok(())
            }
            Some('\r') if matches!(self.peek(), None | Some('\n')) => {
                self.next();
                Ok(())
            }
            _ => bail!("line {line}: values with spaces need quotes"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(text: &str) -> Vec<(String, String)> {
        parse(text).unwrap()
    }

    fn one(text: &str) -> String {
        let mut parsed = pairs(text);
        assert_eq!(parsed.len(), 1, "{text:?}");
        parsed.remove(0).1
    }

    #[test]
    fn dollar_signs_stay_literal() {
        assert_eq!(one("PW=abc$HOME"), "abc$HOME");
        assert_eq!(one("PW=pa$word"), "pa$word");
        assert_eq!(one("PW=x$!y"), "x$!y");
        assert_eq!(one("URL=\"${BASE}/api\""), "${BASE}/api");
        assert_eq!(one("PW='$HOME'"), "$HOME");
        assert_eq!(one("PW=\"\\$HOME\""), "$HOME");
    }

    #[test]
    fn earlier_keys_are_not_substituted() {
        let parsed = pairs("BASE=http://x\nURL=$BASE/api\n");
        assert_eq!(parsed[1], ("URL".into(), "$BASE/api".into()));
    }

    #[test]
    fn comments_blank_lines_and_export() {
        let parsed = pairs("\u{feff}# top\n\n  export A=1\nB=2 # note\nC=a#b\nD=   # empty\nE=\nexport=x\n");
        let expected = [("A", "1"), ("B", "2"), ("C", "a#b"), ("D", ""), ("E", ""), ("export", "x")];
        assert_eq!(parsed.len(), expected.len());
        for ((key, value), (k, v)) in parsed.iter().zip(expected) {
            assert_eq!((key.as_str(), value.as_str()), (k, v));
        }
    }

    #[test]
    fn quotes_and_escapes() {
        assert_eq!(one("A='single \"x\" \\n'"), "single \"x\" \\n");
        assert_eq!(one("A=\"double 'x' \\\"y\\\" \\\\ \\n\""), "double 'x' \"y\" \\ \n");
        assert_eq!(one("A=a'b c'd"), "ab cd");
        assert_eq!(one("A=s\\ ix"), "s ix");
        assert_eq!(one("A=  \"spaced\"  "), "spaced");
    }

    #[test]
    fn quoted_values_can_span_lines() {
        let parsed = pairs("KEY=\"-----BEGIN-----\nabc\n-----END-----\"\nNEXT=1\n");
        assert_eq!(parsed[0].1, "-----BEGIN-----\nabc\n-----END-----");
        assert_eq!(parsed[1], ("NEXT".into(), "1".into()));
    }

    #[test]
    fn crlf_line_endings() {
        let parsed = pairs("A=1\r\nB=\"two\"\r\nC=3 # c\r\n");
        let values: Vec<_> = parsed.iter().map(|(_, v)| v.as_str()).collect();
        assert_eq!(values, ["1", "two", "3"]);
    }

    #[test]
    fn bad_lines_name_the_line() {
        for (text, msg) in [
            ("A=1\nB=two words\n", "line 2: values with spaces need quotes"),
            ("A=1\n\nB=\"open\n", "line 3: missing closing \""),
            ("A='open", "line 1: missing closing '"),
            ("A=1\nMY-KEY=1\n", "line 2: expected `=` after MY"),
            ("=1\n", "line 1: expected a variable name"),
            ("A=\"\\q\"", "line 1: unsupported escape"),
        ] {
            let err = parse(text).unwrap_err().to_string();
            assert!(err.contains(msg), "{text:?} gave {err:?}");
        }
    }
}
