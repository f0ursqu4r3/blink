//! GraphQL schemas from introspection, and completions from them. Port of
//! `src/lib/graphql-schema.ts`; the schema model replaces graphql-js
//! `buildClientSchema`, and `graphql_completions` covers the editor hints.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::future::Future;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::engine::Engine;
use crate::model::{ApiResponse, BodyMode, Draft, RequestInput, TransportOptions};
use crate::request::{RequestContext, build_request};

const NOT_INTROSPECTION: &str =
    "Endpoint did not return an introspection result. Introspection may be disabled.";

/// graphql-js `getIntrospectionQuery()` with default options.
pub const INTROSPECTION_QUERY: &str = "\n    query IntrospectionQuery {\n      __schema {\n        \n        queryType { name kind }\n        mutationType { name kind }\n        subscriptionType { name kind }\n        types {\n          ...FullType\n        }\n        directives {\n          name\n          description\n          \n          \n          \n          locations\n          args {\n            ...InputValue\n          }\n        }\n      }\n    }\n\n    fragment FullType on __Type {\n      kind\n      name\n      description\n      \n      \n      fields(includeDeprecated: true) {\n        name\n        description\n        args {\n          ...InputValue\n        }\n        type {\n          ...TypeRef\n        }\n        isDeprecated\n        deprecationReason\n      }\n      inputFields {\n        ...InputValue\n      }\n      interfaces {\n        ...TypeRef\n      }\n      enumValues(includeDeprecated: true) {\n        name\n        description\n        isDeprecated\n        deprecationReason\n      }\n      possibleTypes {\n        ...TypeRef\n      }\n    }\n\n    fragment InputValue on __InputValue {\n      name\n      description\n      type { ...TypeRef }\n      defaultValue\n      \n      \n    }\n\n    fragment TypeRef on __Type {\n      kind\n      name\n      ofType {\n        name\n        kind\n        ofType {\n          name\n          kind\n          ofType {\n            name\n            kind\n            ofType {\n              name\n              kind\n              ofType {\n                name\n                kind\n                ofType {\n                  name\n                  kind\n                  ofType {\n                    name\n                    kind\n                    ofType {\n                      name\n                      kind\n                      ofType {\n                        name\n                        kind\n                      }\n                    }\n                  }\n                }\n              }\n            }\n          }\n        }\n      }\n    }\n  ";

// ── Schema model ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeKind {
    Scalar,
    Object,
    Interface,
    Union,
    Enum,
    InputObject,
}

/// A type reference such as `[User!]!`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeRef {
    Named(String),
    List(Box<TypeRef>),
    NonNull(Box<TypeRef>),
}

impl TypeRef {
    /// The named type under any list and non-null wrappers.
    pub fn named(&self) -> &str {
        match self {
            TypeRef::Named(name) => name,
            TypeRef::List(inner) | TypeRef::NonNull(inner) => inner.named(),
        }
    }
}

