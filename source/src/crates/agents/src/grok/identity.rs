// Agent Bot modifications, 2026-10-26; upstream portions retain their original licenses.
//! Account-bound Bot display identities from the existing plaintext roster and
//! local photo cache. No network URLs, credentials, or transcript text are read.
use super::{account::CACHE, cache};
use crate::account::{locate, Host};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

const PHOTO_LIMIT: u64 = 2 * 1024 * 1024;
const TOTAL_PHOTO_LIMIT: u64 = 4 * 1024 * 1024;
const SHAPES: &[&str] = &[
    "blob", "pebble", "bean", "egg", "squircle", "tablet", "capsule", "cylinder", "hex", "gem",
    "crystal", "wedge", "shield", "dome", "arch", "cloud", "teardrop", "leaf", "square", "sparkle",
    "clover", "flower", "heart", "house", "star", "pumpkin",
];
// Color IDs and values are the installed client's public avatar palette.
const COLORS: &[(&str, &str)] = &[
    ("black", "#000000"),
    ("brown", "#936439"),
    ("red", "#FF263C"),
    ("orange", "#FF6700"),
    ("yellow", "#FF9800"),
    ("green", "#00C972"),
    ("cyan", "#00BCA6"),
    ("blue", "#1084FE"),
    ("violet", "#9159FE"),
    ("magenta", "#FF309B"),
    ("gray", "#777777"),
];

fn invalid() -> String {
    crate::l10n::text(
        "Grok Bot 身份缓存不可验证",
        "Grok Bot identity cache could not be verified",
    )
}

fn fallback(row: &Value, raw: &str) -> Value {
    let seed = u32::from_str_radix(&cache::hash(&[raw])[..8], 16).unwrap_or(0) as usize;
    let shape = row["avatarShape"]
        .as_str()
        .filter(|shape| SHAPES.contains(shape))
        .unwrap_or(SHAPES[seed % SHAPES.len()]);
    let color = row["avatarColor"]
        .as_str()
        .and_then(|id| COLORS.iter().find(|(name, _)| *name == id))
        .map(|(_, value)| *value)
        .unwrap_or(COLORS[(seed / SHAPES.len()) % COLORS.len()].1);
    json!({"shape":shape,"color":color})
}

fn mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() >= 24 && bytes.starts_with(b"\x89PNG\r\n\x1a\n") && &bytes[12..16] == b"IHDR" {
        Some("image/png")
    } else if bytes.len() >= 4 && bytes.starts_with(b"\xff\xd8\xff") {
        Some("image/jpeg")
    } else if bytes.len() >= 10 && (bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")) {
        Some("image/gif")
    } else if bytes.len() >= 16
        && bytes.starts_with(b"RIFF")
        && &bytes[8..12] == b"WEBP"
        && matches!(&bytes[12..16], b"VP8 " | b"VP8L" | b"VP8X")
    {
        Some("image/webp")
    } else {
        None
    }
}

fn photo_root(host: &dyn Host, cache_directory: &Path) -> Option<PathBuf> {
    let app = cache_directory.parent()?;
    let root = app.join("roster-avatars");
    if host.file_info(&root).found()?.symlink {
        return None;
    }
    let canonical = host.canonical(&root)?;
    (canonical == host.canonical(app)?.join("roster-avatars")).then_some(root)
}

fn photo(host: &dyn Host, root: Option<&Path>, row: &Value, budget: &mut u64) -> Option<String> {
    let root = root?;
    let reference = &row["avatarPhoto"];
    if reference["kind"] != "file" {
        return None;
    }
    let key = reference["key"].as_str().filter(|key| {
        key.len() == 32
            && key
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })?;
    let file = root.join(key);
    let info = host.file_info(&file).found()?;
    if info.symlink || info.len == 0 || info.len > PHOTO_LIMIT || info.len > *budget {
        return None;
    }
    let expected = host.canonical(root)?.join(key);
    if host.canonical(&file)? != expected {
        return None;
    }
    let bytes = host.read_file(&file, PHOTO_LIMIT).found()?;
    if bytes.is_empty()
        || bytes.len() as u64 > PHOTO_LIMIT
        || bytes.len() as u64 > *budget
        || host.file_info(&file).found()?.symlink
        || host.canonical(&file)? != expected
    {
        return None;
    }
    let content_type = mime(&bytes)?;
    *budget -= bytes.len() as u64;
    Some(format!(
        "data:{content_type};base64,{}",
        STANDARD.encode(bytes)
    ))
}

