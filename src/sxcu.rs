// SPDX-License-Identifier: GPL-3.0-or-later
//
// Ported from ShareX.UploadersLib/CustomUploader/CustomUploaderItem.cs.
// Copyright (c) 2007-2026 ShareX Team
// Copyright (c) 2026 David

use std::collections::BTreeMap;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::time::Duration;

use reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue};
use reqwest::multipart::{Form, Part};
use reqwest::{Client, Method, Url};
use serde::Deserialize;

use crate::Result;
use crate::syntax::{Parser, Response};

const PNG_MIME: &str = "image/png";

#[derive(Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct Uploader {
    version: String,
    request_method: Option<String>,
    #[serde(rename = "RequestURL")]
    request_url: String,
    parameters: BTreeMap<String, String>,
    headers: BTreeMap<String, String>,
    body: Body,
    arguments: BTreeMap<String, String>,
    file_form_name: String,
    #[serde(rename = "URL")]
    url: String,
    #[serde(rename = "DeletionURL")]
    deletion_url: String,
    error_message: String,
}

#[derive(Default, Deserialize, Debug, PartialEq)]
enum Body {
    #[default]
    None,
    MultipartFormData,
    FormURLEncoded,
    #[serde(rename = "JSON")]
    Json,
    #[serde(rename = "XML")]
    Xml,
    Binary,
}

#[derive(Debug)]
pub struct Uploaded {
    pub url: String,
    pub deletion_url: Option<String>,
}

pub fn load(name: &str) -> Result<Uploader> {
    let path = resolve(name)?;
    let text = std::fs::read_to_string(&path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    parse(&text).map_err(|error| format!("{}: {error}", path.display()).into())
}

fn resolve(name: &str) -> Result<PathBuf> {
    if name.contains('/') || name.ends_with(".sxcu") {
        return Ok(Path::new(name).to_path_buf());
    }
    let config = crate::config::base_dir().ok_or("cannot find the config directory")?;
    Ok(config
        .join("accioshot/uploaders")
        .join(format!("{name}.sxcu")))
}

fn parse(text: &str) -> Result<Uploader> {
    let mut uploader: Uploader = serde_json::from_str(text.trim_start_matches('\u{feff}'))?;
    if uploader.request_url.is_empty() {
        return Err("RequestURL must be configured".into());
    }
    match uploader.body {
        Body::MultipartFormData if uploader.file_form_name.is_empty() => {
            return Err("FileFormName must be configured".into());
        }
        Body::MultipartFormData | Body::Binary => {}
        ref body => return Err(format!("Body {body:?} cannot upload a file").into()),
    }

    let version = version_parts(&uploader.version);
    if version.is_empty() || version.as_slice() <= [12, 3, 1].as_slice() {
        return Err(format!(
            "unsupported old format, Version is \"{}\"",
            uploader.version
        )
        .into());
    }
    if version.as_slice() <= [13, 7, 1].as_slice() {
        uploader.migrate();
    }
    Ok(uploader)
}

fn version_parts(version: &str) -> Vec<u32> {
    version
        .split('.')
        .map_while(|part| part.parse().ok())
        .collect()
}

impl Uploader {
    fn migrate(&mut self) {
        for field in [
            &mut self.request_url,
            &mut self.url,
            &mut self.deletion_url,
            &mut self.error_message,
        ] {
            *field = migrate_old_syntax(field);
        }
        for map in [&mut self.parameters, &mut self.headers, &mut self.arguments] {
            for value in map.values_mut() {
                *value = migrate_old_syntax(value);
            }
        }
    }

    fn request_url(&self, file_name: &str) -> Result<Url> {
        let url = Parser {
            file_name,
            url_encode: true,
            ..Default::default()
        }
        .parse(&self.request_url)?;
        let url = if url.contains("://") {
            url
        } else {
            format!("https://{url}")
        };
        let mut url = Url::parse(&url).map_err(|error| format!("invalid RequestURL: {error}"))?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(format!("unsupported URL scheme: {}", url.scheme()).into());
        }
        let parser = Parser {
            file_name,
            ..Default::default()
        };
        for (key, value) in &self.parameters {
            url.query_pairs_mut()
                .append_pair(key, &parser.parse(value)?);
        }
        Ok(url)
    }

    fn header_map(&self, file_name: &str) -> Result<HeaderMap> {
        let parser = Parser {
            file_name,
            ..Default::default()
        };
        let mut map = HeaderMap::new();
        for (key, value) in &self.headers {
            let name = HeaderName::try_from(key.as_str())
                .map_err(|_| format!("invalid header name: {key}"))?;
            // No value in the message, headers usually hold tokens.
            let value = HeaderValue::try_from(parser.parse(value)?)
                .map_err(|_| format!("invalid value for header {key}"))?;
            map.insert(name, value);
        }
        Ok(map)
    }

    fn method(&self) -> Result<Method> {
        let method = self.request_method.as_deref().unwrap_or("POST");
        Method::from_bytes(method.to_ascii_uppercase().as_bytes())
            .map_err(|_| format!("invalid RequestMethod: {method}").into())
    }

    fn form(&self, png: Vec<u8>, file_name: &str) -> Result<Form> {
        let parser = Parser {
            file_name,
            ..Default::default()
        };
        let mut form = Form::new();
        for (key, value) in &self.arguments {
            form = form.text(key.clone(), parser.parse(value)?);
        }
        let part = Part::bytes(png)
            .file_name(file_name.to_string())
            .mime_str(PNG_MIME)?;
        Ok(form.part(self.file_form_name.clone(), part))
    }

    fn read_response(&self, response: &Response, status: u16, file_name: &str) -> Result<Uploaded> {
        let parser = Parser {
            file_name,
            response: Some(response),
            url_encode: true,
        };
        if !(200..300).contains(&status) {
            let detail = parser.parse(&self.error_message).unwrap_or_default();
            return Err(format!("upload failed: HTTP {status} {detail}")
                .trim_end()
                .to_string()
                .into());
        }
        let url = match self.url.is_empty() {
            true => response.text.trim().to_string(),
            false => parser.parse(&self.url)?,
        };
        if url.is_empty() {
            return Err("upload succeeded but the response has no URL".into());
        }
        // The file is already up, a bad deletion template must not hide its URL.
        let deletion_url = parser
            .parse(&self.deletion_url)
            .ok()
            .filter(|url| !url.is_empty());
        Ok(Uploaded { url, deletion_url })
    }
}

