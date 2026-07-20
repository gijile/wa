use wasm_bindgen::prelude::*;
use serde::Serialize;
use cssparser::{Parser, ParserInput, Token};

#[derive(Serialize)]
pub struct Property {
    pub name: String,
    pub value: String,
}

#[derive(Serialize)]
pub struct Rule {
    pub selector: String,
    pub properties: Vec<Property>,
}

// Helper to serialize cssparser Tokens back to CSS-valid strings
fn append_token_str(token: &Token, out: &mut String) {
    match token {
        Token::Ident(s) => out.push_str(s.as_ref()),
        Token::AtKeyword(s) => { out.push('@'); out.push_str(s.as_ref()); }
        Token::Hash(s) => { out.push('#'); out.push_str(s.as_ref()); }
        Token::IDHash(s) => { out.push('#'); out.push_str(s.as_ref()); }
        Token::QuotedString(s) => { out.push('"'); out.push_str(s.as_ref()); out.push('"'); }
        Token::Number { value, .. } => out.push_str(&value.to_string()),
        Token::Percentage { unit_value, .. } => {
            out.push_str(&(unit_value * 100.0).to_string());
            out.push('%');
        }
        Token::Dimension { value, unit, .. } => {
            out.push_str(&value.to_string());
            out.push_str(unit.as_ref());
        }
        Token::WhiteSpace(s) => out.push_str(s),
        Token::Colon => out.push(':'),
        Token::Semicolon => out.push(';'),
        Token::Comma => out.push(','),
        Token::ParenthesisBlock => out.push('('),
        Token::SquareBracketBlock => out.push('['),
        Token::CurlyBracketBlock => out.push('{'),
        Token::Function(s) => { out.push_str(s.as_ref()); out.push('('); }
        Token::Delim(c) => out.push(*c),
        _ => {}
    }
}

// Helper to parse properties inside curly brackets
fn parse_properties(parser: &mut Parser) -> Vec<Property> {
    let mut props = Vec::new();
    while !parser.is_exhausted() {
        let mut name = String::new();
        while let Ok(token) = parser.next() {
            match token {
                Token::Ident(ref id) => { name = id.to_string(); break; }
                _ => {}
            }
        }
        if name.is_empty() { continue; }

        let mut has_colon = false;
        while let Ok(token) = parser.next() {
            if let Token::Colon = token { has_colon = true; break; }
        }
        if !has_colon { continue; }

        let mut value = String::new();
        while let Ok(token) = parser.next_including_whitespace() {
            match token {
                Token::Semicolon => break,
                t => append_token_str(t, &mut value),
            }
        }
        let trimmed_value = value.trim().to_string();
        if !trimmed_value.is_empty() {
            props.push(Property { name, value: trimmed_value });
        }
    }
    props
}

#[wasm_bindgen]
pub fn parse_css(css: &str) -> Result<JsValue, JsValue> {
    let mut input = ParserInput::new(css);
    let mut parser = Parser::new(&mut input);
    let mut rules = Vec::new();

    while !parser.is_exhausted() {
        let mut selector = String::new();
        let mut properties = Vec::new();
        let mut has_block = false;

        while let Ok(token) = parser.next_including_whitespace() {
            match token {
                Token::CurlyBracketBlock => {
                    let _ = parser.parse_nested_block(|nested| {
                        properties = parse_properties(nested);
                        Ok::<(), cssparser::ParseError<'_, ()>>(())
                    });
                    has_block = true;
                    break;
                }
                Token::CloseCurlyBracket => break,
                t => append_token_str(t, &mut selector),
            }
        }

        let trimmed_selector = selector.trim().to_string();
        if !trimmed_selector.is_empty() && has_block {
            rules.push(Rule { selector: trimmed_selector, properties });
        }
        while let Ok(Token::Semicolon) = parser.next() {}
    }

    serde_wasm_bindgen::to_value(&rules).map_err(|e| JsValue::from_str(&e.to_string()))
}