impl fmt::Display for TypeRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeRef::Named(name) => f.write_str(name),
            TypeRef::List(inner) => write!(f, "[{inner}]"),
            TypeRef::NonNull(inner) => write!(f, "{inner}!"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputValue {
    pub name: String,
    pub description: Option<String>,
    pub type_ref: TypeRef,
    /// GraphQL source text of the default value.
    pub default_value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    pub description: Option<String>,
    pub args: Vec<InputValue>,
    pub type_ref: TypeRef,
    pub deprecation_reason: Option<String>,
    pub is_deprecated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumValue {
    pub name: String,
    pub description: Option<String>,
    pub is_deprecated: bool,
    pub deprecation_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedType {
    pub name: String,
    pub kind: TypeKind,
    pub description: Option<String>,
    /// Objects and interfaces.
    pub fields: Vec<Field>,
    /// Objects and interfaces.
    pub interfaces: Vec<String>,
    /// Unions.
    pub possible_types: Vec<String>,
    pub enum_values: Vec<EnumValue>,
    pub input_fields: Vec<InputValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Directive {
    pub name: String,
    pub description: Option<String>,
    pub locations: Vec<String>,
    pub args: Vec<InputValue>,
}

/// A client schema built from an introspection result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphqlSchema {
    pub query_type: Option<String>,
    pub mutation_type: Option<String>,
    pub subscription_type: Option<String>,
    /// In introspection order.
    pub types: Vec<NamedType>,
    pub directives: Vec<Directive>,
    by_name: HashMap<String, usize>,
}

impl GraphqlSchema {
    pub fn get_type(&self, name: &str) -> Option<&NamedType> {
        self.by_name.get(name).map(|index| &self.types[*index])
    }

    pub fn get_query_type(&self) -> Option<&NamedType> {
        self.query_type
            .as_deref()
            .and_then(|name| self.get_type(name))
    }

    pub fn get_mutation_type(&self) -> Option<&NamedType> {
        self.mutation_type
            .as_deref()
            .and_then(|name| self.get_type(name))
    }

    pub fn get_subscription_type(&self) -> Option<&NamedType> {
        self.subscription_type
            .as_deref()
            .and_then(|name| self.get_type(name))
    }

    /// A field of an object or interface, including `__typename`, and
    /// `__schema` and `__type` on the query type.
    pub fn field(&self, type_name: &str, field_name: &str) -> Option<Field> {
        if let Some(field) = self
            .get_type(type_name)?
            .fields
            .iter()
            .find(|field| field.name == field_name)
        {
            return Some(field.clone());
        }
        meta_fields(self, type_name)
            .into_iter()
            .find(|field| field.name == field_name)
    }
}

fn meta_field(name: &str, type_ref: TypeRef, description: &str, args: Vec<InputValue>) -> Field {
    Field {
        name: name.into(),
        description: Some(description.into()),
        args,
        type_ref,
        deprecation_reason: None,
        is_deprecated: false,
    }
}

fn non_null(name: &str) -> TypeRef {
    TypeRef::NonNull(Box::new(TypeRef::Named(name.into())))
}

/// Fields every selection may use but introspection does not list.
fn meta_fields(schema: &GraphqlSchema, type_name: &str) -> Vec<Field> {
    let mut fields = vec![meta_field(
        "__typename",
        non_null("String"),
        "The name of the current Object type at runtime.",
        vec![],
    )];
    if schema.query_type.as_deref() == Some(type_name) {
        fields.push(meta_field(
            "__schema",
            non_null("__Schema"),
            "Access the current type schema of this server.",
            vec![],
        ));
        fields.push(meta_field(
            "__type",
            TypeRef::Named("__Type".into()),
            "Request the type information of a single type.",
            vec![InputValue {
                name: "name".into(),
                description: None,
                type_ref: non_null("String"),
                default_value: None,
            }],
        ));
    }
    fields
}

// ── Introspection ───────────────────────────────────────────────────────────

type Obj = serde_json::Map<String, Value>;

fn as_object(value: &Value) -> Result<&Obj, ()> {
    value.as_object().ok_or(())
}

fn list<'a>(object: &'a Obj, key: &str) -> Result<&'a Vec<Value>, ()> {
    object.get(key).and_then(Value::as_array).ok_or(())
}

fn optional_string(object: &Obj, key: &str) -> Option<String> {
    object.get(key).and_then(Value::as_str).map(String::from)
}

fn name_of(object: &Obj) -> Result<String, ()> {
    optional_string(object, "name").ok_or(())
}

fn type_ref(value: &Value, depth: usize) -> Result<TypeRef, ()> {
    let object = as_object(value)?;
    if depth > 100 {
        return Err(());
    }
    match object.get("kind").and_then(Value::as_str) {
        Some("LIST") => Ok(TypeRef::List(Box::new(type_ref(
            object.get("ofType").ok_or(())?,
            depth + 1,
        )?))),
        Some("NON_NULL") => {
            let inner = type_ref(object.get("ofType").ok_or(())?, depth + 1)?;
            if matches!(inner, TypeRef::NonNull(_)) {
                return Err(());
            }
            Ok(TypeRef::NonNull(Box::new(inner)))
        }
        Some(_) => Ok(TypeRef::Named(name_of(object)?)),
        None => Err(()),
    }
}

fn input_values(object: &Obj, key: &str) -> Result<Vec<InputValue>, ()> {
    list(object, key)?
        .iter()
        .map(|value| {
            let value = as_object(value)?;
            Ok(InputValue {
                name: name_of(value)?,
                description: optional_string(value, "description"),
                type_ref: type_ref(value.get("type").ok_or(())?, 0)?,
                default_value: optional_string(value, "defaultValue"),
            })
        })
        .collect()
}

fn type_kind(kind: &str) -> Option<TypeKind> {
    Some(match kind {
        "SCALAR" => TypeKind::Scalar,
        "OBJECT" => TypeKind::Object,
        "INTERFACE" => TypeKind::Interface,
        "UNION" => TypeKind::Union,
        "ENUM" => TypeKind::Enum,
        "INPUT_OBJECT" => TypeKind::InputObject,
        _ => return None,
    })
}

fn named_type(value: &Value) -> Result<NamedType, ()> {
    let object = as_object(value)?;
    let kind = object
        .get("kind")
        .and_then(Value::as_str)
        .and_then(type_kind)
        .ok_or(())?;
    let mut result = NamedType {
        name: name_of(object)?,
        kind,
        description: optional_string(object, "description"),
        fields: Vec::new(),
        interfaces: Vec::new(),
        possible_types: Vec::new(),
        enum_values: Vec::new(),
        input_fields: Vec::new(),
    };
    let names = |key: &str| -> Result<Vec<String>, ()> {
        list(object, key)?
            .iter()
            .map(|value| name_of(as_object(value)?))
            .collect()
    };
    match kind {
        TypeKind::Object | TypeKind::Interface => {
            // Interfaces from older servers may omit their own interfaces.
            result.interfaces = match object.get("interfaces") {
                Some(Value::Null) | None if kind == TypeKind::Interface => Vec::new(),
                _ => names("interfaces")?,
            };
            result.fields = list(object, "fields")?
                .iter()
                .map(|value| {
                    let field = as_object(value)?;
                    Ok(Field {
                        name: name_of(field)?,
                        description: optional_string(field, "description"),
                        args: input_values(field, "args")?,
                        type_ref: type_ref(field.get("type").ok_or(())?, 0)?,
                        deprecation_reason: optional_string(field, "deprecationReason"),
                        is_deprecated: field.get("isDeprecated") == Some(&Value::Bool(true)),
                    })
                })
                .collect::<Result<_, ()>>()?;
        }
        TypeKind::Union => result.possible_types = names("possibleTypes")?,
        TypeKind::Enum => {
            result.enum_values = list(object, "enumValues")?
                .iter()
                .map(|value| {
                    let value = as_object(value)?;
                    Ok(EnumValue {
                        name: name_of(value)?,
                        description: optional_string(value, "description"),
                        is_deprecated: value.get("isDeprecated") == Some(&Value::Bool(true)),
                        deprecation_reason: optional_string(value, "deprecationReason"),
                    })
                })
                .collect::<Result<_, ()>>()?;
        }
        TypeKind::InputObject => result.input_fields = input_values(object, "inputFields")?,
        TypeKind::Scalar => {}
    }
    Ok(result)
}

/// Build a client schema from `data.__schema`, as `buildClientSchema` does.
pub fn build_client_schema(data: &Value) -> Result<GraphqlSchema, String> {
    build(data).map_err(|_| NOT_INTROSPECTION.to_string())
}

fn build(data: &Value) -> Result<GraphqlSchema, ()> {
    let schema = as_object(as_object(data)?.get("__schema").ok_or(())?)?;
    let types: Vec<NamedType> = list(schema, "types")?
        .iter()
        .map(named_type)
        .collect::<Result<_, ()>>()?;
    let mut by_name = HashMap::new();
    for (index, named) in types.iter().enumerate() {
        by_name.insert(named.name.clone(), index);
    }
    let directives = match schema.get("directives") {
        None | Some(Value::Null) => Vec::new(),
        Some(_) => list(schema, "directives")?
            .iter()
            .map(|value| {
                let directive = as_object(value)?;
                Ok(Directive {
                    name: name_of(directive)?,
                    description: optional_string(directive, "description"),
                    locations: list(directive, "locations")?
                        .iter()
                        .map(|location| location.as_str().map(String::from).ok_or(()))
                        .collect::<Result<_, ()>>()?,
                    args: input_values(directive, "args")?,
                })
            })
            .collect::<Result<_, ()>>()?,
    };
    let root = |key: &str| -> Result<Option<String>, ()> {
        match schema.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(value) => Ok(Some(name_of(as_object(value)?)?)),
        }
    };
    let result = GraphqlSchema {
        query_type: root("queryType")?,
        mutation_type: root("mutationType")?,
        subscription_type: root("subscriptionType")?,
        types,
        directives,
        by_name,
    };
    validate(&result)?;
    Ok(result)
}

