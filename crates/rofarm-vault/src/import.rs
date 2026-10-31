use secrecy::SecretString;
use serde::Deserialize;
use std::collections::BTreeSet;

use crate::error::{VaultError, VaultResult};

#[derive(Clone)]
pub struct ImportedCredential {
    pub username: Option<String>,
    pub cookie: SecretString,
    pub roblox_user_id: Option<u64>,
    pub alias: Option<String>,
    pub tags: Vec<String>,
    pub notes: String,
}

impl std::fmt::Debug for ImportedCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImportedCredential")
            .field("username", &self.username)
            .field("cookie", &"[redacted]")
            .field("roblox_user_id", &self.roblox_user_id)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportIssue {
    pub line: usize,
    pub message: String,
}

#[derive(Debug, Default)]
pub struct CredentialImport {
    pub credentials: Vec<ImportedCredential>,
    pub issues: Vec<ImportIssue>,
    pub duplicates: Vec<String>,
}

impl CredentialImport {
    pub fn summary(&self) -> String {
        format!(
            "{} credential(s), {} duplicate(s), {} issue(s)",
            self.credentials.len(),
            self.duplicates.len(),
            self.issues.len()
        )
    }
}

fn is_valid_username(name: &str) -> bool {
    let len = name.chars().count();
    (3..=32).contains(&len) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub fn looks_like_cookie(value: &str) -> bool {
    let v = value.trim();
    if v.len() < 40 || v.len() > 4096 {
        return false;
    }
    if v.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return false;
    }
    v.starts_with("_|WARNING")
        || v.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '=' | '/' | '+' | '|' | ':'))
}

pub fn cookie_fingerprint(cookie: &str) -> String {
    let digest = blake3::hash(cookie.trim().as_bytes());
    digest.to_hex()[..16].to_string()
}

fn parse_line(line: &str, lineno: usize, seen: &mut BTreeSet<String>) -> Result<Option<ImportedCredential>, ImportIssue> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return Ok(None);
    }
    let issue = |msg: &str| ImportIssue { line: lineno, message: msg.to_string() };
    let (username, cookie) = if let Some((left, right)) = trimmed.split_once('\t') {
        (Some(left.trim().to_string()), right.trim())
    } else if let Some((left, right)) = trimmed.split_once(',') {
        if looks_like_cookie(right.trim()) {
            (Some(left.trim().to_string()), right.trim())
        } else {
            (None, trimmed)
        }
    } else if let Some((left, right)) = trimmed.split_once(':') {
        if right.trim_start().starts_with("_|WARNING") || right.trim().starts_with("CA") {
            (Some(left.trim().to_string()), right.trim())
        } else {
            (None, trimmed)
        }
    } else {
        (None, trimmed)
    };
    if let Some(name) = &username {
        if !is_valid_username(name) {
            return Err(issue("Invalid username on this line."));
        }
    }
    if !looks_like_cookie(cookie) {
        return Err(issue("This does not look like a Roblox session cookie."));
    }
    let key = username.clone().unwrap_or_else(|| cookie_fingerprint(cookie));
    if !seen.insert(key.clone()) {
        return Err(ImportIssue { line: lineno, message: format!("Duplicate entry for {key}.") });
    }
    Ok(Some(ImportedCredential {
        username,
        cookie: SecretString::from(cookie.to_string()),
        roblox_user_id: None,
        alias: None,
        tags: Vec::new(),
        notes: String::new(),
    }))
}

pub fn parse_bulk_paste(text: &str) -> CredentialImport {
    let mut out = CredentialImport::default();
    let mut seen = BTreeSet::new();
    for (idx, line) in text.lines().enumerate() {
        match parse_line(line, idx + 1, &mut seen) {
            Ok(Some(cred)) => out.credentials.push(cred),
            Ok(None) => {}
            Err(issue) => {
                if issue.message.starts_with("Duplicate") {
                    out.duplicates.push(issue.message);
                } else {
                    out.issues.push(issue);
                }
            }
        }
    }
    out
}

