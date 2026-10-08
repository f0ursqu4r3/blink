//! Named credentials resolved only by the transport, never by saved drafts.
use crate::model::RequestInput;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use sha2::{Digest, Sha256};

pub fn service(scope: Option<&str>) -> String {
    format!(
        "com.kyle.blink.credentials.{:x}",
        Sha256::digest(scope.unwrap_or("local").as_bytes())
    )
}

pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_.-".contains(&c))
}

pub fn set(scope: Option<&str>, name: &str, value: &[u8]) -> Result<(), String> {
    #[cfg(test)]
    if let Some(result) = crate::engine::auth_test_support::with_store(scope, |store| {
        store.insert(name.into(), value.to_vec());
    }) {
        return result;
    }
    #[cfg(target_os = "macos")]
    {
        security_framework::passwords::set_generic_password(&service(scope), name, value)
            .map_err(|_| "Cannot save the credential in Keychain.".into())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (scope, name, value);
        Err("OS credential storage is supported on macOS only.".into())
    }
}
pub fn get(scope: Option<&str>, name: &str) -> Result<Option<Vec<u8>>, String> {
    #[cfg(test)]
    if let Some(result) =
        crate::engine::auth_test_support::with_store(scope, |store| store.get(name).cloned())
    {
        return result;
    }
    #[cfg(target_os = "macos")]
    {
        match security_framework::passwords::get_generic_password(&service(scope), name) {
            Ok(value) => Ok(Some(value)),
            Err(error) if error.code() == -25300 => Ok(None),
            Err(_) => Err("Cannot read the credential from Keychain.".into()),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (scope, name);
        Ok(None)
    }
}
pub fn delete(scope: Option<&str>, name: &str) -> Result<(), String> {
    #[cfg(test)]
    if let Some(result) = crate::engine::auth_test_support::with_store(scope, |store| {
        store.remove(name);
    }) {
        return result;
    }
    #[cfg(target_os = "macos")]
    {
        match security_framework::passwords::delete_generic_password(&service(scope), name) {
            Ok(()) => Ok(()),
            Err(error) if error.code() == -25300 => Ok(()),
            Err(_) => Err("Cannot remove the credential from Keychain.".into()),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (scope, name);
        Err("OS credential storage is supported on macOS only.".into())
    }
}

fn encoded_references() -> &'static regex::Regex {
    static RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"(?i)%7b%7b(?:%40|@)([a-z0-9_.-]+)%7d%7d").unwrap()
    });
    &RE
}
pub fn references(text: &str) -> Vec<String> {
    let normalized = encoded_references().replace_all(text, |caps: &regex::Captures| {
        format!("{{{{@{}}}}}", &caps[1])
    });
    let mut rest = normalized.as_ref();
    let mut names = Vec::new();
    while let Some((_, after)) = rest.split_once("{{@") {
        let Some((name, tail)) = after.split_once("}}") else {
            break;
        };
        if valid_name(name) && !names.iter().any(|n| n == name) {
            names.push(name.to_owned());
        }
        rest = tail;
    }
    names
}

pub fn replace(
    text: &str,
    mut lookup: impl FnMut(&str) -> Result<String, String>,
) -> Result<String, String> {
    let mut error = None;
    let normalized =
        encoded_references().replace_all(text, |caps: &regex::Captures| match lookup(&caps[1]) {
            Ok(value) => url::form_urlencoded::byte_serialize(value.as_bytes())
                .collect::<String>()
                .replace('+', "%20"),
            Err(message) => {
                error = Some(message);
                String::new()
            }
        });
    if let Some(error) = error {
        return Err(error);
    }
    let mut rest = normalized.as_ref();
    let mut out = String::new();
    while let Some((before, after)) = rest.split_once("{{@") {
        out.push_str(before);
        let (name, tail) = after.split_once("}}").ok_or("Invalid secret reference.")?;
        if !valid_name(name) {
            return Err(
                "Invalid secret name. Use letters, numbers, dots, dashes or underscores.".into(),
            );
        }
        out.push_str(&lookup(name)?);
        rest = tail;
    }
    out.push_str(rest);
    Ok(out)
}

/// Basic auth is encoded by draft preparation. Decode before resolving secrets.
pub fn resolve_request(
    request: &mut RequestInput,
    mut lookup: impl FnMut(&str) -> Result<String, String>,
) -> Result<(), String> {
    request.url = replace(&request.url, &mut lookup)?;
    for header in &mut request.headers {
        if header.key.eq_ignore_ascii_case("authorization") && header.value.starts_with("Basic ") {
            let encoded = &header.value[6..];
            if let Ok(decoded) = STANDARD.decode(encoded)
                && let Ok(decoded) = String::from_utf8(decoded)
            {
                header.value =
                    format!("Basic {}", STANDARD.encode(replace(&decoded, &mut lookup)?));
                continue;
            }
        }
        header.value = replace(&header.value, &mut lookup)?;
    }
    if let Some(body) = &mut request.body {
        *body = replace(body, &mut lookup)?;
    }
    for part in request.multipart.iter_mut().flatten() {
        part.key = replace(&part.key, &mut lookup)?;
        if !part.file {
            part.value = replace(&part.value, &mut lookup)?;
        }
    }
    Ok(())
}

pub fn identity_key(url: &str) -> Result<String, String> {
    let url = url::Url::parse(url).map_err(|_| "Enter an HTTPS origin.")?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.host_str().is_none()
    {
        return Err("Client certificates require an HTTPS origin without credentials.".into());
    }
    Ok(format!("mtls:{}", url.origin().ascii_serialization()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn secret_references_are_literal_and_scoped() {
        assert_eq!(
            replace("Bearer {{@token}}", |_| Ok("{{@other}}".into())).unwrap(),
            "Bearer {{@other}}"
        );
        assert!(replace("{{@bad name}}", |_| panic!()).is_err());
        assert_ne!(service(None), service(Some("/project")));
    }
    #[test]
    fn basic_credentials_resolve_after_encoding() {
        let mut req = RequestInput {
            headers: vec![crate::model::Header {
                key: "Authorization".into(),
                value: format!("Basic {}", STANDARD.encode("user:{{@password}}")),
            }],
            ..Default::default()
        };
        resolve_request(&mut req, |_| Ok("secret".into())).unwrap();
        assert_eq!(
            req.headers[0].value,
            format!("Basic {}", STANDARD.encode("user:secret"))
        );
    }
    #[test]
    fn encoded_query_and_form_secrets_stay_encoded() {
        let source = "key=%7B%7B%40api%7D%7D";
        assert_eq!(references(source), vec!["api"]);
        assert_eq!(
            replace(source, |_| Ok("a&b c".into())).unwrap(),
            "key=a%26b%20c"
        );
    }
    #[test]
    fn identity_is_exact_origin() {
        assert_eq!(
            identity_key("https://EXAMPLE.com/a").unwrap(),
            "mtls:https://example.com"
        );
        assert_ne!(
            identity_key("https://example.com:8443").unwrap(),
            identity_key("https://example.com").unwrap()
        );
        assert!(identity_key("http://example.com").is_err());
    }
}
