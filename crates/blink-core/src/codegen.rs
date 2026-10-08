//! Copy a request as source code. Port of `src/lib/codegen.ts`.

use std::collections::BTreeSet;

use crate::model::{CodeTarget, RequestInput, TransportOptions};
use crate::request::to_curl;

/// The saved id of a target, such as `curl`.
pub fn code_target_id(target: CodeTarget) -> &'static str {
    match target {
        CodeTarget::Curl => "curl",
        CodeTarget::Fetch => "fetch",
        CodeTarget::Python => "python",
        CodeTarget::Go => "go",
        CodeTarget::Httpie => "httpie",
        CodeTarget::Rust => "rust",
    }
}

pub fn code_target_from_id(id: &str) -> Option<CodeTarget> {
    CodeTarget::ALL
        .into_iter()
        .find(|target| code_target_id(*target) == id)
}

pub fn is_code_target(value: &str) -> bool {
    code_target_from_id(value).is_some()
}

// JSON string literals are valid string literals in JavaScript, Python and Go.
fn str(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

fn shell(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

/// The last path segment, split on `/` or `\`. May be empty.
fn last_segment(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// A Rust string literal. Rust has no \b, \f or \uXXXX escapes.
fn rust_string(value: &str) -> String {
    let mut out = String::from("\"");
    for char in value.chars() {
        let code = char as u32;
        match char {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ if code < 0x20 || code == 0x7f => out.push_str(&format!("\\u{{{code:x}}}")),
            _ => out.push(char),
        }
    }
    out.push('"');
    out
}

fn fetch_code(request: &RequestInput, options: &TransportOptions) -> String {
    let mut lines: Vec<String> = Vec::new();
    let files = request
        .body_file
        .as_deref()
        .is_some_and(|file| !file.is_empty())
        || request
            .multipart
            .as_ref()
            .is_some_and(|parts| parts.iter().any(|p| p.file));
    if files {
        lines.push("import { openAsBlob } from \"node:fs\";".into());
        lines.push(String::new());
    }
    let mut body = String::new();
    if let Some(multipart) = &request.multipart {
        lines.push("const form = new FormData();".into());
        for part in multipart {
            lines.push(if part.file {
                format!(
                    "form.append({}, await openAsBlob({}), {});",
                    str(&part.key),
                    str(&part.value),
                    str(last_segment(&part.value))
                )
            } else {
                format!("form.append({}, {});", str(&part.key), str(&part.value))
            });
        }
        lines.push(String::new());
        body = "form".into();
    } else if let Some(file) = request.body_file.as_deref().filter(|file| !file.is_empty()) {
        body = format!("await openAsBlob({})", str(file));
    } else if let Some(text) = &request.body {
        body = str(text);
    }
    lines.push(format!(
        "const response = await fetch({}, {{",
        str(&request.url)
    ));
    lines.push(format!("  method: {},", str(&request.method)));
    if !request.headers.is_empty() {
        lines.push("  headers: [".into());
        for header in &request.headers {
            lines.push(format!(
                "    [{}, {}],",
                str(&header.key),
                str(&header.value)
            ));
        }
        lines.push("  ],".into());
    }
    if !body.is_empty() {
        lines.push(format!("  body: {body},"));
    }
    if !options.follow_redirects {
        lines.push("  redirect: \"manual\",".into());
    }
    lines.push(format!(
        "  signal: AbortSignal.timeout({}),",
        options.timeout_seconds * 1000
    ));
    lines.push("});".into());
    lines.push("console.log(response.status, response.statusText);".into());
    lines.push("console.log(await response.text());".into());
    lines.join("\n")
}

fn python_code(request: &RequestInput, options: &TransportOptions) -> String {
    let mut args = vec![
        format!("    {},", str(&request.method)),
        format!("    {},", str(&request.url)),
    ];
    if !request.headers.is_empty() {
        args.push("    headers={".into());
        for header in &request.headers {
            args.push(format!(
                "        {}: {},",
                str(&header.key),
                str(&header.value)
            ));
        }
        args.push("    },".into());
    }
    if let Some(multipart) = &request.multipart {
        // A list keeps repeated keys and the part order.
        args.push("    files=[".into());
        for part in multipart {
            args.push(if part.file {
                format!(
                    "        ({}, open({}, \"rb\")),",
                    str(&part.key),
                    str(&part.value)
                )
            } else {
                format!(
                    "        ({}, (None, {})),",
                    str(&part.key),
                    str(&part.value)
                )
            });
        }
        args.push("    ],".into());
    } else if let Some(file) = request.body_file.as_deref().filter(|file| !file.is_empty()) {
        args.push(format!("    data=open({}, \"rb\"),", str(file)));
    } else if let Some(text) = &request.body {
        args.push(format!("    data={}.encode(),", str(text)));
    }
    args.push(format!(
        "    timeout=({}, {}),",
        options.connect_timeout_seconds, options.timeout_seconds
    ));
    args.push(format!(
        "    allow_redirects={},",
        if options.follow_redirects {
            "True"
        } else {
            "False"
        }
    ));
    if !options.verify_tls {
        args.push("    verify=False,".into());
    }
    if !options.proxy_url.is_empty() {
        args.push(format!(
            "    proxies={{\"http\": {}, \"https\": {}}},",
            str(&options.proxy_url),
            str(&options.proxy_url)
        ));
    }
    let mut lines = vec!["import requests".to_string(), String::new()];
    if options.follow_redirects {
        lines.push("session = requests.Session()".into());
        lines.push(format!("session.max_redirects = {}", options.max_redirects));
        lines.push("response = session.request(".into());
    } else {
        lines.push("response = requests.request(".into());
    }
    lines.extend(args);
    lines.push(")".into());
    lines.push("print(response.status_code, response.reason)".into());
    lines.push("print(response.text)".into());
    lines.join("\n")
}

fn go_code(request: &RequestInput, options: &TransportOptions) -> String {
    let mut imports: BTreeSet<&str> = ["fmt", "io", "net/http", "time"].into_iter().collect();
    let mut setup: Vec<String> = Vec::new();
    let mut body = "nil".to_string();
    let push = |setup: &mut Vec<String>, lines: &[&str]| {
        setup.extend(lines.iter().map(|line| line.to_string()));
    };
    if let Some(multipart) = &request.multipart {
        imports.insert("bytes");
        imports.insert("mime/multipart");
        push(
            &mut setup,
            &[
                "\tvar body bytes.Buffer",
                "\tform := multipart.NewWriter(&body)",
            ],
        );
        for part in multipart {
            if part.file {
                imports.insert("os");
                let name = last_segment(&part.value);
                setup.push("\t{".into());
                setup.push(format!("\t\tfile, err := os.Open({})", str(&part.value)));
                push(
                    &mut setup,
                    &["\t\tif err != nil {", "\t\t\tpanic(err)", "\t\t}"],
                );
                setup.push(format!(
                    "\t\tpart, err := form.CreateFormFile({}, {})",
                    str(&part.key),
                    str(name)
                ));
                push(
                    &mut setup,
                    &[
                        "\t\tif err != nil {",
                        "\t\t\tpanic(err)",
                        "\t\t}",
                        "\t\tif _, err := io.Copy(part, file); err != nil {",
                        "\t\t\tpanic(err)",
                        "\t\t}",
                        "\t\tfile.Close()",
                        "\t}",
                    ],
                );
            } else {
                setup.push(format!(
                    "\tform.WriteField({}, {})",
                    str(&part.key),
                    str(&part.value)
                ));
            }
        }
        push(&mut setup, &["\tform.Close()", ""]);
        body = "&body".into();
    } else if let Some(file) = request.body_file.as_deref().filter(|file| !file.is_empty()) {
        imports.insert("os");
        setup.push(format!("\tbody, err := os.Open({})", str(file)));
        push(
            &mut setup,
            &[
                "\tif err != nil {",
                "\t\tpanic(err)",
                "\t}",
                "\tdefer body.Close()",
                "",
            ],
        );
        body = "body".into();
    } else if let Some(text) = &request.body {
        imports.insert("strings");
        body = format!("strings.NewReader({})", str(text));
    }
    let mut headers: Vec<String> = request
        .headers
        .iter()
        .map(|header| {
            format!(
                "\treq.Header.Add({}, {})",
                str(&header.key),
                str(&header.value)
            )
        })
        .collect();
    if request.multipart.is_some() {
        headers.push("\treq.Header.Set(\"Content-Type\", form.FormDataContentType())".into());
    }
    let mut transport: Vec<String> = Vec::new();
    if !options.verify_tls {
        imports.insert("crypto/tls");
        transport.push("\t\t\tTLSClientConfig: &tls.Config{InsecureSkipVerify: true},".into());
    }
    if !options.proxy_url.is_empty() {
        imports.insert("net/url");
        transport.push("\t\t\tProxy: http.ProxyURL(proxy),".into());
        setup.push(format!(
            "\tproxy, err := url.Parse({})",
            str(&options.proxy_url)
        ));
        push(
            &mut setup,
            &["\tif err != nil {", "\t\tpanic(err)", "\t}", ""],
        );
    }
    let mut client = vec![
        "\tclient := &http.Client{".to_string(),
        format!("\t\tTimeout: {} * time.Second,", options.timeout_seconds),
    ];
    if !transport.is_empty() {
        client.push("\t\tTransport: &http.Transport{".into());
        client.extend(transport);
        client.push("\t\t},".into());
    }
    client.push("\t\tCheckRedirect: func(req *http.Request, via []*http.Request) error {".into());
    if options.follow_redirects {
        client.push(format!("\t\t\tif len(via) >= {} {{", options.max_redirects));
        client.push(format!(
            "\t\t\t\treturn fmt.Errorf(\"stopped after {} redirects\")",
            options.max_redirects
        ));
        client.push("\t\t\t}".into());
        client.push("\t\t\treturn nil".into());
    } else {
        client.push("\t\t\treturn http.ErrUseLastResponse".into());
    }
    client.push("\t\t},".into());
    client.push("\t}".into());

    let mut lines = vec!["package main".to_string(), String::new(), "import (".into()];
    lines.extend(imports.iter().map(|name| format!("\t{}", str(name))));
    lines.push(")".into());
    lines.push(String::new());
    lines.push("func main() {".into());
    lines.extend(setup);
    lines.push(format!(
        "\treq, err := http.NewRequest({}, {}, {body})",
        str(&request.method),
        str(&request.url)
    ));
    lines.extend(["\tif err != nil {", "\t\tpanic(err)", "\t}"].map(String::from));
    lines.extend(headers);
    lines.push(String::new());
    lines.extend(client);
    lines.extend(
        [
            "\tresp, err := client.Do(req)",
            "\tif err != nil {",
            "\t\tpanic(err)",
            "\t}",
            "\tdefer resp.Body.Close()",
            "\tdata, err := io.ReadAll(resp.Body)",
            "\tif err != nil {",
            "\t\tpanic(err)",
            "\t}",
            "\tfmt.Println(resp.Status)",
            "\tfmt.Println(string(data))",
            "}",
        ]
        .map(String::from),
    );
    lines.join("\n")
}

fn httpie_code(request: &RequestInput, options: &TransportOptions) -> String {
    let mut parts = vec![format!(
        "http --ignore-stdin --timeout={}",
        options.timeout_seconds
    )];
    if options.follow_redirects {
        parts.push(format!(
            "--follow --max-redirects={}",
            options.max_redirects
        ));
    }
    if !options.verify_tls {
        parts.push("--verify=no".into());
    }
    if !options.proxy_url.is_empty() {
        parts.push(format!(
            "--proxy={} --proxy={}",
            shell(&format!("http:{}", options.proxy_url)),
            shell(&format!("https:{}", options.proxy_url))
        ));
    }
    if request.multipart.is_some() {
        parts.push("--multipart".into());
    }
    let body_file = request.body_file.as_deref().filter(|file| !file.is_empty());
    if body_file.is_some() {
        parts[0] = parts[0].replacen(" --ignore-stdin", "", 1);
    } else if let Some(text) = &request.body {
        parts.push(format!("--raw {}", shell(text)));
    }
    parts.push(request.method.clone());
    parts.push(shell(&request.url));
    for header in &request.headers {
        parts.push(shell(&format!("{}:{}", header.key, header.value)));
    }
    for part in request.multipart.iter().flatten() {
        let separator = if part.file { '@' } else { '=' };
        parts.push(shell(&format!("{}{separator}{}", part.key, part.value)));
    }
    let mut command = parts.join(" \\\n  ");
    if let Some(file) = body_file {
        command += &format!(" \\\n  < {}", shell(file));
    }
    command
}

const STANDARD_METHODS: [&str; 7] = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

fn rust_code(request: &RequestInput, options: &TransportOptions) -> String {
    let s = rust_string;
    let mut builder = vec![
        "    let client = reqwest::blocking::Client::builder()".to_string(),
        format!(
            "        .timeout(Duration::from_secs({}))",
            options.timeout_seconds
        ),
        format!(
            "        .connect_timeout(Duration::from_secs({}))",
            options.connect_timeout_seconds
        ),
        if options.follow_redirects {
            format!(
                "        .redirect(reqwest::redirect::Policy::limited({}))",
                options.max_redirects
            )
        } else {
            "        .redirect(reqwest::redirect::Policy::none())".into()
        },
    ];
    if !options.verify_tls {
        builder.push("        .danger_accept_invalid_certs(true)".into());
    }
    if !options.proxy_url.is_empty() {
        builder.push(format!(
            "        .proxy(reqwest::Proxy::all({})?)",
            s(&options.proxy_url)
        ));
    }
    builder.push("        .build()?;".into());
    let mut setup: Vec<String> = Vec::new();
    if let Some(multipart) = &request.multipart {
        setup.push("    let form = reqwest::blocking::multipart::Form::new()".into());
        for part in multipart {
            setup.push(if part.file {
                format!("        .file({}, {})?", s(&part.key), s(&part.value))
            } else {
                format!("        .text({}, {})", s(&part.key), s(&part.value))
            });
        }
        if let Some(last) = setup.last_mut() {
            last.push(';');
        }
    }
    let method = if STANDARD_METHODS.contains(&request.method.as_str()) {
        format!("reqwest::Method::{}", request.method)
    } else {
        format!("reqwest::Method::from_bytes(b{})?", s(&request.method))
    };
    let mut call = vec![
        "    let response = client".to_string(),
        format!("        .request({method}, {})", s(&request.url)),
    ];
    call.extend(
        request
            .headers
            .iter()
            .map(|header| format!("        .header({}, {})", s(&header.key), s(&header.value))),
    );
    if request.multipart.is_some() {
        call.push("        .multipart(form)".into());
    } else if let Some(file) = request.body_file.as_deref().filter(|file| !file.is_empty()) {
        call.push(format!("        .body(std::fs::read({})?)", s(file)));
    } else if let Some(text) = &request.body {
        call.push(format!("        .body({})", s(text)));
    }
    call.push("        .send()?;".into());
    let mut lines = vec![
        "use std::time::Duration;".to_string(),
        String::new(),
        "// Cargo.toml: reqwest = { version = \"0.12\", features = [\"blocking\", \"multipart\"] }"
            .into(),
        "fn main() -> Result<(), Box<dyn std::error::Error>> {".into(),
    ];
    lines.extend(builder);
    lines.extend(setup);
    lines.extend(call);
    lines.extend(
        [
            "    println!(\"{}\", response.status());",
            "    println!(\"{}\", response.text()?);",
            "    Ok(())",
            "}",
        ]
        .map(String::from),
    );
    lines.join("\n")
}

/// Source code that sends what Blink sends with these transport options.
pub fn generate_code(
    target: CodeTarget,
    request: &RequestInput,
    options: &TransportOptions,
) -> String {
    match target {
        CodeTarget::Fetch => fetch_code(request, options),
        CodeTarget::Python => python_code(request, options),
        CodeTarget::Go => go_code(request, options),
        CodeTarget::Httpie => httpie_code(request, options),
        CodeTarget::Rust => rust_code(request, options),
        CodeTarget::Curl => to_curl(request, options),
    }
}

/// Generate code for display or sharing. Saved credential references stay
/// references; literal authorization and sensitive header values are masked.
/// The pure generator remains available for callers that need exact requests.
pub fn generate_redacted_code(
    target: CodeTarget,
    request: &RequestInput,
    options: &TransportOptions,
) -> String {
    let mut request = request.clone();
    for header in &mut request.headers {
        if crate::inspection::sensitive(&header.key) {
            header.value = redact_header_value(&header.value);
        }
    }
    request.url = redact_url_credentials(&request.url);
    let mut options = options.clone();
    options.proxy_url = redact_url_credentials(&options.proxy_url);
    generate_code(target, &request, &options)
}

fn redact_header_value(value: &str) -> String {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    if let Some((scheme, credentials)) = value.split_once(' ')
        && (scheme.eq_ignore_ascii_case("basic") || scheme.eq_ignore_ascii_case("bearer"))
    {
        if scheme.eq_ignore_ascii_case("basic") {
            // Basic auth was encoded during request preparation. Keep only
            // reference placeholders from its decoded value, never literals.
            if let Ok(bytes) = STANDARD.decode(credentials)
                && let Ok(decoded) = String::from_utf8(bytes)
            {
                let safe = decoded
                    .split_once(':')
                    .map(|(user, password)| {
                        format!("{}:{}", redact_value(user), redact_value(password))
                    })
                    .unwrap_or_else(|| redact_value(&decoded));
                if safe.contains("{{") {
                    return format!("{scheme} {}", STANDARD.encode(safe));
                }
            }
            return format!("{scheme} [redacted]");
        }
        return format!("{scheme} {}", redact_value(credentials));
    }
    redact_value(value)
}

static CREDENTIAL_REFERENCES: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r"\{\{[@!][A-Za-z0-9_.-]+\}\}").expect("credential reference pattern")
});

