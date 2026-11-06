//! `t!`, the app's words (`pley_i18n::t`), checked as the code builds: the key must be in the
//! English catalogs, a counted key needs `count`, and every `{{name}}` its words hold must be given
//! a value, with no value given that they don't hold. A mistake fails the build at the call.

use std::collections::HashMap;
use std::sync::OnceLock;

use proc_macro::{Delimiter, Group, Literal, Punct, Spacing, Span, TokenStream, TokenTree};

include!(concat!(env!("OUT_DIR"), "/english.rs"));

/// The English words by their whole key (`signin.servers.title`), read once per build.
fn english() -> &'static HashMap<String, String> {
    static WORDS: OnceLock<HashMap<String, String>> = OnceLock::new();
    WORDS.get_or_init(|| {
        let mut words = HashMap::new();
        for (area, json) in ENGLISH {
            let value: serde_json::Value =
                serde_json::from_str(json).unwrap_or_else(|e| panic!("locales/en/{area}.json isn't JSON: {e}"));
            flatten(area, &value, &mut words);
        }
        words
    })
}

fn flatten(prefix: &str, value: &serde_json::Value, into: &mut HashMap<String, String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                flatten(&format!("{prefix}.{k}"), v, into);
            }
        }
        serde_json::Value::String(s) => {
            into.insert(prefix.to_string(), s.clone());
        }
        _ => {}
    }
}

/// The names of the `{{name}}` and `{{name, format}}` in `text`.
fn placeholders(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find("{{") {
        let Some(close) = rest[open + 2..].find("}}") else { break };
        let inside = &rest[open + 2..open + 2 + close];
        names.push(inside.split(',').next().unwrap_or("").trim().to_string());
        rest = &rest[open + 2 + close + 2..];
    }
    names
}

/// The words for a key, in the language shown: `t!("signin.servers.title")` is a `&'static str`;
/// `t!("signin.servers.forgetTitle", name = row.name)` fills in `{{name}}` and is a `String`;
/// `t!("library.movies", count = n)` takes the plural form of `n` (`library.movies_one`,
/// `_many`, `_other`) and fills in `{{count}}` and any other values.
#[proc_macro]
pub fn t(input: TokenStream) -> TokenStream {
    match expand(input) {
        Ok(out) => out,
        Err((message, span)) => error(&message, span),
    }
}

type Failure = (String, Span);

/// The tokens of `input` split at its top-level commas.
fn arguments(input: TokenStream) -> Vec<Vec<TokenTree>> {
    let mut parts = vec![Vec::new()];
    for tree in input {
        match &tree {
            TokenTree::Punct(p) if p.as_char() == ',' => parts.push(Vec::new()),
            _ => parts.last_mut().expect("one part at least").push(tree),
        }
    }
    if parts.last().is_some_and(Vec::is_empty) {
        parts.pop();
    }
    parts
}

/// A string literal's text, through the invisible groups a `macro_rules!` passes it in.
fn literal_text(tokens: &[TokenTree]) -> Option<(String, Span)> {
    match tokens {
        [TokenTree::Literal(l)] => {
            let s = l.to_string();
            let inner = s.strip_prefix('"')?.strip_suffix('"')?;
            (!inner.contains('\\')).then(|| (inner.to_string(), l.span()))
        }
        [TokenTree::Group(g)] if g.delimiter() == Delimiter::None => {
            literal_text(&g.stream().into_iter().collect::<Vec<_>>())
        }
        _ => None,
    }
}

