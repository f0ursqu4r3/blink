//! Port of `src/lib/tree-guides.ts`.

use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Elbow {
    /// ├
    Mid,
    /// └
    Last,
}

/// Tree lines for one row of an indented list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeGuide {
    /// Ancestor depths whose line passes through this row, ascending.
    pub through: Vec<usize>,
    /// The row's own elbow.
    pub elbow: Elbow,
}

/// Guides for rows given their levels, in display order. Level 0 rows have
/// none. A depth `d` line connects the children of a row at level `d - 1`.
pub fn tree_guides(levels: &[usize]) -> Vec<Option<TreeGuide>> {
    // continues[d] for row i: a later row at depth d comes before any row
    // shallower than d. Scan from the end.
    let mut result = vec![None; levels.len()];
    let mut open = BTreeSet::new();
    for (i, &level) in levels.iter().enumerate().rev() {
        if level > 0 {
            result[i] = Some(TreeGuide {
                through: open.range(..level).copied().collect(),
                elbow: if open.contains(&level) {
                    Elbow::Mid
                } else {
                    Elbow::Last
                },
            });
        }
        // Rows above see this row: depths deeper than it end here.
        open.retain(|&depth| depth < level);
        if level > 0 {
            open.insert(level);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guide(through: &[usize], elbow: Elbow) -> Option<TreeGuide> {
        Some(TreeGuide {
            through: through.to_vec(),
            elbow,
        })
    }

    #[test]
    fn draws_elbows_and_through_lines() {
        // A            level 0
        //   ├ r1       1
        //   ├ B        1
        //   │  └ r2    2
        //   └ r3       1
        // C            0
        //   └ r4       1
        assert_eq!(
            tree_guides(&[0, 1, 1, 2, 1, 0, 1]),
            [
                None,
                guide(&[], Elbow::Mid),
                guide(&[], Elbow::Mid),
                guide(&[1], Elbow::Last),
                guide(&[], Elbow::Last),
                None,
                guide(&[], Elbow::Last),
            ]
        );
    }

    #[test]
    fn ends_a_line_when_the_list_goes_shallower() {
        assert_eq!(tree_guides(&[0, 1, 2, 0, 1])[2], guide(&[], Elbow::Last));
    }
}