fn redact_value(value: &str) -> String {
    let mut safe = String::new();
    let mut end = 0;
    for reference in CREDENTIAL_REFERENCES.find_iter(value) {
        if reference.start() > end {
            safe.push_str("[redacted]");
        }
        safe.push_str(reference.as_str());
        end = reference.end();
    }
    if end < value.len() {
        safe.push_str("[redacted]");
    }
    safe
}

fn redact_url_credentials(value: &str) -> String {
    let Some((scheme, rest)) = value.split_once("://") else {
        return value.into();
    };
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    let Some((at, _)) = authority.rmatch_indices('@').find(|(at, _)| {
        !CREDENTIAL_REFERENCES
            .find_iter(authority)
            .any(|reference| reference.range().contains(at))
    }) else {
        return value.into();
    };
    let (userinfo, host) = (&authority[..at], &authority[at + 1..]);
    format!(
        "{scheme}://{}@{host}{}",
        redact_value(userinfo),
        &rest[authority_end..]
    )
}

/// The highlight language for a target's snippet.
pub fn code_language(target: CodeTarget) -> &'static str {
    match target {
        CodeTarget::Curl | CodeTarget::Httpie => "bash",
        CodeTarget::Fetch => "javascript",
        CodeTarget::Python => "python",
        CodeTarget::Go => "go",
        CodeTarget::Rust => "rust",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Header, MultipartPart};

    fn post() -> RequestInput {
        RequestInput {
            method: "POST".into(),
            url: "https://api.example.test/items?q=a%20b".into(),
            headers: vec![
                Header {
                    key: "Content-Type".into(),
                    value: "application/json".into(),
                },
                Header {
                    key: "Authorization".into(),
                    value: "Bearer it's".into(),
                },
            ],
            body: Some("{\"name\":\"line\nbreak\"}".into()),
            ..Default::default()
        }
    }

    fn multipart() -> RequestInput {
        RequestInput {
            method: "POST".into(),
            url: "https://api.example.test/upload".into(),
            headers: vec![],
            body: None,
            multipart: Some(vec![
                MultipartPart {
                    key: "note".into(),
                    value: "hello".into(),
                    file: false,
                },
                MultipartPart {
                    key: "file".into(),
                    value: "/tmp/a b.png".into(),
                    file: true,
                },
            ]),
            ..Default::default()
        }
    }

    fn defaults() -> TransportOptions {
        TransportOptions::default()
    }

    #[test]
    fn reuses_to_curl_for_curl() {
        assert_eq!(
            generate_code(CodeTarget::Curl, &post(), &defaults()),
            to_curl(&post(), &defaults())
        );
    }

    #[test]
    fn knows_every_target_id() {
        for target in CodeTarget::ALL {
            assert!(is_code_target(code_target_id(target)));
        }
        assert!(!is_code_target("perl"));
    }

    #[test]
    fn writes_fetch_with_headers_body_and_timeout() {
        let code = generate_code(CodeTarget::Fetch, &post(), &defaults());
        assert!(code.contains("await fetch(\"https://api.example.test/items?q=a%20b\""));
        assert!(code.contains("[\"Authorization\", \"Bearer it's\"]"));
        assert!(code.contains("body: \"{\\\"name\\\":\\\"line\\nbreak\\\"}\""));
        assert!(code.contains("redirect: \"manual\""));
        assert!(code.contains("AbortSignal.timeout(30000)"));
    }

    #[test]
    fn writes_fetch_multipart_with_open_as_blob() {
        let code = generate_code(CodeTarget::Fetch, &multipart(), &defaults());
        assert!(code.contains("import { openAsBlob } from \"node:fs\";"));
        assert!(code.contains("form.append(\"note\", \"hello\");"));
        assert!(
            code.contains(
                "form.append(\"file\", await openAsBlob(\"/tmp/a b.png\"), \"a b.png\");"
            )
        );
    }

    #[test]
    fn writes_python_with_transport_options() {
        let options = TransportOptions {
            verify_tls: false,
            follow_redirects: true,
            proxy_url: "http://proxy.test:8080".into(),
            ..defaults()
        };
        let code = generate_code(CodeTarget::Python, &post(), &options);
        assert!(code.contains("session.max_redirects = 10"));
        assert!(code.contains("verify=False"));
        assert!(code.contains("\"https\": \"http://proxy.test:8080\""));
        assert!(code.contains("allow_redirects=True"));
        assert!(code.contains("timeout=(10, 30)"));
    }

    #[test]
    fn writes_python_multipart_parts_in_order() {
        let code = generate_code(CodeTarget::Python, &multipart(), &defaults());
        let note = code.find("(\"note\", (None, \"hello\"))").unwrap();
        let file = code
            .find("(\"file\", open(\"/tmp/a b.png\", \"rb\"))")
            .unwrap();
        assert!(note < file);
    }

    #[test]
    fn writes_go_with_sorted_imports_and_no_redirect_policy() {
        let code = generate_code(CodeTarget::Go, &post(), &defaults());
        assert!(code.contains("\t\"strings\""));
        assert!(code.contains("return http.ErrUseLastResponse"));
        assert!(code.contains("req.Header.Add(\"Authorization\", \"Bearer it's\")"));
        let imports = code[code.find('(').unwrap() + 1..code.find(')').unwrap()].trim();
        let names: Vec<&str> = imports.split('\n').map(str::trim).collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
    }

    #[test]
    fn writes_go_multipart_with_the_form_content_type() {
        let code = generate_code(CodeTarget::Go, &multipart(), &defaults());
        assert!(code.contains("form.CreateFormFile(\"file\", \"a b.png\")"));
        assert!(code.contains("form.FormDataContentType()"));
    }

    #[test]
    fn writes_httpie_with_raw_body_and_shell_quoting() {
        let code = generate_code(CodeTarget::Httpie, &post(), &defaults());
        assert!(code.contains("--raw '{\"name\":\"line\nbreak\"}'"));
        assert!(code.contains("'Authorization:Bearer it'\"'\"'s'"));
    }

    #[test]
    fn writes_httpie_file_body_from_stdin() {
        let request = RequestInput {
            body: None,
            body_file: Some("/tmp/body.bin".into()),
            ..post()
        };
        let code = generate_code(CodeTarget::Httpie, &request, &defaults());
        assert!(!code.contains("--ignore-stdin"));
        assert!(code.contains("< '/tmp/body.bin'"));
    }

    #[test]
    fn writes_rust_with_rust_string_escapes() {
        let request = RequestInput {
            body: Some("a\"\\\u{8}".into()),
            method: "PURGE".into(),
            ..post()
        };
        let code = generate_code(CodeTarget::Rust, &request, &defaults());
        assert!(code.contains(".body(\"a\\\"\\\\\\u{8}\")"));
        assert!(code.contains("reqwest::Method::from_bytes(b\"PURGE\")?"));
        assert!(code.contains("reqwest::redirect::Policy::none()"));
    }
    #[test]
    fn shared_code_masks_auth_and_sensitive_headers_for_every_target() {
        let request = RequestInput {
            method: "GET".into(),
            url: "https://example.test".into(),
            headers: vec![
                Header {
                    key: "Authorization".into(),
                    value: "Bearer private-bearer".into(),
                },
                Header {
                    key: "X-API-Key".into(),
                    value: "private-key".into(),
                },
                Header {
                    key: "Cookie".into(),
                    value: "session=private-cookie".into(),
                },
                Header {
                    key: "Accept".into(),
                    value: "application/json".into(),
                },
            ],
            ..Default::default()
        };
        let mut options = defaults();
        options.proxy_url = "http://proxy-user:private-proxy@localhost:8888".into();
        for target in CodeTarget::ALL {
            let code = generate_redacted_code(target, &request, &options);
            for secret in [
                "private-bearer",
                "private-key",
                "private-cookie",
                "private-proxy",
                "proxy-user",
            ] {
                assert!(!code.contains(secret), "{target:?}: {secret}");
            }
            assert!(code.contains("application/json"));
            assert!(code.contains("[redacted]"));
        }
        assert_eq!(request.headers[0].value, "Bearer private-bearer");
    }

    #[test]
    fn shared_code_preserves_named_and_environment_references() {
        let request = RequestInput {
            method: "GET".into(),
            url: "https://example.test/{{@path}}".into(),
            headers: vec![
                Header {
                    key: "Authorization".into(),
                    value: "Bearer {{@token}}".into(),
                },
                Header {
                    key: "X-API-Key".into(),
                    value: "{{!API_KEY}}".into(),
                },
            ],
            ..Default::default()
        };
        for target in CodeTarget::ALL {
            let code = generate_redacted_code(target, &request, &defaults());
            assert!(code.contains("{{@token}}"));
            assert!(code.contains("{{!API_KEY}}"));
        }
        assert_eq!(
            redact_url_credentials("https://{{@host}}/path"),
            "https://{{@host}}/path"
        );
        assert_eq!(
            redact_url_credentials("https://user:{{@password}}@example.test"),
            "https://[redacted]{{@password}}@example.test"
        );
    }

    #[test]
    fn basic_auth_redacts_encoded_literals_but_keeps_references() {
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        assert_eq!(
            redact_header_value(&format!("Basic {}", STANDARD.encode("user:password"))),
            "Basic [redacted]"
        );
        let safe = redact_header_value(&format!(
            "Basic {}",
            STANDARD.encode("private-user:{{@password}}")
        ));
        let decoded = String::from_utf8(
            STANDARD
                .decode(safe.strip_prefix("Basic ").unwrap())
                .unwrap(),
        )
        .unwrap();
        assert_eq!(decoded, "[redacted]:{{@password}}");
        assert_eq!(
            redact_header_value("literal{{@name}}literal"),
            "[redacted]{{@name}}[redacted]"
        );
    }
}