pub fn parse_ram_export(json: &str) -> VaultResult<CredentialImport> {
    #[derive(Deserialize)]
    struct Ram {
        #[serde(default, alias = "Username")]
        username: String,
        #[serde(default, alias = "Alias")]
        alias: String,
        #[serde(default, alias = "UserId", alias = "UserID")]
        user_id: Option<u64>,
        #[serde(default, alias = "Note")]
        note: String,
        #[serde(default, alias = "Group")]
        group: String,
        #[serde(default, alias = "Tags")]
        tags: Vec<String>,
        #[serde(default, alias = "Cookie")]
        cookie: String,
    }
    let value: serde_json::Value = serde_json::from_str(json).map_err(|_| VaultError::Invalid("Not valid JSON.".into()))?;
    let list = value.get("Accounts").or_else(|| value.get("accounts")).cloned().unwrap_or(value);
    let entries: Vec<Ram> = serde_json::from_value(list).map_err(|_| VaultError::Invalid("Unrecognized RAM export.".into()))?;
    let mut out = CredentialImport::default();
    let mut seen = BTreeSet::new();
    for (idx, entry) in entries.into_iter().enumerate() {
        let lineno = idx + 1;
        if !entry.username.trim().is_empty() && !is_valid_username(entry.username.trim()) {
            out.issues.push(ImportIssue { line: lineno, message: "Invalid username.".into() });
            continue;
        }
        if !looks_like_cookie(&entry.cookie) {
            out.issues.push(ImportIssue { line: lineno, message: "Missing or malformed cookie.".into() });
            continue;
        }
        let key = if entry.username.trim().is_empty() {
            cookie_fingerprint(&entry.cookie)
        } else {
            entry.username.to_ascii_lowercase()
        };
        if !seen.insert(key.clone()) {
            out.duplicates.push(key);
            continue;
        }
        let mut tags = entry.tags;
        if !entry.group.trim().is_empty() {
            tags.push(entry.group.trim().to_string());
        }
        out.credentials.push(ImportedCredential {
            username: if entry.username.trim().is_empty() { None } else { Some(entry.username.trim().to_string()) },
            cookie: SecretString::from(entry.cookie),
            roblox_user_id: entry.user_id,
            alias: if entry.alias.trim().is_empty() { None } else { Some(entry.alias.trim().to_string()) },
            tags,
            notes: entry.note,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use secrecy::ExposeSecret;

    fn cookie(prefix: &str) -> String {
        format!("_|WARNING:-DO-NOT-SHARE|_|{prefix}{}", "A".repeat(400))
    }

    #[test]
    fn parses_tab_colon_comma_and_bare_cookies() {
        let c1 = cookie("one");
        let c2 = cookie("two");
        let c3 = cookie("three");
        let text = format!("Alpha\t{c1}\nBeta:{c2}\nGamma,{c3}\n\n# comment\n{}", cookie("bare"));
        let parsed = parse_bulk_paste(&text);
        assert_eq!(parsed.credentials.len(), 4);
        assert_eq!(parsed.credentials[0].username.as_deref(), Some("Alpha"));
        assert_eq!(parsed.credentials[1].username.as_deref(), Some("Beta"));
        assert_eq!(parsed.credentials[2].username.as_deref(), Some("Gamma"));
        assert!(parsed.credentials[3].username.is_none());
        assert!(parsed.issues.is_empty());
    }

    #[test]
    fn rejects_bad_cookies_and_duplicate_users() {
        let c = cookie("x");
        let text = format!("Bad name\t{c}\nAlpha\t{c}\nAlpha\t{c}\nshort\tnotacookie");
        let parsed = parse_bulk_paste(&text);
        assert_eq!(parsed.credentials.len(), 1);
        assert_eq!(parsed.duplicates.len(), 1);
        assert_eq!(parsed.issues.len(), 2);
        assert!(!looks_like_cookie("abc"));
        assert!(!looks_like_cookie(&"a".repeat(5000)));
        assert!(looks_like_cookie(&c));
    }

    #[test]
    fn credentials_never_print_cookies() {
        let c = cookie("secret");
        let parsed = parse_bulk_paste(&format!("Alpha\t{c}"));
        let debug = format!("{:?}", parsed.credentials[0]);
        assert!(!debug.contains("DO-NOT-SHARE"));
        assert!(!debug.contains(&"A".repeat(50)));
        assert!(debug.contains("redacted"));
        assert_eq!(parsed.credentials[0].cookie.expose_secret(), c);
    }

    #[test]
    fn fingerprints_are_stable_and_distinguish_cookies() {
        let a = cookie("a");
        let b = cookie("b");
        assert_eq!(cookie_fingerprint(&a), cookie_fingerprint(&a));
        assert_ne!(cookie_fingerprint(&a), cookie_fingerprint(&b));
        assert_eq!(cookie_fingerprint(&a).len(), 16);
    }

    #[test]
    fn ram_export_imports_cookies_quietly() {
        let json = format!(
            r#"{{"Accounts":[
                {{"Username":"Alpha","UserId":1,"Cookie":"{c}","Note":"main","Tags":["farm"],"Group":"G"}},
                {{"Username":"Beta","Cookie":"bad"}}
            ]}}"#,
            c = cookie("ram")
        );
        let parsed = parse_ram_export(&json).unwrap();
        assert_eq!(parsed.credentials.len(), 1);
        assert_eq!(parsed.issues.len(), 1);
        assert_eq!(parsed.credentials[0].tags.len(), 2);
        assert!(parsed.credentials[0].alias.is_none());
        assert!(!format!("{parsed:?}").contains("DO-NOT-SHARE"));
        assert!(parse_ram_export("not json").is_err());
    }

    proptest::proptest! {
        #[test]
        fn arbitrary_lines_never_panic(text in proptest::collection::vec(any::<char>(), 0..500)) {
            let s: String = text.into_iter().collect();
            let parsed = parse_bulk_paste(&s);
            let _ = parsed.summary();
        }
    }
}