fn identities(
    host: &dyn Host,
    directory: &Path,
    slot: &str,
    roster: &Value,
) -> Result<Vec<Value>, String> {
    let rows = roster["rows"]
        .as_array()
        .filter(|rows| rows.len() <= 500)
        .ok_or_else(invalid)?;
    let root = photo_root(host, directory);
    let mut budget = TOTAL_PHOTO_LIMIT;
    let mut seen = HashSet::new();
    let mut duplicate = HashSet::new();
    let mut result = Vec::new();
    for row in rows {
        let Some(raw) = row["id"].as_str().filter(|id| cache::agent_id(id)) else {
            continue;
        };
        if !seen.insert(raw) {
            duplicate.insert(raw);
            continue;
        }
        let name = row["name"]
            .as_str()
            .unwrap_or("")
            .chars()
            .filter(|ch| !ch.is_control())
            .take(128)
            .collect::<String>();
        let name = if name.trim().is_empty() {
            "Bot"
        } else {
            name.trim()
        };
        let fallback = fallback(row, raw);
        let avatar = match photo(host, root.as_deref(), row, &mut budget) {
            Some(data_url) => json!({"kind":"photo","dataUrl":data_url,"fallback":fallback}),
            None => json!({"kind":"shape","shape":fallback["shape"],"color":fallback["color"]}),
        };
        result.push((
            raw,
            json!({"id":cache::session_id(slot,raw),"name":name,"avatar":avatar}),
        ));
    }
    Ok(result
        .into_iter()
        .filter(|(raw, _)| !duplicate.contains(raw))
        .map(|(_, identity)| identity)
        .collect())
}

