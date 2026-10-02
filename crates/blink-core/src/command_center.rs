//! Port of `src/lib/command-center.ts`. Match indices count characters.

use std::cmp::Reverse;
use std::collections::{BTreeSet, HashMap, HashSet};

use crate::model::{Definitions, RequestGroup, RequestSession};
use crate::session::{LabelTokens, display_method, session_label};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestMatch {
    pub id: u64,
    pub method: String,
    pub label: String,
    pub url: String,
    pub group_path: String,
    /// Matched characters in `label`, ascending.
    pub label_indices: Vec<usize>,
}

/// Group names from the root to `group_id`, joined with " / ".
pub fn group_path(groups: &[RequestGroup], group_id: Option<u64>) -> String {
    let by_id: HashMap<u64, &RequestGroup> = groups.iter().map(|group| (group.id, group)).collect();
    let mut names = Vec::new();
    let mut seen = HashSet::new();
    let mut group = group_id.and_then(|id| by_id.get(&id).copied());
    while let Some(current) = group {
        if !seen.insert(current.id) {
            break;
        }
        names.push(current.name.as_str());
        group = current.parent_id.and_then(|id| by_id.get(&id).copied());
    }
    names.reverse();
    names.join(" / ")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuzzyMatch {
    pub score: i64,
    pub indices: Vec<usize>,
}

fn boundary(text: &[char], index: usize) -> bool {
    if index == 0 {
        return true;
    }
    let previous = text[index - 1];
    !previous.is_ascii_alphanumeric()
        || (previous.is_ascii_lowercase() && text.get(index).is_some_and(char::is_ascii_uppercase))
}

/// Lower case, one character for one, so indices stay aligned.
fn lower(text: &str) -> Vec<char> {
    text.chars()
        .map(|c| {
            let mut lower = c.to_lowercase();
            match (lower.next(), lower.next()) {
                (Some(single), None) => single,
                _ => c,
            }
        })
        .collect()
}

fn find(haystack: &[char], needle: &[char], from: usize) -> Option<usize> {
    if needle.len() > haystack.len() {
        return None;
    }
    (from..=haystack.len() - needle.len()).find(|&at| haystack[at..at + needle.len()] == *needle)
}

/// Match `pattern` in `text` without case. An exact substring scores highest,
/// more so at a word start. Otherwise the characters must appear in order;
/// consecutive and word-start characters score more. None when no match.
pub fn fuzzy_match(text: &str, pattern: &str) -> Option<FuzzyMatch> {
    let original: Vec<char> = text.chars().collect();
    let haystack = lower(text);
    let needle = lower(pattern);
    if needle.is_empty() {
        return Some(FuzzyMatch {
            score: 0,
            indices: vec![],
        });
    }
    let mut best = None;
    let mut at = find(&haystack, &needle, 0);
    while let Some(found) = at {
        if best.is_none() {
            best = Some(found);
        }
        if boundary(&original, found) {
            best = Some(found);
            break;
        }
        at = find(&haystack, &needle, found + 1);
    }
    if let Some(best) = best {
        return Some(FuzzyMatch {
            score: 1000 + if boundary(&original, best) { 100 } else { 0 },
            indices: (best..best + needle.len()).collect(),
        });
    }
    let mut indices: Vec<usize> = Vec::new();
    let mut score = 0;
    let mut from = 0;
    for c in needle {
        let at = (from..haystack.len()).find(|&index| haystack[index] == c)?;
        score += 1;
        if indices.last().is_some_and(|&last| at == last + 1) {
            score += 5;
        }
        if boundary(&original, at) {
            score += 8;
        }
        indices.push(at);
        from = at + 1;
    }
    Some(FuzzyMatch { score, indices })
}

fn words(query: &str) -> Vec<String> {
    query
        .trim()
        .to_lowercase()
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

/// Rank prepared rows. Every word of `query` must match the label or group
/// path (fuzzy), or the URL or method (substring). Best matches first.
pub fn rank_requests(rows: Vec<RequestMatch>, query: &str) -> Vec<RequestMatch> {
    let needles = words(query);
    let mut ranked: Vec<(RequestMatch, i64)> = Vec::new();
    'rows: for mut row in rows {
        let mut score = 0;
        let mut indices = BTreeSet::new();
        for needle in &needles {
            let label = fuzzy_match(&row.label, needle);
            let group = fuzzy_match(&row.group_path, needle);
            let exact = if [&row.url, &row.method]
                .iter()
                .any(|text| text.to_lowercase().contains(needle.as_str()))
            {
                900
            } else {
                -1
            };
            let best = label
                .as_ref()
                .map_or(-1, |m| m.score)
                .max(group.as_ref().map_or(-1, |m| m.score))
                .max(exact);
            if best < 0 {
                continue 'rows;
            }
            score += best;
            if let Some(label) = label
                && label.score == best
            {
                indices.extend(label.indices);
            }
        }
        row.label_indices = indices.into_iter().collect();
        ranked.push((row, score));
    }
    // Stable: equal scores keep list order.
    ranked.sort_by_key(|(_, score)| Reverse(*score));
    ranked.into_iter().map(|(row, _)| row).collect()
}

/// Requests where every word of `query` matches the label or group path
/// (fuzzy), or the URL or method (substring). Best matches first.
pub fn match_requests(
    sessions: &[RequestSession],
    groups: &[RequestGroup],
    query: &str,
    global_definitions: &Definitions,
) -> Vec<RequestMatch> {
    let tokens = LabelTokens {
        groups,
        global_definitions,
    };
    let rows = sessions
        .iter()
        .map(|session| RequestMatch {
            id: session.id,
            method: display_method(session).to_string(),
            label: session_label(session, Some(tokens)),
            url: session.draft.url.clone(),
            group_path: group_path(groups, session.group_id),
            label_indices: vec![],
        })
        .collect();
    rank_requests(rows, query)
}

/// An app action in the command center. `>` in the search switches to these.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Command {
    pub id: String,
    pub label: String,
    /// Keys for `shortcut_label`, such as `["mod", "\\"]`.
    pub shortcut: Option<Vec<String>>,
    pub disabled: bool,
}