/// Every reference names a type of the right kind, as graphql-js checks.
fn validate(schema: &GraphqlSchema) -> Result<(), ()> {
    let kind_of = |name: &str| schema.get_type(name).map(|named| named.kind).ok_or(());
    let output = |type_ref: &TypeRef| {
        kind_of(type_ref.named())
            .and_then(|kind| (kind != TypeKind::InputObject).then_some(()).ok_or(()))
    };
    let input = |type_ref: &TypeRef| {
        kind_of(type_ref.named()).and_then(|kind| {
            matches!(
                kind,
                TypeKind::Scalar | TypeKind::Enum | TypeKind::InputObject
            )
            .then_some(())
            .ok_or(())
        })
    };
    for root in [
        &schema.query_type,
        &schema.mutation_type,
        &schema.subscription_type,
    ]
    .into_iter()
    .flatten()
    {
        if kind_of(root)? != TypeKind::Object {
            return Err(());
        }
    }
    for named in &schema.types {
        for field in &named.fields {
            output(&field.type_ref)?;
            field.args.iter().try_for_each(|arg| input(&arg.type_ref))?;
        }
        for interface in &named.interfaces {
            if kind_of(interface)? != TypeKind::Interface {
                return Err(());
            }
        }
        for member in &named.possible_types {
            if kind_of(member)? != TypeKind::Object {
                return Err(());
            }
        }
        named
            .input_fields
            .iter()
            .try_for_each(|field| input(&field.type_ref))?;
    }
    for directive in &schema.directives {
        directive
            .args
            .iter()
            .try_for_each(|arg| input(&arg.type_ref))?;
    }
    Ok(())
}

fn parse_introspection(body: &str) -> Result<GraphqlSchema, String> {
    let Ok(payload) = serde_json::from_str::<Value>(body) else {
        return Err(NOT_INTROSPECTION.into());
    };
    let data = payload.get("data").filter(|data| !data.is_null());
    let has_schema = data
        .and_then(|data| data.get("__schema"))
        .is_some_and(truthy);
    if !has_schema {
        let message = payload
            .get("errors")
            .and_then(Value::as_array)
            .and_then(|errors| errors.first())
            .and_then(|error| error.get("message"))
            .and_then(Value::as_str);
        return Err(message.unwrap_or(NOT_INTROSPECTION).to_string());
    }
    build_client_schema(data.unwrap_or(&Value::Null))
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Number(number) => number.as_f64().is_some_and(|n| n != 0.0),
        Value::String(text) => !text.is_empty(),
        _ => true,
    }
}

// ── Fetch and cache ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CachedSchema {
    pub schema: Arc<GraphqlSchema>,
    /// Epoch milliseconds.
    pub fetched_at: f64,
}

/// In memory only, keyed by resolved URL. Schemas are refetched after restart.
static CACHE: LazyLock<Mutex<HashMap<String, CachedSchema>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn introspection_draft(draft: &Draft, body: &str) -> Draft {
    Draft {
        method: "POST".into(),
        body_mode: BodyMode::Graphql,
        body: body.into(),
        variables: Some(String::new()),
        ..draft.clone()
    }
}

pub fn schema_key(draft: &Draft, ctx: Option<RequestContext>) -> Option<String> {
    build_request(&introspection_draft(draft, "{__typename}"), ctx)
        .ok()
        .map(|request| request.url)
}

pub fn get_cached_schema(key: &str) -> Option<CachedSchema> {
    CACHE.lock().ok()?.get(key).cloned()
}

pub fn clear_schema_cache() {
    if let Ok(mut cache) = CACHE.lock() {
        cache.clear();
    }
}

fn now_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |elapsed| elapsed.as_millis() as f64)
}

/// The introspection request for a draft: its URL, headers, and auth.
pub fn introspection_request(
    draft: &Draft,
    ctx: Option<RequestContext>,
) -> Result<RequestInput, String> {
    build_request(&introspection_draft(draft, INTROSPECTION_QUERY), ctx)
}

/// Fetch and cache a schema. `send` sends the request; `release` frees the
/// stored response body once it is read.
pub async fn fetch_schema_with<S, F>(
    draft: &Draft,
    ctx: Option<RequestContext<'_>>,
    send: S,
    release: impl FnOnce(&ApiResponse),
) -> Result<Arc<GraphqlSchema>, String>
where
    S: FnOnce(RequestInput) -> F,
    F: Future<Output = Result<ApiResponse, String>>,
{
    let request = introspection_request(draft, ctx)?;
    let url = request.url.clone();
    let response = send(request).await?;
    let result = schema_from_response(&response);
    release(&response);
    let schema = Arc::new(result?);
    if let Ok(mut cache) = CACHE.lock() {
        cache.insert(
            url,
            CachedSchema {
                schema: schema.clone(),
                fetched_at: now_ms(),
            },
        );
    }
    Ok(schema)
}

