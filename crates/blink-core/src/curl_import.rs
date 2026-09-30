//! Parse a pasted cURL command into a draft. Supports the options that
//! describe a request: method, URL, headers, data, forms, and user auth.
//! Options that only change curl's own output or transport, such as -s or
//! --proxy, are returned in `ignored`. Port of `src/lib/curl-import.ts`.

use crate::model::{AuthorizationConfig, BodyMode, Draft, Pair};
use crate::request::{create_draft, is_method, pair};

/// Whitespace as JavaScript's `\s` and `trim` see it.
pub(crate) fn is_js_space(char: char) -> bool {
    matches!(
        char,
        '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

/// JavaScript's `String.prototype.trim`.
pub(crate) fn js_trim(text: &str) -> &str {
    text.trim_matches(is_js_space)
}

/// JavaScript's `encodeURIComponent`.
pub(crate) fn encode_uri_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// True when pasted text looks like a cURL command.
pub fn is_curl_command(text: &str) -> bool {
    let rest = text.trim_start_matches(is_js_space);
    rest.strip_prefix("curl")
        .is_some_and(|after| after.chars().next().is_none_or(is_js_space))
}

const UNCLOSED: &str = "The cURL command has an unclosed quote.";

/// Split a command line as a POSIX shell does, without expansion.
pub fn shell_words(command: &str) -> Result<Vec<String>, String> {
    let chars: Vec<char> = command.chars().collect();
    let at = |index: usize| chars.get(index).copied();
    let mut words = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut i = 0;
    let ansi = |char: char| match char {
        'n' => '\n',
        't' => '\t',
        'r' => '\r',
        '0' => '\0',
        other => other,
    };
    while i < chars.len() {
        let char = chars[i];
        if char == '\\' && at(i + 1) == Some('\n') {
            i += 2;
            continue;
        }
        if char == '\\' && at(i + 1) == Some('\r') && at(i + 2) == Some('\n') {
            i += 3;
            continue;
        }
        if is_js_space(char) {
            if in_word {
                words.push(std::mem::take(&mut word));
            }
            word.clear();
            in_word = false;
            i += 1;
            continue;
        }
        in_word = true;
        if char == '\'' {
            let end = chars[i + 1..]
                .iter()
                .position(|c| *c == '\'')
                .ok_or(UNCLOSED)?
                + i
                + 1;
            word.extend(&chars[i + 1..end]);
            i = end + 1;
        } else if char == '$' && at(i + 1) == Some('\'') {
            i += 2;
            while i < chars.len() && chars[i] != '\'' {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    word.push(ansi(chars[i + 1]));
                    i += 2;
                } else {
                    word.push(chars[i]);
                    i += 1;
                }
            }
            if i >= chars.len() {
                return Err(UNCLOSED.into());
            }
            i += 1;
        } else if char == '"' {
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                if chars[i] == '\\' && at(i + 1).is_none_or(|next| "\"\\$`\n".contains(next)) {
                    if let Some(next) = at(i + 1).filter(|next| *next != '\n') {
                        word.push(next);
                    }
                    i += 2;
                } else {
                    word.push(chars[i]);
                    i += 1;
                }
            }
            if i >= chars.len() {
                return Err(UNCLOSED.into());
            }
            i += 1;
        } else if char == '\\' {
            if let Some(next) = at(i + 1) {
                word.push(next);
            }
            i += 2;
        } else {
            word.push(char);
            i += 1;
        }
    }
    if in_word {
        words.push(word);
    }
    Ok(words)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CurlOption {
    Request,
    Header,
    Data,
    DataRaw,
    DataUrlencode,
    Json,
    Form,
    FormString,
    User,
    Get,
    Head,
    Cookie,
    UserAgent,
    Referer,
    Url,
}

fn long_option(word: &str) -> Option<CurlOption> {
    use CurlOption::*;
    Some(match word {
        "--request" => Request,
        "--header" => Header,
        "--data" | "--data-ascii" | "--data-binary" => Data,
        "--data-raw" => DataRaw,
        "--data-urlencode" => DataUrlencode,
        "--json" => Json,
        "--form" => Form,
        "--form-string" => FormString,
        "--user" => User,
        "--get" => Get,
        "--head" => Head,
        "--cookie" => Cookie,
        "--user-agent" => UserAgent,
        "--referer" => Referer,
        "--url" => Url,
        _ => return None,
    })
}

fn short_option(letter: char) -> Option<CurlOption> {
    use CurlOption::*;
    Some(match letter {
        'X' => Request,
        'H' => Header,
        'd' => Data,
        'F' => Form,
        'u' => User,
        'G' => Get,
        'I' => Head,
        'b' => Cookie,
        'A' => UserAgent,
        'e' => Referer,
        _ => return None,
    })
}

fn is_flag(option: CurlOption) -> bool {
    matches!(option, CurlOption::Get | CurlOption::Head)
}

/// Ignored options that take a value, so the value is not read as the URL.
const IGNORED_WITH_VALUE: [&str; 22] = [
    "-o",
    "--output",
    "-m",
    "--max-time",
    "--connect-timeout",
    "--max-redirs",
    "-x",
    "--proxy",
    "--retry",
    "-w",
    "--write-out",
    "-c",
    "--cookie-jar",
    "--cacert",
    "--cert",
    "--key",
    "-E",
    "--resolve",
    "--connect-to",
    "--limit-rate",
    "-r",
    "--range",
];
const IGNORED_SHORT_WITH_VALUE: [char; 7] = ['o', 'm', 'x', 'w', 'c', 'E', 'r'];

fn header(text: &str) -> Option<Pair> {
    match text.find(':') {
        Some(colon) if colon > 0 => {
            Some(pair(js_trim(&text[..colon]), js_trim(&text[colon + 1..])))
        }
        // "Name;" sends an empty header in curl.
        _ => text.strip_suffix(';').map(|name| pair(js_trim(name), "")),
    }
}

/// Browser `atob`: forgiving base64 into bytes.
fn atob(encoded: &str) -> Option<Vec<u8>> {
    let mut data: Vec<u8> = encoded
        .bytes()
        .filter(|b| !matches!(b, b'\t' | b'\n' | b'\x0C' | b'\r' | b' '))
        .collect();
    if data.len().is_multiple_of(4) {
        if data.ends_with(b"==") {
            data.truncate(data.len() - 2);
        } else if data.ends_with(b"=") {
            data.truncate(data.len() - 1);
        }
    }
    if data.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::new();
    let mut buffer = 0u32;
    let mut bits = 0;
    for byte in data {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        buffer = (buffer << 6) | value as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    Some(out)
}

fn decode_basic(encoded: &str) -> Option<(String, String)> {
    let text = String::from_utf8(atob(encoded)?).ok()?;
    // TextDecoder drops a leading byte order mark.
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(&text);
    let colon = text.find(':')?;
    Some((text[..colon].to_string(), text[colon + 1..].to_string()))
}

fn encode_data_urlencode(value: &str) -> String {
    // curl: "name=content" encodes content; "=content" and "content" encode all.
    match value.find('=') {
        Some(0) => encode_uri_component(&value[1..]),
        Some(equals) => format!(
            "{}={}",
            &value[..equals],
            encode_uri_component(&value[equals + 1..])
        ),
        None => encode_uri_component(value),
    }
}

/// `^<scheme>\s+(\S+)$`, case-insensitive: the credential after the scheme.
fn scheme_credential<'a>(value: &'a str, scheme: &str) -> Option<&'a str> {
    let head = value.get(..scheme.len())?;
    if !head.eq_ignore_ascii_case(scheme) {
        return None;
    }
    let rest = &value[scheme.len()..];
    let credential = rest.trim_start_matches(is_js_space);
    (credential.len() < rest.len() && !credential.is_empty() && !credential.contains(is_js_space))
        .then_some(credential)
}