fn expand(input: TokenStream) -> Result<TokenStream, Failure> {
    let mut args = arguments(input).into_iter();
    let first = args.next().ok_or_else(|| ("t! takes a key: t!(\"area.key\")".to_string(), Span::call_site()))?;
    let (key, span) = literal_text(&first)
        .ok_or_else(|| ("t!'s key is a plain string literal: t!(\"area.key\")".to_string(), Span::call_site()))?;

    let mut values: Vec<(String, Span, TokenStream)> = Vec::new();
    for arg in args {
        match arg.as_slice() {
            [TokenTree::Ident(name), TokenTree::Punct(eq), rest @ ..] if eq.as_char() == '=' && !rest.is_empty() => {
                let name_text = name.to_string();
                if values.iter().any(|(n, _, _)| *n == name_text) {
                    return Err((format!("{name_text} is given twice"), name.span()));
                }
                values.push((name_text, name.span(), rest.iter().cloned().collect()));
            }
            _ => return Err(("t!'s values are `name = value`".to_string(), span)),
        }
    }

    let words = english();
    let counted = !words.contains_key(&key);
    let forms: Vec<&String> = if counted {
        ["one", "other"].iter().filter_map(|f| words.get(&format!("{key}_{f}"))).collect()
    } else {
        words.get(&key).into_iter().collect()
    };
    if forms.is_empty() {
        return Err((format!("there are no words for \"{key}\" in locales/en"), span));
    }
    if counted && forms.len() < 2 {
        return Err((format!("\"{key}\" is counted: locales/en needs {key}_one and {key}_other"), span));
    }
    let count = values.iter().position(|(n, _, _)| n == "count");
    if counted && count.is_none() {
        return Err((format!("\"{key}\" is counted: give it count = …"), span));
    }
    let held: Vec<String> = forms.iter().flat_map(|f| placeholders(f)).collect();
    for (name, name_span, _) in &values {
        if !held.contains(name) && !(counted && name == "count") {
            return Err((format!("\"{key}\" has no {{{{{name}}}}}"), *name_span));
        }
    }
    for name in &held {
        if !values.iter().any(|(n, _, _)| n == name) {
            return Err((format!("\"{key}\" needs a value for {{{{{name}}}}}"), span));
        }
    }

    let key_literal = TokenTree::Literal(Literal::string(&key));
    if counted {
        let (_, _, n) = values.remove(count.expect("checked above"));
        let mut out = code("::pley_i18n::count");
        let mut inner = TokenStream::from(key_literal);
        inner.extend(code(", ::pley_i18n::__n"));
        inner.extend([paren(n), punct(',')]);
        inner.extend(slice(&values));
        out.extend([paren(inner)]);
        return Ok(out);
    }
    if values.is_empty() {
        let mut out = code("::pley_i18n::text");
        out.extend([paren(TokenStream::from(key_literal))]);
        return Ok(out);
    }
    let mut out = code("::pley_i18n::fill");
    let mut inner = code("::pley_i18n::text");
    inner.extend([paren(TokenStream::from(key_literal)), punct(',')]);
    inner.extend(slice(&values));
    out.extend([paren(inner)]);
    Ok(out)
}

/// `&[("name", &(value) as &dyn ::core::fmt::Display), …]`.
fn slice(values: &[(String, Span, TokenStream)]) -> TokenStream {
    let mut items = TokenStream::new();
    for (name, _, value) in values {
        let mut pair = TokenStream::from(TokenTree::Literal(Literal::string(name)));
        pair.extend([punct(','), punct('&'), paren(value.clone())]);
        pair.extend(code("as &dyn ::core::fmt::Display"));
        items.extend([paren(pair), punct(',')]);
    }
    let mut out = TokenStream::from(punct('&'));
    out.extend([TokenTree::Group(Group::new(Delimiter::Bracket, items))]);
    out
}

fn code(text: &str) -> TokenStream {
    text.parse().expect("the macro's own code parses")
}

fn paren(inner: TokenStream) -> TokenTree {
    TokenTree::Group(Group::new(Delimiter::Parenthesis, inner))
}

fn punct(c: char) -> TokenTree {
    TokenTree::Punct(Punct::new(c, Spacing::Alone))
}

/// `compile_error!("message")` at `span`.
fn error(message: &str, span: Span) -> TokenStream {
    let mut text = Literal::string(message);
    text.set_span(span);
    let mut out: TokenStream = code("::core::compile_error!")
        .into_iter()
        .map(|mut t| {
            t.set_span(span);
            t
        })
        .collect();
    let mut group = Group::new(Delimiter::Parenthesis, TokenStream::from(TokenTree::Literal(text)));
    group.set_span(span);
    out.extend([TokenTree::Group(group)]);
    out
}