fn schema_from_response(response: &ApiResponse) -> Result<GraphqlSchema, String> {
    if response.status < 200 || response.status >= 300 {
        return Err(format!(
            "Schema request failed: {} {}",
            response.status, response.status_text
        )
        .trim()
        .to_string());
    }
    if response.is_truncated() {
        return Err("Schema response exceeds the inspection limit.".into());
    }
    parse_introspection(&response.body)
}

/// Fetch and cache a schema through the engine.
pub async fn fetch_schema(
    engine: &Engine,
    draft: &Draft,
    ctx: Option<RequestContext<'_>>,
    options: &TransportOptions,
) -> Result<Arc<GraphqlSchema>, String> {
    let id = engine.next_id("schema");
    let options = options.clone();
    fetch_schema_with(
        draft,
        ctx,
        |request| engine.send_request(request, options, id, None),
        |response| {
            if let Some(body_id) = &response.body_id {
                engine.release_response(body_id);
            }
        },
    )
    .await
}

pub fn format_schema_age(fetched_at: f64, now: f64) -> String {
    let minutes = ((now - fetched_at) / 60_000.0).floor();
    if minutes < 1.0 {
        return "just now".into();
    }
    if minutes < 60.0 {
        return format!("{minutes}m ago");
    }
    format!("{}h ago", (minutes / 60.0).floor())
}

// ── Completions ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionKind {
    Keyword,
    Field,
    Argument,
    Type,
    EnumValue,
    Directive,
}

/// One editor suggestion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionItem {
    pub label: String,
    pub kind: CompletionKind,
    /// A type such as `[User!]!`, or empty.
    pub detail: String,
    pub documentation: Option<String>,
    pub deprecated: bool,
}

/// Suggestions at `offset` (a byte offset) and the byte offset where the
/// word being typed starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completions {
    pub from: usize,
    pub items: Vec<CompletionItem>,
}

#[derive(Debug, Clone, PartialEq)]
enum Word {
    Name(String),
    Punct(char),
    Spread,
    Value,
}

/// A tolerant scan: comments and strings are skipped, invalid text is
/// ignored. Also returns whether the text ends inside a comment or string.
fn scan(text: &str) -> (Vec<Word>, bool) {
    let chars: Vec<char> = text.chars().collect();
    let mut words = Vec::new();
    let mut open = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        open = false;
        if c == '#' {
            while i < chars.len() && chars[i] != '\n' && chars[i] != '\r' {
                i += 1;
            }
            open = true;
        } else if c == '"' {
            let block = chars.get(i + 1) == Some(&'"') && chars.get(i + 2) == Some(&'"');
            i += if block { 3 } else { 1 };
            open = true;
            while i < chars.len() {
                if block && chars[i..].starts_with(&['"', '"', '"']) {
                    i += 3;
                    open = false;
                    break;
                }
                if !block && (chars[i] == '"' || chars[i] == '\n') {
                    i += 1;
                    open = false;
                    break;
                }
                i += if chars[i] == '\\' { 2 } else { 1 };
            }
            words.push(Word::Value);
        } else if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            words.push(Word::Name(chars[start..i].iter().collect()));
        } else if c.is_ascii_digit() || c == '-' {
            while i < chars.len()
                && (chars[i].is_ascii_alphanumeric() || matches!(chars[i], '.' | '-' | '+'))
            {
                i += 1;
            }
            words.push(Word::Value);
        } else if chars[i..].starts_with(&['.', '.', '.']) {
            words.push(Word::Spread);
            i += 3;
        } else {
            if "{}()[]:@$=!|&".contains(c) {
                words.push(Word::Punct(c));
            }
            i += 1;
        }
    }
    (words, open)
}

#[derive(Debug, Clone)]
enum Frame {
    /// A selection set on this type; None when the type is unknown.
    Selection(Option<String>),
    /// Arguments of a field (type, field) or of a directive.
    Arguments(Option<(String, String)>, Option<String>),
    /// Variable definitions, or any other parenthesis.
    Parens,
    /// An object or list value.
    Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Expect {
    /// Top level, before an operation or fragment.
    Definition,
    /// After `on` in a fragment or inline fragment.
    TypeCondition,
    /// After `@`.
    Directive,
    /// A selection name, argument name, or other name.
    Name,
    /// An argument value.
    ArgumentValue,
    Other,
}

struct Walker<'a> {
    schema: &'a GraphqlSchema,
    stack: Vec<Frame>,
    /// Root type for the next `{` at the top level.
    pending_root: Option<String>,
    /// Type condition waiting for its `{`.
    pending_condition: Option<String>,
    /// The last field name in the current selection set and its type.
    last_field: Option<(String, String)>,
    last_directive: Option<String>,
    /// The argument whose value comes next: its type.
    argument_type: Option<TypeRef>,
    expect: Expect,
}

