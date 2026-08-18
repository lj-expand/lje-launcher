//! Simple tokenizer + stack-based parser for Valve's VDF text format.

use std::collections::HashMap;

/// A parsed VDF value: either a bare string or a block of key/value pairs.
#[derive(Debug, Clone, PartialEq)]
pub enum VdfValue {
    String(String),
    Block(HashMap<String, VdfValue>),
}

impl VdfValue {
    /// Panics if this value is not a string (matches C# `AsString`).
    /// Used by the test suite; the locator matches on `VdfValue::String` directly.
    #[allow(dead_code)]
    pub fn as_string(&self) -> &str {
        match self {
            VdfValue::String(s) => s,
            VdfValue::Block(_) => panic!("not a string"),
        }
    }

    /// Panics if this value is not a block (matches C# `AsBlock`).
    pub fn as_block(&self) -> &HashMap<String, VdfValue> {
        match self {
            VdfValue::Block(b) => b,
            VdfValue::String(_) => panic!("not a block"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VdfTokenType {
    String,
    BraceStart,
    BraceEnd,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VdfToken {
    pub token_type: VdfTokenType,
    pub value: String,
}

/// Tokenizes VDF text into strings, `{` and `}` tokens.
pub struct VdfTokenizer<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> VdfTokenizer<'a> {
    pub fn new(input: &'a str) -> Self {
        VdfTokenizer { input, pos: 0 }
    }

    pub fn tokenize(&mut self) -> Result<Vec<VdfToken>, String> {
        let mut result = Vec::new();
        self.pos = 0;

        while self.pos < self.input.len() {
            self.skip_whitespace();

            if self.pos >= self.input.len() {
                break;
            }

            let c = self.input[self.pos..].chars().next().unwrap();
            match c {
                '{' => {
                    result.push(VdfToken {
                        token_type: VdfTokenType::BraceStart,
                        value: "{".to_string(),
                    });
                    self.pos += 1;
                }
                '}' => {
                    result.push(VdfToken {
                        token_type: VdfTokenType::BraceEnd,
                        value: "}".to_string(),
                    });
                    self.pos += 1;
                }
                '"' => result.push(VdfToken {
                    token_type: VdfTokenType::String,
                    value: self.parse_string()?,
                }),
                _ => return Err(format!("unexpected character '{}' at position {}", c, self.pos)),
            }
        }

        Ok(result)
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.input.len() {
            let c = self.input[self.pos..].chars().next().unwrap();
            if !c.is_whitespace() {
                break;
            }
            self.pos += c.len_utf8();
        }
    }

    fn parse_string(&mut self) -> Result<String, String> {
        self.pos += 1; // skip opening quote
        let start = self.pos;
        while self.pos < self.input.len() {
            let c = self.input[self.pos..].chars().next().unwrap();
            if c == '"' {
                let s = self.input[start..self.pos].to_string();
                self.pos += 1; // skip closing quote
                return Ok(s);
            }
            self.pos += c.len_utf8();
        }
        Err("unterminated string".to_string())
    }
}

/// Parses VDF text into a root block.
pub fn parse(input: &str) -> Result<HashMap<String, VdfValue>, String> {
    let mut tokenizer = VdfTokenizer::new(input);
    let tokens = tokenizer.tokenize()?;
    if tokens.is_empty() || tokens[0].token_type != VdfTokenType::String {
        return Err("expected root string token".to_string());
    }

    let mut stack: Vec<HashMap<String, VdfValue>> = vec![HashMap::new()]; // root
    // Key under which each frame sits in its parent (None = root).
    let mut keys: Vec<Option<String>> = vec![None];

    let mut i = 0;
    while i < tokens.len() {
        let tok = &tokens[i];
        match tok.token_type {
            VdfTokenType::String => {
                if i + 1 >= tokens.len() {
                    return Err("unexpected end of tokens after string".to_string());
                }

                let next = &tokens[i + 1];
                match next.token_type {
                    VdfTokenType::String => {
                        // Key-value pair
                        stack
                            .last_mut()
                            .unwrap()
                            .insert(tok.value.clone(), VdfValue::String(next.value.clone()));
                        i += 1; // consume value token
                    }
                    VdfTokenType::BraceStart => {
                        // New block: push an empty frame; the parent entry is
                        // inserted when the block closes (avoids aliasing).
                        stack.push(HashMap::new());
                        keys.push(Some(tok.value.clone()));
                        i += 1; // consume brace token
                    }
                    VdfTokenType::BraceEnd => {
                        return Err(format!("unexpected token {:?} after string", next.token_type));
                    }
                }
            }
            VdfTokenType::BraceEnd => {
                if stack.len() == 1 {
                    return Err("unexpected closing brace at root level".to_string());
                }
                let child = stack.pop().unwrap();
                let key = keys.pop().unwrap();
                if let Some(k) = key {
                    stack.last_mut().unwrap().insert(k, VdfValue::Block(child));
                }
            }
            VdfTokenType::BraceStart => {
                // The C# parser ignores stray opening braces at the top of the loop;
                // keep that behavior.
            }
        }
        i += 1;
    }

    Ok(stack.pop().unwrap()) // Last remaining element is root
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tree() {
        let result = parse("\"root\" { \"hi\" \"hey\" }").unwrap();
        assert_eq!(result["root"].as_block()["hi"].as_string(), "hey");
    }

    #[test]
    fn parses_nested() {
        let result = parse(
            r#""root" {
"hi" {
"hey" "ho"
}
"bye" "see ya"
}"#,
        )
        .unwrap();
        assert_eq!(result["root"].as_block()["bye"].as_string(), "see ya");
        assert_eq!(result["root"].as_block()["hi"].as_block()["hey"].as_string(), "ho");
    }

    #[test]
    fn parses_library_folders() {
        let vdf = r#""libraryfolders"
{
	"0"
	{
		"path"		"C:\\Program Files (x86)\\Steam"
		"apps"
		{
			"4000"		"1"
			"480"		"1"
		}
	}
	"1"
	{
		"path"		"D:\\SteamLibrary"
		"apps"
		{
			"4000"		"1"
		}
	}
}"#;
        let root = parse(vdf).unwrap();

        let folders = root["libraryfolders"].as_block();
        let first = folders["0"].as_block();
        // C# tokenizer does no escape handling: backslashes stay literal.
        assert_eq!(first["path"].as_string(), r"C:\\Program Files (x86)\\Steam");
        assert_eq!(first["apps"].as_block()["4000"].as_string(), "1");
        assert_eq!(first["apps"].as_block()["480"].as_string(), "1");

        let second = folders["1"].as_block();
        assert_eq!(second["path"].as_string(), r"D:\\SteamLibrary");
        assert_eq!(second["apps"].as_block()["4000"].as_string(), "1");
        assert!(!second["apps"].as_block().contains_key("480"));
    }

    #[test]
    fn tokenize_rejects_garbage() {
        let mut tokenizer = VdfTokenizer::new("\"a\" x");
        let err = tokenizer.tokenize().unwrap_err();
        assert_eq!(err, "unexpected character 'x' at position 4");
    }

    #[test]
    fn parse_rejects_bad_root() {
        let err = parse("{}").unwrap_err();
        assert_eq!(err, "expected root string token");
    }
}