pub(crate) fn contains_ignore_ascii_case(text: &str, needle: &str) -> bool {
    text.to_ascii_lowercase().contains(needle)
}

#[derive(Debug, Clone, PartialEq)]
pub struct CurlImport {
    pub draft: Draft,
    pub ignored: Vec<String>,
}

#[derive(Default)]
struct Parsed {
    method: Option<String>,
    url: String,
    get: bool,
    head: bool,
    json: bool,
    headers: Vec<Pair>,
    data: Vec<String>,
    data_file: Option<String>,
    form: Vec<Pair>,
    user: Option<String>,
    ignored: Vec<String>,
}

impl Parsed {
    fn apply(&mut self, option: CurlOption, value: String) {
        match option {
            CurlOption::Request => self.method = Some(value.to_uppercase()),
            CurlOption::Header => {
                if let Some(row) = header(&value) {
                    self.headers.push(row);
                }
            }
            CurlOption::Data => {
                if let Some(file) = value.strip_prefix('@') {
                    self.data_file = Some(file.to_string());
                } else {
                    self.data.push(value.replace(['\r', '\n'], ""));
                }
            }
            CurlOption::DataRaw => self.data.push(value),
            CurlOption::DataUrlencode => self.data.push(encode_data_urlencode(&value)),
            CurlOption::Json => {
                self.json = true;
                self.data.push(value);
            }
            CurlOption::Form | CurlOption::FormString => {
                let Some(equals) = value.find('=').filter(|equals| *equals > 0) else {
                    return;
                };
                let key = &value[..equals];
                let content = &value[equals + 1..];
                match content.strip_prefix('@') {
                    Some(path) if option == CurlOption::Form => {
                        let path = path.split(';').next().unwrap_or_default();
                        self.form.push(Pair {
                            file: Some(true),
                            ..pair(key, path)
                        });
                    }
                    _ => self.form.push(pair(key, content)),
                }
            }
            CurlOption::User => self.user = Some(value),
            CurlOption::Get => self.get = true,
            CurlOption::Head => self.head = true,
            CurlOption::Cookie => {
                // A value without "=" is a cookie file, which Blink cannot read.
                if value.contains('=') {
                    self.headers.push(pair("Cookie", value));
                } else {
                    self.ignored.push("--cookie".into());
                }
            }
            CurlOption::UserAgent => self.headers.push(pair("User-Agent", value)),
            CurlOption::Referer => self.headers.push(pair("Referer", value)),
            CurlOption::Url => self.url = value,
        }
    }
}

