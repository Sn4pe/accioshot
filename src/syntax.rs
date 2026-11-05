// SPDX-License-Identifier: GPL-3.0-or-later
//
// Ported from ShareX.UploadersLib/CustomUploader (ShareXSyntaxParser and Functions).
// Copyright (c) 2007-2026 ShareX Team
// Copyright (c) 2026 David

use std::time::{SystemTime, UNIX_EPOCH};

use fancy_regex::Regex;
use reqwest::header::HeaderMap;
use serde_json::Value;

use crate::Result;

pub struct Response {
    pub text: String,
    pub url: String,
    pub headers: HeaderMap,
}

#[derive(Default)]
pub struct Parser<'a> {
    pub file_name: &'a str,
    pub response: Option<&'a Response>,
    /// Only URL encodes the file name.
    pub url_encode: bool,
}

impl Parser<'_> {
    pub fn parse(&self, text: &str) -> Result<String> {
        let chars: Vec<char> = text.chars().collect();
        Ok(self.parse_from(&chars, false, 0)?.0)
    }

    fn parse_from(
        &self,
        text: &[char],
        is_function: bool,
        start: usize,
    ) -> Result<(String, usize)> {
        let mut output = String::new();
        let mut escape = false;
        let mut i = start;
        while i < text.len() {
            let c = text[i];
            if !escape {
                match c {
                    '{' => {
                        let (parsed, end) = self.parse_from(text, true, i + 1)?;
                        output.push_str(&parsed);
                        i = end + 1;
                        continue;
                    }
                    '}' | '|' => break,
                    '\\' => {
                        escape = true;
                        i += 1;
                        continue;
                    }
                    ':' if is_function => {
                        let mut parameters = Vec::new();
                        loop {
                            let (parsed, end) = self.parse_from(text, false, i + 1)?;
                            parameters.push(parsed);
                            i = end;
                            if text.get(i) != Some(&'|') {
                                break;
                            }
                        }
                        return Ok((self.call(&output, &parameters)?, i));
                    }
                    _ => {}
                }
            }
            escape = false;
            output.push(c);
            i += 1;
        }
        if is_function {
            return Ok((self.call(&output, &[])?, i));
        }
        Ok((output, i))
    }

    fn call(&self, name: &str, parameters: &[String]) -> Result<String> {
        let required = |count: usize| -> Result<()> {
            if parameters.len() < count {
                return Err(format!("{{{name}}} needs at least {count} parameter(s)").into());
            }
            Ok(())
        };
        match name.to_ascii_lowercase().as_str() {
            "" => Err("function name cannot be empty".into()),
            "filename" if self.url_encode => Ok(url_encode(self.file_name)),
            "filename" => Ok(self.file_name.to_string()),
            // Only text uploads have an input, a capture never does.
            "input" => Ok(String::new()),
            "base64" => {
                required(1)?;
                Ok(base64(parameters[0].as_bytes()))
            }
            "random" => {
                required(2)?;
                let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.subsec_nanos();
                Ok(parameters[nanos as usize % parameters.len()].clone())
            }
            "response" => Ok(self.response()?.text.clone()),
            "responseurl" => Ok(self.response()?.url.clone()),
            "header" => {
                required(1)?;
                let value = self.response()?.headers.get(&parameters[0]);
                Ok(value
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("")
                    .to_string())
            }
            "json" => {
                required(1)?;
                match parameters {
                    [input, path, ..] => json(input, path),
                    [path] => json(&self.response()?.text, path),
                    [] => unreachable!(),
                }
            }
            "regex" => {
                required(1)?;
                match parameters {
                    [input, pattern, group, ..] => regex(input, pattern, group),
                    [pattern, group] => regex(&self.response()?.text, pattern, group),
                    [pattern] => regex(&self.response()?.text, pattern, ""),
                    [] => unreachable!(),
                }
            }
            "xml" | "inputbox" | "prompt" | "outputbox" | "select" => {
                Err(format!("{{{name}}} is not supported yet").into())
            }
            _ => Err(format!("unknown function {{{name}}}").into()),
        }
    }

    fn response(&self) -> Result<&Response> {
        self.response
            .ok_or_else(|| "this function needs a response, use it in URL fields".into())
    }
}

fn json(input: &str, path: &str) -> Result<String> {
    if input.is_empty() || path.is_empty() {
        return Ok(String::new());
    }
    let root: Value = serde_json::from_str(input)?;
    Ok(match select(&root, path)? {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(text)) => text.clone(),
        Some(value) => value.to_string(),
    })
}

/// shortcut: supports `a.b[0]['c']`, not JSONPath filters or wildcards. Add them if a real
/// uploader needs them.
fn select<'a>(root: &'a Value, path: &str) -> Result<Option<&'a Value>> {
    let path = path
        .strip_prefix("$.")
        .or(path.strip_prefix('$'))
        .unwrap_or(path);
    let mut current = root;
    let mut rest = path;
    while !rest.is_empty() {
        let next = if let Some(tail) = rest.strip_prefix('[') {
            let (index, tail) = tail.split_once(']').ok_or("unclosed [ in JSON path")?;
            rest = tail;
            let quoted = index.trim_matches(|c| c == '\'' || c == '"');
            if quoted.len() != index.len() {
                current.get(quoted)
            } else {
                let index: usize = index
                    .parse()
                    .map_err(|_| format!("unsupported JSON path: {path}"))?;
                current.get(index)
            }
        } else {
            let end = rest.find(['.', '[']).unwrap_or(rest.len());
            let (key, tail) = rest.split_at(end);
            if key == "*" {
                return Err(format!("unsupported JSON path: {path}").into());
            }
            rest = tail;
            current.get(key)
        };
        let Some(next) = next else {
            return Ok(None);
        };
        current = next;
        rest = rest.strip_prefix('.').unwrap_or(rest);
    }
    Ok(Some(current))
}