pub const COMMAND_PREFIX: &str = ">";

pub fn is_command_query(query: &str) -> bool {
    query.starts_with(COMMAND_PREFIX)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RankedCommand {
    pub command: Command,
    /// Matched characters in the label, ascending.
    pub indices: Vec<usize>,
}

/// Commands whose label fuzzy-matches every word of `query`. Best first.
pub fn match_commands(commands: &[Command], query: &str) -> Vec<RankedCommand> {
    let query = query.strip_prefix(COMMAND_PREFIX).unwrap_or(query);
    let needles = words(query);
    let mut ranked: Vec<(RankedCommand, i64)> = Vec::new();
    'commands: for command in commands {
        let mut score = 0;
        let mut indices = BTreeSet::new();
        for needle in &needles {
            let Some(found) = fuzzy_match(&command.label, needle) else {
                continue 'commands;
            };
            score += found.score;
            indices.extend(found.indices);
        }
        ranked.push((
            RankedCommand {
                command: command.clone(),
                indices: indices.into_iter().collect(),
            },
            score,
        ));
    }
    // Stable: equal scores keep list order.
    ranked.sort_by_key(|(_, score)| Reverse(*score));
    ranked.into_iter().map(|(command, _)| command).collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightRun {
    pub text: String,
    pub matched: bool,
}

/// Split `text` into runs, marking the characters at `indices`.
pub fn highlight_runs(text: &str, indices: &[usize]) -> Vec<HighlightRun> {
    let marked: HashSet<usize> = indices.iter().copied().collect();
    let mut runs: Vec<HighlightRun> = Vec::new();
    for (index, c) in text.chars().enumerate() {
        let matched = marked.contains(&index);
        match runs.last_mut() {
            Some(last) if last.matched == matched => last.text.push(c),
            _ => runs.push(HighlightRun {
                text: c.to_string(),
                matched,
            }),
        }
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::create_session;

    fn group(id: u64, name: &str, parent_id: Option<u64>) -> RequestGroup {
        RequestGroup {
            id,
            name: name.into(),
            parent_id,
            collapsed: false,
            local_auth: None,
            local_definitions: None,
            response_tokens: None,
            default_method: None,
            default_url: None,
            environments: None,
            active_environment_id: None,
        }
    }

    fn groups() -> Vec<RequestGroup> {
        vec![group(1, "Platform", None), group(2, "Identity", Some(1))]
    }

    fn session(url: &str, group_id: Option<u64>) -> RequestSession {
        let mut value = create_session(None);
        value.draft.url = url.into();
        value.group_id = group_id;
        value
    }

    fn command(id: &str, label: &str) -> Command {
        Command {
            id: id.into(),
            label: label.into(),
            ..Command::default()
        }
    }

    fn ids(commands: &[RankedCommand]) -> Vec<&str> {
        commands.iter().map(|c| c.command.id.as_str()).collect()
    }

    #[test]
    fn builds_the_group_path_from_the_root() {
        assert_eq!(group_path(&groups(), Some(2)), "Platform / Identity");
        assert_eq!(group_path(&groups(), None), "");
        assert_eq!(group_path(&groups(), Some(99)), "");
    }

    #[test]
    fn stops_on_a_group_cycle() {
        let cyclic = [group(1, "A", Some(2)), group(2, "B", Some(1))];
        assert_eq!(group_path(&cyclic, Some(1)), "B / A");
    }

    #[test]
    fn matches_label_url_and_group_path_without_case() {
        let users = session("https://api.example.test/users", Some(2));
        let health = session("https://status.example.test/health", None);
        let all = [users.clone(), health.clone()];
        let none = Definitions::new();
        let found = |query: &str| -> Vec<u64> {
            match_requests(&all, &groups(), query, &none)
                .iter()
                .map(|m| m.id)
                .collect()
        };
        assert_eq!(found(""), [users.id, health.id]);
        assert_eq!(found("IDENTITY"), [users.id]);
        assert_eq!(found("status.example"), [health.id]);
        assert_eq!(found("  health "), [health.id]);
        assert!(found("nothing").is_empty());
        let first = &match_requests(&all, &groups(), "users", &none)[0];
        assert_eq!(first.method, "GET");
        assert_eq!(first.group_path, "Platform / Identity");
    }

    #[test]
    fn lists_every_command_for_a_bare_prefix() {
        let commands = [
            command("layout", "View: Stack request above response"),
            command("browser", "View: Hide request browser"),
            command("new", "Request: New request"),
        ];
        assert_eq!(
            ids(&match_commands(&commands, ">")),
            ["layout", "browser", "new"]
        );
    }

    #[test]
    fn matches_every_word_in_any_order() {
        let commands = [
            command("layout", "View: Stack request above response"),
            command("browser", "View: Hide request browser"),
            command("new", "Request: New request"),
        ];
        assert_eq!(
            ids(&match_commands(&commands, ">response stack")),
            ["layout"]
        );
        assert_eq!(
            ids(&match_commands(&commands, "> VIEW  request")),
            ["layout", "browser"]
        );
        assert!(match_commands(&commands, ">nothing").is_empty());
    }

    #[test]
    fn detects_command_queries() {
        assert!(is_command_query(">x"));
        assert!(!is_command_query("x>"));
    }

    #[test]
    fn prefers_a_substring_at_a_word_start() {
        let found = fuzzy_match("View: Stack request", "req").unwrap();
        assert_eq!(found.indices, [12, 13, 14]);
        assert!(found.score > fuzzy_match("prerequest", "req").unwrap().score);
    }

    #[test]
    fn matches_characters_in_order() {
        assert_eq!(
            fuzzy_match("Request: Duplicate request", "dupr")
                .unwrap()
                .indices,
            [9, 10, 11, 19]
        );
        assert_eq!(fuzzy_match("abc", "acb"), None);
    }

    #[test]
    fn ranks_commands_by_score_then_list_order() {
        let commands = [
            command("a", "View: Toggle unwrapped"),
            command("b", "Response: Wrap lines"),
        ];
        assert_eq!(ids(&match_commands(&commands, ">wrap")), ["b", "a"]);
        assert_eq!(ids(&match_commands(&commands, ">tgu")), ["a"]);
    }

    #[test]
    fn splits_text_into_highlighted_runs() {
        let run = |text: &str, matched| HighlightRun {
            text: text.into(),
            matched,
        };
        assert_eq!(
            highlight_runs("abcd", &[1, 2]),
            [run("a", false), run("bc", true), run("d", false)]
        );
    }

    #[test]
    fn matches_requests_fuzzily_by_label() {
        let users = session("https://api.example.test/v1/users/list", None);
        let found = match_requests(
            std::slice::from_ref(&users),
            &[],
            "usli",
            &Definitions::new(),
        );
        assert_eq!(found.iter().map(|m| m.id).collect::<Vec<_>>(), [users.id]);
    }
}