pub fn parse_curl(command: &str) -> Result<CurlImport, String> {
    let words = shell_words(js_trim(command))?;
    if words.first().map(String::as_str) != Some("curl") {
        return Err("Paste a command that starts with curl.".into());
    }
    let mut parsed = Parsed::default();
    let mut i = 1;
    while i < words.len() {
        let word = &words[i];
        let take_value = |i: &mut usize| -> Result<String, String> {
            if *i + 1 >= words.len() {
                return Err(format!("{word} needs a value."));
            }
            *i += 1;
            Ok(words[*i].clone())
        };
        if word.starts_with("--") {
            if let Some(option) = long_option(word) {
                let value = if is_flag(option) {
                    String::new()
                } else {
                    take_value(&mut i)?
                };
                parsed.apply(option, value);
            } else {
                parsed.ignored.push(word.clone());
                if IGNORED_WITH_VALUE.contains(&word.as_str()) {
                    i += 1;
                }
            }
        } else if word.starts_with('-') && word.chars().count() > 1 {
            // Short options can be grouped (-sSL) or carry a value (-XPOST).
            for (at, letter) in word.char_indices().skip(1) {
                let rest = &word[at + letter.len_utf8()..];
                match short_option(letter) {
                    Some(option) if is_flag(option) => parsed.apply(option, String::new()),
                    Some(option) => {
                        let value = if rest.is_empty() {
                            take_value(&mut i)?
                        } else {
                            rest.to_string()
                        };
                        parsed.apply(option, value);
                        break;
                    }
                    None => {
                        parsed.ignored.push(format!("-{letter}"));
                        if IGNORED_SHORT_WITH_VALUE.contains(&letter) {
                            if rest.is_empty() {
                                i += 1;
                            }
                            break;
                        }
                    }
                }
            }
        } else if parsed.url.is_empty() {
            parsed.url = word.clone();
        } else {
            parsed.ignored.push(word.clone());
        }
        i += 1;
    }
    if parsed.url.is_empty() {
        return Err("The cURL command has no URL.".into());
    }

    let Parsed {
        method,
        mut url,
        get,
        head,
        json,
        mut headers,
        data,
        data_file,
        form,
        user,
        ignored,
    } = parsed;
    let content_type = headers
        .iter()
        .find(|row| row.key.to_lowercase() == "content-type")
        .map(|row| row.value.clone())
        .unwrap_or_default();
    if json {
        if content_type.is_empty() {
            headers.push(pair("Content-Type", "application/json"));
        }
        if !headers.iter().any(|row| row.key.to_lowercase() == "accept") {
            headers.push(pair("Accept", "application/json"));
        }
    }
    let mut draft = create_draft();
    draft.url = url.clone();
    draft.headers = if headers.is_empty() {
        vec![pair("", "")]
    } else {
        headers
    };
    draft.query = vec![pair("", "")];
    let body = data.join("&");
    let has_form = !form.is_empty();
    if get && !body.is_empty() {
        url += if url.contains('?') { "&" } else { "?" };
        url += &body;
        draft.url = url;
    } else if has_form {
        draft.body_mode = BodyMode::Multipart;
        draft.form = Some(form);
        // Multipart sets its own Content-Type with the boundary.
        draft
            .headers
            .retain(|row| row.key.to_lowercase() != "content-type");
        if draft.headers.is_empty() {
            draft.headers = vec![pair("", "")];
        }
    } else if let Some(file) = &data_file {
        draft.body_mode = BodyMode::File;
        draft.body_file = Some(file.clone());
    } else if !data.is_empty() {
        draft.body_mode =
            if contains_ignore_ascii_case(&content_type, "json") || json || is_json(&body) {
                BodyMode::Json
            } else {
                BodyMode::Text
            };
        draft.body = body;
    }
    let sends_body = !get && (has_form || data_file.is_some() || !data.is_empty());
    draft.method = method.unwrap_or_else(|| {
        if head {
            "HEAD"
        } else if sends_body {
            "POST"
        } else {
            "GET"
        }
        .into()
    });
    if !is_method(&draft.method) {
        return Err(format!("Invalid method: {}", draft.method));
    }

    // Move credentials to the Auth tab, where Blink builds the header.
    let auth_index = draft
        .headers
        .iter()
        .position(|row| row.key.to_lowercase() == "authorization");
    let auth_value = auth_index
        .map(|index| draft.headers[index].value.clone())
        .unwrap_or_default();
    let bearer = scheme_credential(&auth_value, "Bearer");
    let decoded = scheme_credential(&auth_value, "Basic").and_then(decode_basic);
    let from_header = bearer.is_some() || decoded.is_some();
    if let Some(user) = user {
        let (username, password) = match user.find(':') {
            Some(colon) => (user[..colon].to_string(), user[colon + 1..].to_string()),
            None => (user, String::new()),
        };
        draft.local_auth = Some(AuthorizationConfig::Basic { username, password });
    } else if let Some(token) = bearer {
        draft.local_auth = Some(AuthorizationConfig::Bearer {
            token: token.to_string(),
        });
    } else if let Some((username, password)) = decoded {
        draft.local_auth = Some(AuthorizationConfig::Basic { username, password });
    }
    if let (Some(_), true, Some(index)) = (&draft.local_auth, from_header, auth_index) {
        draft.headers.remove(index);
        if draft.headers.is_empty() {
            draft.headers = vec![pair("", "")];
        }
    }
    let mut unique: Vec<String> = Vec::new();
    for option in ignored {
        if !unique.contains(&option) {
            unique.push(option);
        }
    }
    Ok(CurlImport {
        draft,
        ignored: unique,
    })
}