fn regex(input: &str, pattern: &str, group: &str) -> Result<String> {
    if input.is_empty() || pattern.is_empty() {
        return Ok(String::new());
    }
    let Some(captures) = Regex::new(pattern)?.captures(input)? else {
        return Ok(String::new());
    };
    let found = match group.parse::<usize>() {
        _ if group.is_empty() => captures.get(0),
        Ok(number) => captures.get(number),
        Err(_) => captures.name(group),
    };
    Ok(found.map_or("", |m| m.as_str()).to_string())
}

pub fn url_encode(text: &str) -> String {
    let mut output = String::new();
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                output.push(byte as char)
            }
            _ => output.push_str(&format!("%{byte:02X}")),
        }
    }
    output
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::new();
    for chunk in bytes.chunks(3) {
        let group =
            chunk.iter().fold(0u32, |acc, &b| (acc << 8) | u32::from(b)) << (8 * (3 - chunk.len()));
        for i in 0..4 {
            if i <= chunk.len() {
                output.push(ALPHABET[(group >> (18 - 6 * i)) as usize & 63] as char);
            } else {
                output.push('=');
            }
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::{HeaderValue, LOCATION};

    fn with_response(text: &str, template: &str) -> Result<String> {
        let mut headers = HeaderMap::new();
        headers.insert(LOCATION, HeaderValue::from_static("https://loc.test/a"));
        let response = Response {
            text: text.to_string(),
            url: "https://final.test/".to_string(),
            headers,
        };
        Parser {
            file_name: "a b.png",
            response: Some(&response),
            url_encode: true,
        }
        .parse(template)
    }

    #[test]
    fn json_paths_from_sharex_docs() {
        let body = r#"{"files":[{"url":"https://x.test/1"}],"id":7}"#;
        assert_eq!(
            with_response(body, "{json:files[0].url}").unwrap(),
            "https://x.test/1"
        );
        assert_eq!(
            with_response(body, "{json:$.files[0].url}").unwrap(),
            "https://x.test/1"
        );
        assert_eq!(
            with_response(body, "{json:{response}|files[0].url}").unwrap(),
            "https://x.test/1"
        );
        assert_eq!(with_response(body, "{json:['id']}").unwrap(), "7");
        assert_eq!(with_response(body, "{json:missing.key}").unwrap(), "");
        assert!(with_response(body, "{json:files[*].url}").is_err());
    }

    #[test]
    fn regex_examples_from_sharex_docs() {
        let body = r#"<a href="https://x.test/1">"#;
        assert_eq!(
            with_response(body, r#"{regex:(?<=href=").+(?=")}"#).unwrap(),
            "https://x.test/1"
        );
        assert_eq!(
            with_response(body, r#"{regex:href="(.+)"|1}"#).unwrap(),
            "https://x.test/1"
        );
        assert_eq!(
            with_response(body, r#"{regex:href="(?<url>.+)"|url}"#).unwrap(),
            "https://x.test/1"
        );
        assert_eq!(
            with_response(body, r#"{regex:{response}|href="(.+)"|1}"#).unwrap(),
            "https://x.test/1"
        );
    }

    #[test]
    fn text_around_functions_and_escapes() {
        let body = r#"{"id":"abc"}"#;
        assert_eq!(
            with_response(body, "https://x.test/{json:id}.png").unwrap(),
            "https://x.test/abc.png"
        );
        assert_eq!(with_response(body, r"a\{b\}\|c").unwrap(), "a{b}|c");
        assert_eq!(with_response(body, "{filename}").unwrap(), "a%20b.png");
    }

    #[test]
    fn response_functions() {
        assert_eq!(with_response("x", "{response}").unwrap(), "x");
        assert_eq!(
            with_response("x", "{responseurl}").unwrap(),
            "https://final.test/"
        );
        assert_eq!(
            with_response("x", "{header:location}").unwrap(),
            "https://loc.test/a"
        );
        assert_eq!(with_response("x", "{header:missing}").unwrap(), "");
    }

    #[test]
    fn base64_matches_known_values() {
        assert_eq!(
            with_response("", "Basic {base64:user:pass}").unwrap(),
            "Basic dXNlcjpwYXNz"
        );
        assert_eq!(base64(b"a"), "YQ==");
        assert_eq!(base64(b"ab"), "YWI=");
        assert_eq!(base64(b"abc"), "YWJj");
    }

    #[test]
    fn random_picks_a_parameter() {
        let picked = with_response("", "{random:a|b}").unwrap();
        assert!(picked == "a" || picked == "b");
    }

    #[test]
    fn unsupported_and_unknown_functions_fail() {
        assert!(with_response("", "{inputbox:title}").is_err());
        assert!(with_response("", "{xml:/a}").is_err());
        assert!(with_response("", "{nope}").is_err());
        assert!(with_response("", "{}").is_err());
    }

    #[test]
    fn response_functions_fail_without_a_response() {
        assert!(Parser::default().parse("{response}").is_err());
    }
}