pub async fn upload(uploader: &Uploader, png: Vec<u8>, file_name: &str) -> Result<Uploaded> {
    let client = Client::builder()
        .timeout(Duration::from_secs(120))
        .build()?;
    let mut request = client.request(uploader.method()?, uploader.request_url(file_name)?);
    request = match uploader.body {
        Body::MultipartFormData => request.multipart(uploader.form(png, file_name)?),
        _ => request.header(CONTENT_TYPE, PNG_MIME).body(png),
    };
    // Replaces the default Content-Type if the uploader sets its own.
    request = request.headers(uploader.header_map(file_name)?);

    let response = request.send().await.map_err(describe)?;
    let status = response.status().as_u16();
    let url = response.url().to_string();
    let headers = response.headers().clone();
    let text = response.text().await.map_err(describe)?;
    uploader.read_response(&Response { text, url, headers }, status, file_name)
}

/// reqwest errors include the URL, and request URLs often hold API keys.
fn describe(error: reqwest::Error) -> Box<dyn Error> {
    let error = error.without_url();
    let mut message = format!("upload failed: {error}");
    let mut source = error.source();
    while let Some(cause) = source {
        message.push_str(&format!(": {cause}"));
        source = cause.source();
    }
    message.into()
}

fn migrate_old_syntax(input: &str) -> String {
    let mut output = String::new();
    let mut start = true;
    let mut chars = input.chars();
    while let Some(c) = chars.next() {
        match c {
            '$' => {
                output.push(if start { '{' } else { '}' });
                start = !start;
            }
            '\\' => {
                chars.next();
            }
            '{' | '}' => {
                output.push('\\');
                output.push(c);
            }
            _ => output.push(c),
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sxcu(fields: &str) -> Result<Uploader> {
        parse(&format!(
            r#"{{"Version":"14.0.0","RequestURL":"https://x.test/up",{fields}}}"#
        ))
    }

    fn response(text: &str) -> Response {
        Response {
            text: text.to_string(),
            url: "https://x.test/up".to_string(),
            headers: HeaderMap::new(),
        }
    }

    #[test]
    fn parses_a_real_shaped_file_with_bom() {
        let text = "\u{feff}{\"Version\":\"16.0.0\",\"Name\":\"x\",\"DestinationType\":\"ImageUploader\",\
            \"RequestMethod\":\"PUT\",\"RequestURL\":\"x.test/up\",\"Parameters\":{\"key\":\"a b\"},\
            \"Headers\":{\"Authorization\":\"Basic {base64:u:p}\"},\"Body\":\"MultipartFormData\",\
            \"FileFormName\":\"file\",\"URL\":\"{json:url}\"}";
        let uploader = parse(text).unwrap();
        assert_eq!(uploader.method().unwrap(), Method::PUT);
        let url = uploader.request_url("a.png").unwrap();
        assert_eq!(url.as_str(), "https://x.test/up?key=a+b");
        assert_eq!(
            uploader.header_map("a.png").unwrap()["authorization"],
            "Basic dTpw"
        );
    }

    #[test]
    fn rejects_what_cannot_upload_a_file() {
        assert!(sxcu(r#""Body":"JSON""#).is_err());
        assert!(sxcu(r#""Body":"None""#).is_err());
        assert!(sxcu(r#""Body":"MultipartFormData""#).is_err());
        assert!(parse(r#"{"Version":"14.0.0","Body":"Binary"}"#).is_err());
    }

    #[test]
    fn checks_version_like_sharex() {
        let file = |version: &str| {
            format!(
                r#"{{"Version":"{version}","RequestURL":"https://x.test/$a$","Body":"Binary"}}"#
            )
        };
        assert!(parse(r#"{"RequestURL":"https://x.test","Body":"Binary"}"#).is_err());
        assert!(parse(&file("12.3.1")).is_err());
        assert_eq!(
            parse(&file("13.7.1")).unwrap().request_url,
            "https://x.test/{a}"
        );
        assert_eq!(
            parse(&file("13.7.2")).unwrap().request_url,
            "https://x.test/$a$"
        );
    }

    #[test]
    fn migrates_old_syntax() {
        assert_eq!(migrate_old_syntax("$json:url$"), "{json:url}");
        assert_eq!(migrate_old_syntax("a{b}"), r"a\{b\}");
        assert_eq!(migrate_old_syntax(r"\$x"), "x");
    }

    #[test]
    fn only_http_schemes_are_allowed() {
        let uploader =
            parse(r#"{"Version":"14.0.0","RequestURL":"file:///etc/passwd","Body":"Binary"}"#)
                .unwrap();
        assert!(uploader.request_url("a.png").is_err());
    }

    #[test]
    fn empty_url_template_uses_the_response_text() {
        let uploader = sxcu(r#""Body":"Binary""#).unwrap();
        let uploaded = uploader
            .read_response(&response("https://x.test/1\n"), 200, "a.png")
            .unwrap();
        assert_eq!(uploaded.url, "https://x.test/1");
    }

    #[test]
    fn reads_url_and_deletion_url() {
        let uploader =
            sxcu(r#""Body":"Binary","URL":"{json:url}","DeletionURL":"{json:del}""#).unwrap();
        let body = r#"{"url":"https://x.test/1","del":"https://x.test/d"}"#;
        let uploaded = uploader
            .read_response(&response(body), 201, "a.png")
            .unwrap();
        assert_eq!(uploaded.url, "https://x.test/1");
        assert_eq!(uploaded.deletion_url.as_deref(), Some("https://x.test/d"));
    }

    #[test]
    fn failures_report_status_and_error_message() {
        let uploader = sxcu(r#""Body":"Binary","ErrorMessage":"{json:error}""#).unwrap();
        let error = uploader
            .read_response(&response(r#"{"error":"too big"}"#), 413, "a.png")
            .unwrap_err();
        assert_eq!(error.to_string(), "upload failed: HTTP 413 too big");
    }

    #[test]
    fn missing_url_is_an_error() {
        let uploader = sxcu(r#""Body":"Binary","URL":"{json:url}""#).unwrap();
        assert!(
            uploader
                .read_response(&response("{}"), 200, "a.png")
                .is_err()
        );
    }
}