fn is_json(text: &str) -> bool {
    let start = text.trim_start_matches(is_js_space);
    if !start.starts_with(['[', '{']) {
        return false;
    }
    serde_json::from_str::<serde_json::Value>(text).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::TransportOptions;
    use crate::request::{build_request, to_curl};

    fn rows(items: &[Pair]) -> Vec<(String, String)> {
        items
            .iter()
            .map(|row| (row.key.clone(), row.value.clone()))
            .collect()
    }

    fn row(key: &str, value: &str) -> (String, String) {
        (key.into(), value.into())
    }

    #[test]
    fn handles_quotes_escapes_and_line_continuations() {
        assert_eq!(
            shell_words("curl 'a b' \"c \\\"d\\\" \\$e\" f\\ g \\\n  $'h\\nI' 'it'\"'\"'s'")
                .unwrap(),
            vec!["curl", "a b", "c \"d\" $e", "f g", "h\nI", "it's"]
        );
    }

    #[test]
    fn rejects_an_unclosed_quote() {
        assert!(
            shell_words("curl 'oops")
                .unwrap_err()
                .contains("unclosed quote")
        );
    }

    #[test]
    fn detects_curl_commands() {
        assert!(is_curl_command("  curl https://x"));
        assert!(!is_curl_command("https://x/curl"));
    }

    #[test]
    fn reads_method_url_headers_and_a_json_body() {
        let draft = parse_curl(
            "curl -X PATCH https://api.test/items/1 -H 'Content-Type: application/json' -H 'X-Trace: a:b' -d '{\"name\":\"x\"}'",
        )
        .unwrap()
        .draft;
        assert_eq!(draft.method, "PATCH");
        assert_eq!(draft.url, "https://api.test/items/1");
        assert_eq!(
            rows(&draft.headers),
            vec![
                row("Content-Type", "application/json"),
                row("X-Trace", "a:b")
            ]
        );
        assert_eq!(draft.body_mode, BodyMode::Json);
        assert_eq!(draft.body, "{\"name\":\"x\"}");
    }

    #[test]
    fn defaults_to_post_with_data_and_get_without() {
        assert_eq!(
            parse_curl("curl https://x -d a=1").unwrap().draft.method,
            "POST"
        );
        assert_eq!(parse_curl("curl https://x").unwrap().draft.method, "GET");
        assert_eq!(
            parse_curl("curl -I https://x").unwrap().draft.method,
            "HEAD"
        );
    }

    #[test]
    fn joins_data_flags_and_moves_them_to_the_url_with_get() {
        let draft = parse_curl("curl -G https://x/s?a=1 -d q=blue --data-urlencode 'name=a b'")
            .unwrap()
            .draft;
        assert_eq!(draft.method, "GET");
        assert_eq!(draft.url, "https://x/s?a=1&q=blue&name=a%20b");
    }

    #[test]
    fn reads_grouped_short_flags_and_attached_values() {
        let CurlImport { draft, ignored } =
            parse_curl("curl -sSL -XPUT -HAccept:text/plain -o out.txt https://x").unwrap();
        assert_eq!(draft.method, "PUT");
        assert_eq!(draft.url, "https://x");
        assert_eq!(rows(&draft.headers), vec![row("Accept", "text/plain")]);
        assert_eq!(ignored, vec!["-s", "-S", "-L", "-o"]);
    }

    #[test]
    fn moves_bearer_and_basic_credentials_to_the_auth_tab() {
        let bearer = parse_curl("curl https://x -H 'Authorization: Bearer abc.def'")
            .unwrap()
            .draft;
        assert_eq!(
            bearer.local_auth,
            Some(AuthorizationConfig::Bearer {
                token: "abc.def".into()
            })
        );
        assert_eq!(rows(&bearer.headers), vec![row("", "")]);
        let user = parse_curl("curl -u ada:s3:cret https://x").unwrap().draft;
        assert_eq!(
            user.local_auth,
            Some(AuthorizationConfig::Basic {
                username: "ada".into(),
                password: "s3:cret".into()
            })
        );
        // "ada:pw" in base64.
        let header = parse_curl("curl https://x -H \"Authorization: Basic YWRhOnB3\"")
            .unwrap()
            .draft;
        assert_eq!(
            header.local_auth,
            Some(AuthorizationConfig::Basic {
                username: "ada".into(),
                password: "pw".into()
            })
        );
    }

    #[test]
    fn reads_multipart_forms_with_files() {
        let draft = parse_curl(
            "curl https://x -F title=Hi -F 'upload=@/tmp/a.png;type=image/png' -H 'Content-Type: multipart/form-data'",
        )
        .unwrap()
        .draft;
        assert_eq!(draft.method, "POST");
        assert_eq!(draft.body_mode, BodyMode::Multipart);
        let form = draft.form.unwrap();
        assert_eq!(
            rows(&form),
            vec![row("title", "Hi"), row("upload", "/tmp/a.png")]
        );
        assert_eq!(form[1].file, Some(true));
        assert_eq!(rows(&draft.headers), vec![row("", "")]);
    }

    #[test]
    fn reads_json_and_a_file_body() {
        let json = parse_curl("curl --json '{\"a\":1}' https://x")
            .unwrap()
            .draft;
        assert_eq!(json.body_mode, BodyMode::Json);
        assert_eq!(
            rows(&json.headers),
            vec![
                row("Content-Type", "application/json"),
                row("Accept", "application/json")
            ]
        );
        let file = parse_curl("curl --data-binary @dump.bin https://x")
            .unwrap()
            .draft;
        assert_eq!(file.body_mode, BodyMode::File);
        assert_eq!(file.body_file.as_deref(), Some("dump.bin"));
    }

    #[test]
    fn explains_a_command_without_a_url() {
        assert!(parse_curl("curl -s").unwrap_err().contains("no URL"));
        assert_eq!(parse_curl("curl -X").unwrap_err(), "-X needs a value.");
    }

    #[test]
    fn round_trips_a_blink_curl_export() {
        let source = Draft {
            method: "POST".into(),
            url: "https://api.test/items?x=1".into(),
            headers: vec![pair("Accept", "application/json"), pair("X-A", "it's")],
            body_mode: BodyMode::Json,
            body: "{\"a\": \"b c\"}".into(),
            local_auth: Some(AuthorizationConfig::Bearer {
                token: "tok".into(),
            }),
            ..create_draft()
        };
        let options = TransportOptions::default();
        let exported = build_request(&source, None).unwrap();
        let imported = build_request(
            &parse_curl(&to_curl(&exported, &options)).unwrap().draft,
            None,
        )
        .unwrap();
        let sorted = |mut request: crate::model::RequestInput| {
            request.headers.sort_by(|a, b| a.key.cmp(&b.key));
            request
        };
        assert_eq!(sorted(imported), sorted(exported));
    }
}
