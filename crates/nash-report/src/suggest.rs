//! Elm's `Reporting/Suggest.hs`: near-miss name suggestions.

/// Restricted Damerau-Levenshtein (optimal string alignment) distance.
pub fn distance(x: &str, y: &str) -> usize {
    let a: Vec<char> = x.chars().collect();
    let b: Vec<char> = y.chars().collect();
    let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, value) in d[0].iter_mut().enumerate() {
        *value = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            d[i][j] = (d[i - 1][j] + 1)
                .min(d[i][j - 1] + 1)
                .min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
            }
        }
    }
    d[a.len()][b.len()]
}

/// Elm `sort`: candidates ordered by distance to `target`, case-insensitive.
/// Equal distances retain their input order.
pub fn sort<T>(target: &str, to_string: impl Fn(&T) -> String, mut values: Vec<T>) -> Vec<T> {
    let target = to_lower(target);
    values.sort_by_cached_key(|value| distance(&target, &to_lower(&to_string(value))));
    values
}

/// Elm `rank`: stable candidate ordering with each candidate's distance.
pub fn rank<T>(target: &str, to_string: impl Fn(&T) -> String, values: Vec<T>) -> Vec<(usize, T)> {
    let target = to_lower(target);
    let mut ranked: Vec<_> = values
        .into_iter()
        .map(|value| (distance(&target, &to_lower(&to_string(&value))), value))
        .collect();
    ranked.sort_by_key(|(distance, _)| *distance);
    ranked
}

/// Haskell's `map Char.toLower` maps each scalar to one scalar, without
/// contextual final sigma or the full lowercase expansion of dotted capital I.
fn to_lower(string: &str) -> String {
    string
        .chars()
        .map(|c| c.to_lowercase().next().unwrap_or(c))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_transposition_is_one() {
        assert_eq!(distance("ab", "ba"), 1);
    }

    #[test]
    fn distance_empty() {
        assert_eq!(distance("", ""), 0);
        assert_eq!(distance("", "abc"), 3);
        assert_eq!(distance("abc", ""), 3);
    }

    #[test]
    fn distance_is_restricted_and_case_sensitive() {
        // Unrestricted Damerau-Levenshtein would give 2 here.
        assert_eq!(distance("CA", "ABC"), 3);
        assert_eq!(distance("A", "a"), 1);
        assert_eq!(distance("kitten", "sitting"), 3);
        assert_eq!(distance("same", "same"), 0);
    }

    #[test]
    fn distance_counts_unicode_scalars() {
        assert_eq!(distance("", "é🦀"), 2);
        assert_eq!(distance("é🦀", "🦀é"), 1);
    }

    #[test]
    fn sort_prefers_case_insensitive_match() {
        let values = vec!["height", "len", "LENGTH"];
        assert_eq!(
            sort("lenght", |s| (*s).into(), values),
            ["LENGTH", "height", "len"]
        );
    }

    #[test]
    fn rank_keeps_stable_order_for_ties() {
        let values = vec!["bat", "hat", "CAT", "mat"];
        assert_eq!(
            rank("cat", |s| (*s).into(), values.clone()),
            [(0, "CAT"), (1, "bat"), (1, "hat"), (1, "mat")]
        );
        assert_eq!(
            sort("cat", |s| (*s).into(), values),
            ["CAT", "bat", "hat", "mat"]
        );
    }

    #[test]
    fn lowercase_matches_elms_character_mapping() {
        assert_eq!(rank("İ", |s| (*s).into(), vec!["i"]), [(0, "i")]);
        assert_eq!(
            rank("ΟΣ", |s| (*s).into(), vec!["οσ", "ος"]),
            [(0, "οσ"), (1, "ος")]
        );
    }

    #[test]
    fn sort_and_rank_accept_empty_candidates() {
        assert!(sort::<String>("target", Clone::clone, vec![]).is_empty());
        assert!(rank::<String>("target", Clone::clone, vec![]).is_empty());
    }
}
