//! Port of `src/lib/diff.ts`.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffKind {
    Same,
    Add,
    Remove,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: DiffKind,
    pub text: String,
    /// 1-based line numbers in the old and new text.
    pub before: Option<usize>,
    pub after: Option<usize>,
}

/// Beyond this many changes the diff stops; callers show a notice.
pub const DIFF_EDIT_LIMIT: usize = 4000;

/// Line diff with the Myers O(ND) algorithm. None when the texts differ by
/// more than `limit` lines.
pub fn diff_lines<S: AsRef<str>>(before: &[S], after: &[S], limit: usize) -> Option<Vec<DiffLine>> {
    let n = before.len() as isize;
    let m = after.len() as isize;
    let max = (n + m).min(limit as isize);
    let offset = max + 1;
    let same = |x: isize, y: isize| before[x as usize].as_ref() == after[y as usize].as_ref();
    let mut v = vec![0isize; (2 * max + 3) as usize];
    let mut trace: Vec<Vec<isize>> = Vec::new();
    let mut found = n == 0 && m == 0;
    let at = |k: isize| (offset + k) as usize;
    let mut d = 0;
    while d <= max && !found {
        trace.push(v.clone());
        let mut k = -d;
        while k <= d {
            let mut x = if k == -d || (k != d && v[at(k - 1)] < v[at(k + 1)]) {
                v[at(k + 1)]
            } else {
                v[at(k - 1)] + 1
            };
            let mut y = x - k;
            while x < n && y < m && same(x, y) {
                x += 1;
                y += 1;
            }
            v[at(k)] = x;
            if x >= n && y >= m {
                found = true;
                break;
            }
            k += 2;
        }
        d += 1;
    }
    if !found {
        return None;
    }
    // Walk the trace back from the end.
    let mut lines = Vec::new();
    let mut x = n;
    let mut y = m;
    for d in (0..trace.len() as isize).rev() {
        let row = &trace[d as usize];
        let k = x - y;
        let previous_k = if k == -d || (k != d && row[at(k - 1)] < row[at(k + 1)]) {
            k + 1
        } else {
            k - 1
        };
        let previous_x = if d == 0 { 0 } else { row[at(previous_k)] };
        let previous_y = previous_x - previous_k;
        while x > previous_x && y > previous_y {
            lines.push(DiffLine {
                kind: DiffKind::Same,
                text: before[(x - 1) as usize].as_ref().to_string(),
                before: Some(x as usize),
                after: Some(y as usize),
            });
            x -= 1;
            y -= 1;
        }
        if d == 0 {
            break;
        }
        if x == previous_x {
            lines.push(DiffLine {
                kind: DiffKind::Add,
                text: after[(y - 1) as usize].as_ref().to_string(),
                before: None,
                after: Some(y as usize),
            });
        } else {
            lines.push(DiffLine {
                kind: DiffKind::Remove,
                text: before[(x - 1) as usize].as_ref().to_string(),
                before: Some(x as usize),
                after: None,
            });
        }
        x = previous_x;
        y = previous_y;
    }
    lines.reverse();
    Some(lines)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffHunk {
    Lines(Vec<DiffLine>),
    Hidden(usize),
}

/// Keep `context` unchanged lines around each change; fold the rest.
/// The TS default context is 3.
pub fn fold_diff(lines: &[DiffLine], context: usize) -> Vec<DiffHunk> {
    let mut keep = vec![false; lines.len()];
    for (index, line) in lines.iter().enumerate() {
        if line.kind == DiffKind::Same {
            continue;
        }
        let end = (index + context).min(lines.len() - 1);
        for flag in &mut keep[index.saturating_sub(context)..=end] {
            *flag = true;
        }
    }
    let mut hunks = Vec::new();
    let mut hidden = 0;
    for (index, line) in lines.iter().enumerate() {
        if !keep[index] {
            hidden += 1;
            continue;
        }
        if hidden > 0 {
            hunks.push(DiffHunk::Hidden(hidden));
        }
        hidden = 0;
        match hunks.last_mut() {
            Some(DiffHunk::Lines(last)) => last.push(line.clone()),
            _ => hunks.push(DiffHunk::Lines(vec![line.clone()])),
        }
    }
    if hidden > 0 {
        hunks.push(DiffHunk::Hidden(hidden));
    }
    hunks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(lines: &[DiffLine]) -> (Vec<String>, Vec<String>) {
        let side = |skip: DiffKind| {
            lines
                .iter()
                .filter(|l| l.kind != skip)
                .map(|l| l.text.clone())
                .collect()
        };
        (side(DiffKind::Add), side(DiffKind::Remove))
    }

    #[test]
    fn finds_a_minimal_edit() {
        let lines = diff_lines(
            &["a", "b", "c", "d"],
            &["a", "x", "c", "d", "e"],
            DIFF_EDIT_LIMIT,
        )
        .unwrap();
        let short: Vec<String> = lines
            .iter()
            .map(|l| {
                let kind = match l.kind {
                    DiffKind::Same => 's',
                    DiffKind::Add => 'a',
                    DiffKind::Remove => 'r',
                };
                format!("{kind}{}", l.text)
            })
            .collect();
        assert_eq!(short, ["sa", "rb", "ax", "sc", "sd", "ae"]);
        assert_eq!(lines[2].after, Some(2));
        assert_eq!(lines[1].before, Some(2));
    }

    #[test]
    fn handles_empty_sides() {
        let none: [&str; 0] = [];
        assert_eq!(diff_lines(&none, &none, DIFF_EDIT_LIMIT), Some(vec![]));
        assert_eq!(
            diff_lines(&none, &["a"], DIFF_EDIT_LIMIT),
            Some(vec![DiffLine {
                kind: DiffKind::Add,
                text: "a".into(),
                before: None,
                after: Some(1)
            }])
        );
        assert_eq!(
            diff_lines(&["a"], &none, DIFF_EDIT_LIMIT),
            Some(vec![DiffLine {
                kind: DiffKind::Remove,
                text: "a".into(),
                before: Some(1),
                after: None
            }])
        );
    }

    #[test]
    fn round_trips_random_edits() {
        let mut seed: u64 = 7;
        let mut random = || {
            seed = (seed * 16807) % 2147483647;
            seed as f64 / 2147483647.0
        };
        for _ in 0..50 {
            let a: Vec<String> = (0..(random() * 30.0) as usize)
                .map(|_| ((random() * 5.0) as u32).to_string())
                .collect();
            let b: Vec<String> = (0..(random() * 30.0) as usize)
                .map(|_| ((random() * 5.0) as u32).to_string())
                .collect();
            let (before, after) = apply(&diff_lines(&a, &b, DIFF_EDIT_LIMIT).unwrap());
            assert_eq!(before, a);
            assert_eq!(after, b);
        }
    }

    #[test]
    fn gives_up_past_the_edit_limit() {
        assert_eq!(diff_lines(&["a", "b"], &["c", "d"], 3), None);
    }

    #[test]
    fn folds_unchanged_runs_outside_the_context() {
        let before: Vec<String> = (0..20).map(|i| i.to_string()).collect();
        let mut after = before.clone();
        after[10] = "x".into();
        let hunks = fold_diff(&diff_lines(&before, &after, DIFF_EDIT_LIMIT).unwrap(), 2);
        assert_eq!(hunks[0], DiffHunk::Hidden(8));
        assert!(matches!(&hunks[1], DiffHunk::Lines(lines) if lines.len() == 6));
        assert_eq!(hunks[2], DiffHunk::Hidden(7));
    }
}
