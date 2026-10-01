//! Port of `src/lib/response-content.ts`: the syntax language for a content
//! type. Highlighting itself (highlight.js HTML in the TS) belongs to the UI.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResponseLanguage {
    Bash,
    Css,
    Graphql,
    Html,
    Ini,
    Javascript,
    Json,
    Markdown,
    Plaintext,
    Sql,
    Xml,
    Yaml,
}

impl ResponseLanguage {
    /// The highlight.js language name.
    pub fn id(self) -> &'static str {
        match self {
            ResponseLanguage::Bash => "bash",
            ResponseLanguage::Css => "css",
            ResponseLanguage::Graphql => "graphql",
            ResponseLanguage::Html => "html",
            ResponseLanguage::Ini => "ini",
            ResponseLanguage::Javascript => "javascript",
            ResponseLanguage::Json => "json",
            ResponseLanguage::Markdown => "markdown",
            ResponseLanguage::Plaintext => "plaintext",
            ResponseLanguage::Sql => "sql",
            ResponseLanguage::Xml => "xml",
            ResponseLanguage::Yaml => "yaml",
        }
    }
}

fn normalized_content_type(content_type: &str) -> String {
    content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_lowercase()
}

pub fn response_language(content_type: &str) -> ResponseLanguage {
    use ResponseLanguage::*;
    let kind = normalized_content_type(content_type);
    let kind = kind.as_str();
    if kind == "application/json" || kind.ends_with("+json") {
        return Json;
    }
    if matches!(kind, "text/html" | "application/xhtml+xml") {
        return Html;
    }
    if matches!(kind, "application/xml" | "text/xml") || kind.ends_with("+xml") {
        return Xml;
    }
    match kind {
        "text/css" => Css,
        "application/javascript" | "application/ecmascript" | "text/javascript" => Javascript,
        "application/yaml" | "application/x-yaml" | "text/yaml" => Yaml,
        "application/graphql" | "application/x-graphql" => Graphql,
        "application/sql" | "text/x-sql" => Sql,
        "application/x-sh" | "text/x-shellscript" => Bash,
        "text/markdown" => Markdown,
        "application/x-www-form-urlencoded" | "text/ini" | "application/toml" => Ini,
        _ => Plaintext,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_common_http_content_types_to_syntax_languages() {
        use ResponseLanguage::*;
        assert_eq!(response_language("application/problem+json"), Json);
        assert_eq!(response_language("application/xml"), Xml);
        assert_eq!(response_language("text/html; charset=utf-8"), Html);
        assert_eq!(response_language("application/xhtml+xml"), Html);
        assert_eq!(response_language("image/svg+xml"), Xml);
        assert_eq!(response_language("text/css"), Css);
        assert_eq!(response_language("application/javascript"), Javascript);
        assert_eq!(response_language("application/yaml"), Yaml);
        assert_eq!(response_language("text/plain"), Plaintext);
    }
}