/// Returns render-only identity rows. A missing/bad photo falls back per Bot;
/// account-marker changes reject the complete batch before anything is shown.
pub fn read(host: &dyn Host) -> Result<Vec<Value>, String> {
    let directory = locate(host, &CACHE).map_err(|_| invalid())?;
    let slot = cache::account_through(host, &directory).ok_or_else(invalid)?;
    let roster =
        cache::value_through(host, &directory, &cache::roster_key(&slot), 4).ok_or_else(invalid)?;
    let rows = identities(host, &directory, &slot, &roster)?;
    if cache::account_through(host, &directory).as_deref() != Some(slot.as_str()) {
        return Err(invalid());
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::{fake::FakeHost, Answer, FileInfo};
    const KEY: &str = "0123456789abcdef0123456789abcdef";
    fn setup() -> (FakeHost, PathBuf) {
        let directory = PathBuf::from("/fixture/Grok Bot/sand-client-persistence");
        let root = directory.parent().unwrap().join("roster-avatars");
        let mut host = FakeHost::new("/fixture");
        host.info.insert(
            root,
            FileInfo {
                len: 0,
                modified_ms: None,
                symlink: false,
            },
        );
        (host, directory)
    }
    fn png() -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR".to_vec();
        bytes.extend([0; 8]);
        bytes
    }
    fn roster(photo: Value) -> Value {
        json!({"rows":[{"id":"bot-a","name":" Full Bot Name ","title":"Unrelated task title",
        "avatarShape":"house","avatarColor":"green","avatarPhoto":photo}]})
    }

    #[test]
    fn roster_names_and_shapes_are_bound_to_the_current_account_not_task_titles() {
        let (host, dir) = setup();
        let roster = roster(Value::Null);
        let first = identities(&host, &dir, "account-one", &roster).unwrap();
        let second = identities(&host, &dir, "account-two", &roster).unwrap();
        assert_ne!(first[0]["id"], second[0]["id"]);
        assert_eq!(first[0]["name"], "Full Bot Name");
        assert_eq!(
            first[0]["avatar"],
            json!({"kind":"shape","shape":"house","color":"#00C972"})
        );
    }

    #[test]
    fn local_photo_magic_becomes_data_url_with_the_original_shape_fallback() {
        let (mut host, dir) = setup();
        let file = dir.parent().unwrap().join("roster-avatars").join(KEY);
        host.files.insert(file, Answer::Found(png()));
        let rows = identities(
            &host,
            &dir,
            "fixture-account",
            &roster(json!({"kind":"file","key":KEY})),
        )
        .unwrap();
        assert_eq!(rows[0]["avatar"]["kind"], "photo");
        assert!(rows[0]["avatar"]["dataUrl"]
            .as_str()
            .unwrap()
            .starts_with("data:image/png;base64,"));
        assert_eq!(
            rows[0]["avatar"]["fallback"],
            json!({"shape":"house","color":"#00C972"})
        );
        assert_eq!(mime(b"GIF89a1234"), Some("image/gif"));
        assert_eq!(mime(b"RIFF1234WEBPVP8X"), Some("image/webp"));
        assert!(mime(b"<svg xmlns='http://www.w3.org/2000/svg'/>").is_none());
    }

    #[test]
    fn invalid_links_and_outside_paths_fall_back_without_losing_other_bots() {
        let (mut host, dir) = setup();
        let root = dir.parent().unwrap().join("roster-avatars");
        let file = root.join(KEY);
        host.files.insert(file.clone(), Answer::Found(png()));
        for reference in [
            json!({"kind":"file","key":"../outside"}),
            json!({"kind":"url","key":"https://example.test/photo"}),
        ] {
            assert_eq!(
                identities(&host, &dir, "a", &roster(reference)).unwrap()[0]["avatar"]["kind"],
                "shape"
            );
        }
        host.canonical_paths
            .insert(file.clone(), PathBuf::from("/outside/private-file"));
        assert_eq!(
            identities(&host, &dir, "a", &roster(json!({"kind":"file","key":KEY}))).unwrap()[0]
                ["avatar"]["kind"],
            "shape"
        );
        host.canonical_paths.remove(&file);
        host.info.insert(
            file,
            FileInfo {
                len: 24,
                modified_ms: None,
                symlink: true,
            },
        );
        let mut batch = roster(json!({"kind":"file","key":KEY}));
        batch["rows"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"bot-b","name":"Second","avatarShape":"flower","avatarColor":"red"}));
        let rows = identities(&host, &dir, "a", &batch).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["avatar"]["kind"], "shape");
        assert_eq!(rows[1]["name"], "Second");
    }

    #[test]
    fn photo_and_batch_limits_keep_oversized_or_wrong_mime_data_out() {
        let (mut host, dir) = setup();
        let root = dir.parent().unwrap().join("roster-avatars");
        let file = root.join(KEY);
        host.files
            .insert(file.clone(), Answer::Found(b"<svg/>".to_vec()));
        let mut unused_budget = TOTAL_PHOTO_LIMIT;
        assert!(photo(
            &host,
            Some(&root),
            &roster(json!({"kind":"file","key":KEY}))["rows"][0],
            &mut unused_budget
        )
        .is_none());
        let mut image = png();
        image.resize(PHOTO_LIMIT as usize, 0);
        host.files.insert(file.clone(), Answer::Found(image));
        let row = roster(json!({"kind":"file","key":KEY}))["rows"][0].clone();
        let mut budget = TOTAL_PHOTO_LIMIT;
        assert!(photo(&host, Some(&root), &row, &mut budget).is_some());
        assert!(photo(&host, Some(&root), &row, &mut budget).is_some());
        assert!(photo(&host, Some(&root), &row, &mut budget).is_none());
        assert_eq!(budget, 0);
        host.info.insert(
            file,
            FileInfo {
                len: PHOTO_LIMIT + 1,
                modified_ms: None,
                symlink: false,
            },
        );
        let mut oversized_budget = TOTAL_PHOTO_LIMIT;
        assert!(photo(&host, Some(&root), &row, &mut oversized_budget).is_none());
    }

    #[test]
    fn malformed_rows_are_isolated_and_names_never_contain_control_characters() {
        let (host, dir) = setup();
        let mut body = roster(Value::Null);
        body["rows"][0]["name"] = json!(format!("\u{0000}{}\n", "名".repeat(140)));
        body["rows"][0]["avatarShape"] = json!("<script>");
        body["rows"][0]["avatarColor"] = json!("url(javascript:invalid)");
        body["rows"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"bad/id","name":"Invalid"}));
        let rows = identities(&host, &dir, "a", &body).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["name"].as_str().unwrap().chars().count(), 128);
        assert!(SHAPES.contains(&rows[0]["avatar"]["shape"].as_str().unwrap()));
        assert!(COLORS
            .iter()
            .any(|(_, hex)| *hex == rows[0]["avatar"]["color"].as_str().unwrap()));
    }

    #[test]
    fn public_reader_uses_only_the_current_accounts_roster() {
        let mut host = FakeHost::new("/fixture");
        let directory = PathBuf::from("/fixture/AppData/Grok Bot/sand-client-persistence");
        let marker = cache::path(&directory, cache::ACCOUNT_KEY);
        host.files.insert(
            marker.clone(),
            Answer::Found(
                serde_json::to_vec(&json!({"schemaVersion":1,"value":"account-one"})).unwrap(),
            ),
        );
        host.files.insert(
            cache::path(&directory, &cache::roster_key("account-one")),
            Answer::Found(
                serde_json::to_vec(&json!({"schemaVersion":4,"value":roster(Value::Null)}))
                    .unwrap(),
            ),
        );
        let rows = read(&host).unwrap();
        assert_eq!(rows[0]["id"], cache::session_id("account-one", "bot-a"));
        host.files.insert(
            marker,
            Answer::Found(
                serde_json::to_vec(&json!({"schemaVersion":1,"value":"account-two"})).unwrap(),
            ),
        );
        assert!(read(&host).is_err());
    }
}