impl<'a> Walker<'a> {
    fn walk(&mut self, words: &[Word]) {
        let mut index = 0;
        while index < words.len() {
            let word = &words[index];
            let top = self.stack.last().cloned();
            match word {
                Word::Name(name) => match self.expect {
                    Expect::Definition => {
                        self.pending_root = match name.as_str() {
                            "query" => self.schema.query_type.clone(),
                            "mutation" => self.schema.mutation_type.clone(),
                            "subscription" => self.schema.subscription_type.clone(),
                            _ => self.pending_root.take(),
                        };
                        if name == "on" {
                            self.expect = Expect::TypeCondition;
                        } else if name == "fragment" {
                            self.pending_root = None;
                            self.expect = Expect::Other;
                        } else if !matches!(name.as_str(), "query" | "mutation" | "subscription") {
                            self.expect = Expect::Other;
                        }
                    }
                    Expect::TypeCondition => {
                        self.pending_condition = Some(name.clone());
                        self.expect = Expect::Other;
                    }
                    Expect::Directive => {
                        self.last_directive = Some(name.clone());
                        self.last_field = None;
                        self.expect = Expect::Other;
                    }
                    Expect::ArgumentValue => self.expect = Expect::Name,
                    _ if name == "on"
                        && (self.stack.is_empty()
                            || index.checked_sub(1).and_then(|at| words.get(at))
                                == Some(&Word::Spread)) =>
                    {
                        self.expect = Expect::TypeCondition;
                    }
                    _ => {
                        if let Some(Frame::Selection(Some(type_name))) = &top {
                            // A field; `alias: name` replaces the alias.
                            if words.get(index + 1) != Some(&Word::Punct(':')) {
                                self.last_field = Some((type_name.clone(), name.clone()));
                                self.last_directive = None;
                            }
                        } else if let Some(Frame::Arguments(field, directive)) = &top {
                            self.argument_type =
                                self.argument(field.as_ref(), directive.as_deref(), name);
                        }
                        if matches!(top, Some(Frame::Selection(_))) {
                            self.expect = Expect::Name;
                        }
                    }
                },
                Word::Punct('{') => {
                    let next = if let Some(condition) = self.pending_condition.take() {
                        Frame::Selection(Some(condition))
                    } else if self.stack.is_empty() {
                        let root = self
                            .pending_root
                            .take()
                            .or_else(|| self.schema.query_type.clone());
                        Frame::Selection(root)
                    } else if matches!(top, Some(Frame::Selection(_))) {
                        let field = self.last_field.take();
                        Frame::Selection(field.and_then(|(type_name, name)| {
                            Some(
                                self.schema
                                    .field(&type_name, &name)?
                                    .type_ref
                                    .named()
                                    .to_string(),
                            )
                        }))
                    } else {
                        Frame::Value
                    };
                    self.stack.push(next);
                    self.expect = Expect::Name;
                }
                Word::Punct('}') => {
                    self.stack.pop();
                    self.last_field = None;
                    self.expect = if self.stack.is_empty() {
                        Expect::Definition
                    } else {
                        Expect::Name
                    };
                }
                Word::Punct('(') => {
                    let frame = if let Some(directive) = self.last_directive.take() {
                        Frame::Arguments(None, Some(directive))
                    } else if matches!(top, Some(Frame::Selection(_))) {
                        Frame::Arguments(self.last_field.clone(), None)
                    } else {
                        Frame::Parens
                    };
                    self.stack.push(frame);
                    self.expect = Expect::Name;
                }
                Word::Punct(')') => {
                    self.stack.pop();
                    self.expect = Expect::Other;
                    if matches!(self.stack.last(), Some(Frame::Selection(_))) {
                        self.expect = Expect::Name;
                    }
                }
                Word::Punct('[') => {
                    self.stack.push(Frame::Value);
                }
                Word::Punct(']') => {
                    self.stack.pop();
                    self.expect = Expect::Name;
                }
                Word::Punct(':') => {
                    if matches!(top, Some(Frame::Arguments(..))) {
                        self.expect = Expect::ArgumentValue;
                    }
                }
                Word::Punct('@') => self.expect = Expect::Directive,
                Word::Spread => {
                    self.last_field = None;
                    self.expect = Expect::Other;
                }
                Word::Value | Word::Punct(_) => {
                    if self.expect == Expect::ArgumentValue {
                        self.expect = Expect::Name;
                    }
                }
            }
            index += 1;
        }
    }

    fn argument(
        &self,
        field: Option<&(String, String)>,
        directive: Option<&str>,
        name: &str,
    ) -> Option<TypeRef> {
        let args = self.arguments(field, directive);
        args.into_iter()
            .find(|arg| arg.name == name)
            .map(|arg| arg.type_ref)
    }

    fn arguments(
        &self,
        field: Option<&(String, String)>,
        directive: Option<&str>,
    ) -> Vec<InputValue> {
        if let Some(directive) = directive {
            return self
                .schema
                .directives
                .iter()
                .find(|d| d.name == directive)
                .map(|d| d.args.clone())
                .unwrap_or_default();
        }
        field
            .and_then(|(type_name, name)| self.schema.field(type_name, name))
            .map(|field| field.args)
            .unwrap_or_default()
    }
}

fn keyword(label: &str) -> CompletionItem {
    CompletionItem {
        label: label.into(),
        kind: CompletionKind::Keyword,
        detail: String::new(),
        documentation: None,
        deprecated: false,
    }
}

fn field_item(field: &Field) -> CompletionItem {
    CompletionItem {
        label: field.name.clone(),
        kind: CompletionKind::Field,
        detail: field.type_ref.to_string(),
        documentation: field.description.clone(),
        deprecated: field.is_deprecated,
    }
}

fn argument_item(arg: &InputValue) -> CompletionItem {
    CompletionItem {
        label: arg.name.clone(),
        kind: CompletionKind::Argument,
        detail: arg.type_ref.to_string(),
        documentation: arg.description.clone(),
        deprecated: false,
    }
}