#[wasm_bindgen]
pub fn match_selector(selector: &str, element: &web_sys::Element) -> Result<bool, JsValue> {
    element.matches(selector)
}

#[wasm_bindgen]
pub struct JsonFormatter {
    input: Vec<char>,
    index: usize,
    indent_level: usize,
    in_string: bool,
    is_escaped: bool,
    in_key: bool,
    after_colon: bool,
}

#[wasm_bindgen]
impl JsonFormatter {
    #[wasm_bindgen(constructor)]
    pub fn new(json: &str) -> JsonFormatter {
        JsonFormatter {
            input: json.chars().collect(),
            index: 0,
            indent_level: 0,
            in_string: false,
            is_escaped: false,
            in_key: false,
            after_colon: false,
        }
    }

    pub fn is_done(&self) -> bool {
        self.index >= self.input.len()
    }

    pub fn process_chunk(&mut self, chunk_size: usize) -> Result<String, String> {
        let mut output = String::new();
        let end = std::cmp::min(self.index + chunk_size, self.input.len());
        
        while self.index < end {
            let c = self.input[self.index];
            self.index += 1;

            if self.is_escaped {
                self.escape_and_append_char(c, &mut output);
                self.is_escaped = false;
                continue;
            }

            if c == '\\\' && self.in_string {
                output.push(c);
                self.is_escaped = true;
                continue;
            }

            if c == '"' {
                self.in_string = !self.in_string;
                if self.in_string {
                    self.in_key = !self.after_colon;
                    let class = if self.in_key { "json-key text-emerald-400 font-semibold" } else { "json-string text-amber-300" };
                    output.push_str(&format!("<span class=\"{}\">\"", class));
                } else {
                    output.push_str("\"</span>");
                    self.in_key = false;
                }
                continue;
            }

            if self.in_string {
                self.escape_and_append_char(c, &mut output);
                continue;
            }

            match c {
                '{' | '[' => {
                    self.indent_level += 1;
                    self.after_colon = false;
                    let bracket_class = if c == '{' { "json-brace text-indigo-400 font-bold" } else { "json-bracket text-sky-400 font-bold" };
                    output.push_str(&format!("<span class=\"{}\">{}</span><div class=\"json-indent pl-4 border-l border-zinc-800 my-0.5\">", bracket_class, c));
                }
                '}' | ']' => {
                    if self.indent_level > 0 { self.indent_level -= 1; }
                    self.after_colon = false;
                    let bracket_class = if c == '}' { "json-brace text-indigo-400 font-bold" } else { "json-bracket text-sky-400 font-bold" };
                    output.push_str(&format!("</div><span class=\"{}\">{}</span>", bracket_class, c));
                }
                ',' => {
                    self.after_colon = false;
                    output.push_str("<span class=\"json-comma text-zinc-500\">,</span><br/>");
                }
                ':' => {
                    self.after_colon = true;
                    output.push_str("<span class=\"json-colon text-zinc-400 mx-1\">: </span>");
                }
                _ if c.is_whitespace() => {}
                _ => {
                    let mut token_str = String::new();
                    token_str.push(c);
                    while self.index < end {
                        let next_c = self.input[self.index];
                        if next_c.is_whitespace() || next_c == ',' || next_c == '}' || next_c == ']' || next_c == ':' {
                            break;
                        }
                        token_str.push(next_c);
                        self.index += 1;
                    }

                    let class = if token_str == "true" || token_str == "false" {
                        "json-boolean text-purple-400 font-medium"
                    } else if token_str == "null" {
                        "json-null text-rose-400 font-semibold"
                    } else {
                        "json-number text-orange-400 font-mono"
                    };
                    output.push_str(&format!("<span class=\"{}\">{}</span>", class, token_str));
                }
            }
        }
        Ok(output)
    }

    fn escape_and_append_char(&self, c: char, out: &mut String) {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
          }
