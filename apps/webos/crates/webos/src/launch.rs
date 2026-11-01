//! How the TV started the app: the JSON the application manager passes as
//! the only argument, e.g.
//! `{"event":"launch","reason":"…","appId":"app.pley.tv","parameters":{…},"interfaceVersion":2}`.
//!
//! `parameters` is whatever the caller of `com.webos.applicationManager/launch`
//! put in `params`. A launch while the app runs reaches only LG's SDL, which
//! drops it (`handlesRelaunch` stays false), so only cold launches carry
//! parameters.

use serde_json::{Map, Value};

/// The most of `argv[1]` that is read: a launch payload is a few hundred bytes.
const MAX_BYTES: usize = 64 * 1024;

/// The launch payload.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Launch {
    /// `launch` on a cold start.
    pub event: Option<String>,
    pub reason: Option<String>,
    pub app_id: Option<String>,
    /// The caller's `params`: empty when there were none.
    pub parameters: Map<String, Value>,
}

impl Launch {
    /// The launch of this process, from its first argument; empty when there is none
    /// (started from a shell, or in the simulator) or it isn't a launch payload.
    pub fn from_args() -> Launch {
        std::env::args().nth(1).and_then(|arg| Launch::parse(&arg)).unwrap_or_default()
    }

    /// Reads a launch payload; `None` when `arg` is not a JSON object.
    pub fn parse(arg: &str) -> Option<Launch> {
        if arg.len() > MAX_BYTES {
            return None;
        }
        let Value::Object(mut root) = serde_json::from_str::<Value>(arg).ok()? else {
            return None;
        };
        let text = |v: Option<Value>| match v {
            Some(Value::String(s)) if !s.is_empty() => Some(s),
            _ => None,
        };
        let parameters = match root.remove("parameters") {
            Some(Value::Object(map)) => map,
            // Some callers send the parameters as a JSON string.
            Some(Value::String(s)) => match serde_json::from_str(&s) {
                Ok(Value::Object(map)) => map,
                _ => Map::new(),
            },
            _ => Map::new(),
        };
        Some(Launch {
            event: text(root.remove("event")),
            reason: text(root.remove("reason")),
            app_id: text(root.remove("appId")),
            parameters,
        })
    }

    /// A string parameter, such as a link's `contentTarget`.
    pub fn parameter(&self, name: &str) -> Option<&str> {
        self.parameters.get(name).and_then(Value::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_application_managers_payload() {
        let launch = Launch::parse(
            r#"{"event":"launch","reason":"com.webos.app.home","appId":"app.pley.tv","parameters":{"contentTarget":"item/42","n":1},"interfaceVersion":2,"interfaceMethod":"registerApp"}"#,
        )
        .unwrap();
        assert_eq!(launch.event.as_deref(), Some("launch"));
        assert_eq!(launch.reason.as_deref(), Some("com.webos.app.home"));
        assert_eq!(launch.app_id.as_deref(), Some("app.pley.tv"));
        assert_eq!(launch.parameter("contentTarget"), Some("item/42"));
        assert_eq!(launch.parameter("n"), None);
    }

    #[test]
    fn takes_parameters_sent_as_a_string_and_ignores_anything_else() {
        let launch = Launch::parse(r#"{"event":"launch","parameters":"{\"a\":\"b\"}"}"#).unwrap();
        assert_eq!(launch.parameter("a"), Some("b"));
        assert_eq!(Launch::parse(r#"{"parameters":[1]}"#).unwrap().parameters.len(), 0);
        assert_eq!(Launch::parse("--fixtures"), None);
        assert_eq!(Launch::parse("[1,2]"), None);
        assert_eq!(Launch::parse(&format!("{{\"a\":\"{}\"}}", "x".repeat(MAX_BYTES))), None);
    }
}