/// Suggestions for the GraphQL document `text` with the cursor at `offset`.
pub fn graphql_completions(schema: &GraphqlSchema, text: &str, offset: usize) -> Completions {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    let before = &text[..offset];
    let from = before
        .char_indices()
        .rev()
        .take_while(|(_, c)| c.is_ascii_alphanumeric() || *c == '_')
        .last()
        .map_or(offset, |(at, _)| at);
    let prefix = &text[from..offset];
    let mut walker = Walker {
        schema,
        stack: Vec::new(),
        pending_root: None,
        pending_condition: None,
        last_field: None,
        last_directive: None,
        argument_type: None,
        expect: Expect::Definition,
    };
    let (words, open) = scan(&text[..from]);
    walker.walk(&words);
    let mut items: Vec<CompletionItem> = Vec::new();
    // No suggestions inside a comment or a string.
    if !open {
        match walker.expect {
            Expect::Definition => {
                items.extend(["query", "mutation", "subscription", "fragment"].map(keyword));
            }
            Expect::TypeCondition => {
                items.extend(
                    schema
                        .types
                        .iter()
                        .filter(|named| {
                            matches!(
                                named.kind,
                                TypeKind::Object | TypeKind::Interface | TypeKind::Union
                            ) && !named.name.starts_with("__")
                        })
                        .map(|named| CompletionItem {
                            label: named.name.clone(),
                            kind: CompletionKind::Type,
                            detail: String::new(),
                            documentation: named.description.clone(),
                            deprecated: false,
                        }),
                );
            }
            Expect::Directive => {
                items.extend(schema.directives.iter().map(|directive| CompletionItem {
                    label: directive.name.clone(),
                    kind: CompletionKind::Directive,
                    detail: String::new(),
                    documentation: directive.description.clone(),
                    deprecated: false,
                }));
            }
            Expect::ArgumentValue => {
                let named = walker
                    .argument_type
                    .as_ref()
                    .and_then(|type_ref| schema.get_type(type_ref.named()));
                if let Some(named) = named {
                    if named.kind == TypeKind::Enum {
                        items.extend(named.enum_values.iter().map(|value| CompletionItem {
                            label: value.name.clone(),
                            kind: CompletionKind::EnumValue,
                            detail: named.name.clone(),
                            documentation: value.description.clone(),
                            deprecated: value.is_deprecated,
                        }));
                    } else if named.name == "Boolean" {
                        items.extend(["true", "false"].map(keyword));
                    }
                }
            }
            Expect::Name | Expect::Other => match walker.stack.last() {
                Some(Frame::Selection(Some(type_name))) if walker.expect == Expect::Name => {
                    if let Some(named) = schema.get_type(type_name) {
                        items.extend(named.fields.iter().map(field_item));
                    }
                    items.extend(meta_fields(schema, type_name).iter().map(field_item));
                }
                Some(Frame::Arguments(field, directive)) if walker.expect == Expect::Name => {
                    items.extend(
                        walker
                            .arguments(field.as_ref(), directive.as_deref())
                            .iter()
                            .map(argument_item),
                    );
                }
                _ => {}
            },
        }
    }
    let lower = prefix.to_ascii_lowercase();
    items.retain(|item| item.label.to_ascii_lowercase().starts_with(&lower));
    let mut seen = HashSet::new();
    items.retain(|item| seen.insert(item.label.clone()));
    Completions { from, items }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authorization::ResolvedRequestContext;
    use crate::interpolation::InterpolationContext;
    use crate::model::{AuthorizationConfig, Definitions, Header};
    use crate::request::{create_draft, pair};
    use futures::executor::block_on;
    use serde_json::json;
    use std::cell::{Cell, RefCell};

    // The cache is process-wide; tests that use it run one at a time.
    static SERIAL: Mutex<()> = Mutex::new(());

    const URL: &str = "https://api.test/graphql";

    fn type_ref(kind: &str, name: Option<&str>, of: Option<Value>) -> Value {
        json!({ "kind": kind, "name": name, "ofType": of })
    }

    /// `introspectionFromSchema(buildSchema('type Query { viewer: User } type User { id: ID! }'))`, trimmed.
    fn introspection() -> Value {
        let scalar = |name: &str| {
            json!({ "kind": "SCALAR", "name": name, "description": null, "fields": null,
                "inputFields": null, "interfaces": null, "enumValues": null, "possibleTypes": null })
        };
        json!({
            "__schema": {
                "queryType": { "name": "Query", "kind": "OBJECT" },
                "mutationType": null,
                "subscriptionType": null,
                "types": [
                    {
                        "kind": "OBJECT", "name": "Query", "description": null,
                        "fields": [{ "name": "viewer", "description": null, "args": [],
                            "type": type_ref("OBJECT", Some("User"), None),
                            "isDeprecated": false, "deprecationReason": null }],
                        "inputFields": null, "interfaces": [], "enumValues": null, "possibleTypes": null
                    },
                    {
                        "kind": "OBJECT", "name": "User", "description": "A person.",
                        "fields": [
                            { "name": "id", "description": null, "args": [],
                              "type": type_ref("NON_NULL", None, Some(type_ref("SCALAR", Some("ID"), None))),
                              "isDeprecated": false, "deprecationReason": null },
                            { "name": "friends", "description": "Friends.", "args": [
                                { "name": "first", "description": null, "type": type_ref("SCALAR", Some("Int"), None), "defaultValue": "10" },
                                { "name": "order", "description": null, "type": type_ref("ENUM", Some("Order"), None), "defaultValue": null }
                              ],
                              "type": type_ref("LIST", None, Some(type_ref("NON_NULL", None, Some(type_ref("OBJECT", Some("User"), None))))),
                              "isDeprecated": true, "deprecationReason": "Use links." }
                        ],
                        "inputFields": null, "interfaces": [], "enumValues": null, "possibleTypes": null
                    },
                    {
                        "kind": "ENUM", "name": "Order", "description": null, "fields": null,
                        "inputFields": null, "interfaces": null, "possibleTypes": null,
                        "enumValues": [
                            { "name": "ASC", "description": null, "isDeprecated": false, "deprecationReason": null },
                            { "name": "DESC", "description": null, "isDeprecated": false, "deprecationReason": null }
                        ]
                    },
                    scalar("ID"),
                    scalar("Int"),
                    scalar("String"),
                    scalar("Boolean"),
                ],
                "directives": [{
                    "name": "include", "description": null, "locations": ["FIELD"],
                    "args": [{ "name": "if", "description": null,
                        "type": type_ref("NON_NULL", None, Some(type_ref("SCALAR", Some("Boolean"), None))),
                        "defaultValue": null }]
                }]
            }
        })
    }

    fn ok(body: String) -> ApiResponse {
        response(body, 200, "OK")
    }

    fn response(body: String, status: u16, status_text: &str) -> ApiResponse {
        ApiResponse {
            status,
            status_text: status_text.into(),
            duration_ms: 1.0,
            headers: vec![],
            size_bytes: body.len() as u64,
            body,
            body_id: None,
            truncated: None,
            binary: None,
            final_url: None,
            redirect_count: None,
            timing: None,
        }
    }

    fn draft() -> Draft {
        Draft {
            url: URL.into(),
            method: "GET".into(),
            body_mode: BodyMode::Json,
            body: "{\"ignored\":true}".into(),
            variables: Some("{\"x\":1}".into()),
            headers: vec![crate::model::Pair {
                id: 901,
                key: "X-Api-Key".into(),
                value: "{{key}}".into(),
                enabled: true,
                file: None,
            }],
            ..create_draft()
        }
    }

    fn defs(entries: &[(&str, &str)]) -> Definitions {
        entries
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    fn ctx() -> ResolvedRequestContext {
        ResolvedRequestContext {
            auth: AuthorizationConfig::Bearer {
                token: "tok".into(),
            },
            tokens: InterpolationContext::local(defs(&[("key", "secret")])),
        }
    }

    /// Fetch with a fake transport that returns `reply`; the sent request
    /// and whether the response was released.
    fn fetch(
        draft: &Draft,
        reply: Result<ApiResponse, String>,
    ) -> (
        Result<Arc<GraphqlSchema>, String>,
        Option<RequestInput>,
        bool,
    ) {
        let ctx = ctx();
        let sent = RefCell::new(None);
        let released = Cell::new(false);
        let result = block_on(fetch_schema_with(
            draft,
            Some((&ctx).into()),
            |request| {
                *sent.borrow_mut() = Some(request);
                async move { reply }
            },
            |_| released.set(true),
        ));
        (result, sent.into_inner(), released.get())
    }

    fn body(value: Value) -> String {
        value.to_string()
    }

    #[test]
    fn sends_a_post_introspection_with_the_drafts_headers_and_auth() {
        let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        clear_schema_cache();
        let (_, request, _) = fetch(&draft(), Ok(ok(body(json!({ "data": introspection() })))));
        let request = request.unwrap();
        assert_eq!(request.method, "POST");
        assert_eq!(request.url, URL);
        let sent: Value = serde_json::from_str(request.body.as_deref().unwrap_or("")).unwrap();
        assert!(sent["query"].as_str().unwrap().contains("__schema"));
        assert!(sent.get("variables").is_none());
        assert!(request.headers.contains(&Header {
            key: "X-Api-Key".into(),
            value: "secret".into()
        }));
        assert!(request.headers.contains(&Header {
            key: "Authorization".into(),
            value: "Bearer tok".into()
        }));
    }

    #[test]
    fn returns_the_schema_and_caches_it_by_url() {
        let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        clear_schema_cache();
        let (result, _, _) = fetch(&draft(), Ok(ok(body(json!({ "data": introspection() })))));
        let schema = result.unwrap();
        assert!(
            schema
                .get_query_type()
                .unwrap()
                .fields
                .iter()
                .any(|f| f.name == "viewer")
        );
        let cached = get_cached_schema(URL).unwrap();
        assert!(Arc::ptr_eq(&cached.schema, &schema));
        assert!(cached.fetched_at > 0.0);
        clear_schema_cache();
    }

    #[test]
    fn does_not_modify_the_draft() {
        let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let input = draft();
        let before = input.clone();
        let _ = fetch(&input, Ok(ok(body(json!({ "data": introspection() })))));
        assert_eq!(input, before);
        clear_schema_cache();
    }

    #[test]
    fn rethrows_build_request_errors() {
        let bad = Draft {
            url: "not a url".into(),
            ..draft()
        };
        let (result, sent, _) = fetch(&bad, Ok(ok(String::new())));
        assert!(result.unwrap_err().contains("Enter an absolute"));
        assert!(sent.is_none());
    }

    #[test]
    fn rethrows_transport_errors() {
        let (result, _, _) = fetch(&draft(), Err("Request timed out after 30 seconds.".into()));
        assert_eq!(result.unwrap_err(), "Request timed out after 30 seconds.");
    }

    #[test]
    fn reports_non_2xx_status() {
        let (result, _, _) = fetch(&draft(), Ok(response("nope".into(), 401, "Unauthorized")));
        assert_eq!(
            result.unwrap_err(),
            "Schema request failed: 401 Unauthorized"
        );
    }

    #[test]
    fn reports_non_2xx_status_without_status_text() {
        let (result, _, _) = fetch(&draft(), Ok(response("nope".into(), 500, "")));
        assert_eq!(result.unwrap_err(), "Schema request failed: 500");
    }

    #[test]
    fn reports_bodies_that_are_not_an_introspection_result() {
        for text in [
            "<html>".to_string(),
            "null".to_string(),
            body(json!({ "data": {} })),
            body(json!({ "data": { "__schema": { "types": 1 } } })),
        ] {
            let (result, _, _) = fetch(&draft(), Ok(ok(text.clone())));
            assert_eq!(result.unwrap_err(), NOT_INTROSPECTION, "{text}");
        }
    }

    #[test]
    fn reports_the_first_graphql_error_when_there_is_no_schema() {
        let text = body(json!({
            "errors": [{ "message": "Introspection is disabled" }, { "message": "x" }],
        }));
        let (result, _, _) = fetch(&draft(), Ok(ok(text)));
        assert_eq!(result.unwrap_err(), "Introspection is disabled");
    }

    #[test]
    fn loads_the_schema_when_errors_accompany_a_usable_schema() {
        let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let text = body(json!({ "data": introspection(), "errors": [{ "message": "partial" }] }));
        let (result, _, _) = fetch(&draft(), Ok(ok(text)));
        assert!(result.unwrap().get_query_type().is_some());
        clear_schema_cache();
    }

    #[test]
    fn does_not_cache_on_failure() {
        let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        clear_schema_cache();
        let _ = fetch(&draft(), Ok(response("nope".into(), 401, "Unauthorized")));
        assert!(get_cached_schema(URL).is_none());
    }

    #[test]
    fn rejects_a_truncated_schema_response_and_releases_it() {
        let reply = ApiResponse {
            truncated: Some(true),
            body_id: Some("b".into()),
            ..ok("{".into())
        };
        let (result, _, released) = fetch(&draft(), Ok(reply));
        assert_eq!(
            result.unwrap_err(),
            "Schema response exceeds the inspection limit."
        );
        assert!(released);
    }

    #[test]
    fn rejects_references_to_missing_types() {
        let mut data = introspection();
        data["__schema"]["types"].as_array_mut().unwrap().remove(1);
        assert_eq!(build_client_schema(&data).unwrap_err(), NOT_INTROSPECTION);
    }

    #[test]
    fn returns_the_resolved_url_including_query_params() {
        let d = Draft {
            url: "https://{{host}}/graphql".into(),
            query: vec![crate::model::Pair {
                id: 902,
                key: "v".into(),
                value: "2".into(),
                enabled: true,
                file: None,
            }],
            ..draft()
        };
        let ctx = ResolvedRequestContext {
            auth: AuthorizationConfig::None,
            tokens: InterpolationContext::local(defs(&[("host", "api.test"), ("key", "k")])),
        };
        assert_eq!(
            schema_key(&d, Some((&ctx).into())).as_deref(),
            Some("https://api.test/graphql?v=2")
        );
    }

    #[test]
    fn returns_null_instead_of_throwing_for_an_invalid_url() {
        let ctx = ctx();
        let d = Draft {
            url: String::new(),
            ..draft()
        };
        assert_eq!(schema_key(&d, Some((&ctx).into())), None);
    }

    #[test]
    fn formats_schema_age() {
        let t = 1_000_000_000.0;
        for (elapsed, expected) in [
            (0.0, "just now"),
            (59_000.0, "just now"),
            (60_000.0, "1m ago"),
            (59.0 * 60_000.0, "59m ago"),
            (60.0 * 60_000.0, "1h ago"),
            (5.0 * 60.0 * 60_000.0, "5h ago"),
        ] {
            assert_eq!(format_schema_age(t, t + elapsed), expected);
        }
    }

    fn schema() -> GraphqlSchema {
        build_client_schema(&introspection()).unwrap()
    }

    fn labels(text: &str) -> Vec<String> {
        let offset = text.find('|').unwrap();
        let text = text.replace('|', "");
        graphql_completions(&schema(), &text, offset)
            .items
            .into_iter()
            .map(|item| item.label)
            .collect()
    }

    #[test]
    fn completes_keywords_at_the_top_level() {
        assert_eq!(
            labels("|"),
            vec!["query", "mutation", "subscription", "fragment"]
        );
        assert_eq!(labels("{ viewer { id } }\nqu|"), vec!["query"]);
    }

    #[test]
    fn completes_fields_of_the_selected_type() {
        assert_eq!(
            labels("{ |"),
            vec!["viewer", "__typename", "__schema", "__type"]
        );
        assert_eq!(
            labels("query Q { viewer { |"),
            vec!["id", "friends", "__typename"]
        );
        assert_eq!(labels("{ viewer { friends { i| } }"), vec!["id"]);
        assert_eq!(labels("{ me: viewer { f|"), vec!["friends"]);
        assert_eq!(
            labels("{ viewer { id # c\n  |"),
            vec!["id", "friends", "__typename"]
        );
    }

    #[test]
    fn completes_arguments_and_enum_values() {
        assert_eq!(labels("{ viewer { friends(|"), vec!["first", "order"]);
        assert_eq!(labels("{ viewer { friends(first: 2, o|"), vec!["order"]);
        assert_eq!(labels("{ viewer { friends(order: |"), vec!["ASC", "DESC"]);
        assert_eq!(labels("{ viewer @include(if: |"), vec!["true", "false"]);
    }

    #[test]
    fn completes_type_conditions_and_directives() {
        assert_eq!(labels("fragment F on U|"), vec!["User"]);
        assert_eq!(labels("{ viewer { ... on |"), vec!["Query", "User"]);
        assert_eq!(labels("{ viewer @|"), vec!["include"]);
        assert_eq!(
            labels("fragment F on User { |"),
            vec!["id", "friends", "__typename"]
        );
    }

    #[test]
    fn suggests_nothing_in_comments_and_strings() {
        assert!(labels("{ viewer { # i|").is_empty());
        assert!(labels("{ viewer { friends(order: \"A|").is_empty());
        assert_eq!(
            labels("{ viewer { friends(order: \"x\", |"),
            vec!["first", "order"]
        );
    }

    #[test]
    fn describes_fields_for_the_editor() {
        let item = graphql_completions(&schema(), "{ viewer { fr", 13)
            .items
            .remove(0);
        assert_eq!(item.detail, "[User!]");
        assert_eq!(item.documentation.as_deref(), Some("Friends."));
        assert!(item.deprecated);
        assert_eq!(graphql_completions(&schema(), "{ viewer { fr", 13).from, 11);
    }

    #[test]
    fn pair_ids_are_not_needed_for_schema_keys() {
        let d = Draft {
            query: vec![pair("", "")],
            ..draft()
        };
        let ctx = ctx();
        assert_eq!(schema_key(&d, Some((&ctx).into())).as_deref(), Some(URL));
    }
}
