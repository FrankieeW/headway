//! Lenient version parsing and ordering for release and tag names.
//!
//! Tags in the wild are `v1.2.3`, `1.2`, `cli-v0.4.0-rc.1`, `2024.10.01` …, so this
//! takes the first run of dot-separated numbers and treats whatever follows as a
//! pre-release suffix (`+build` metadata is ignored, as in semver).

use std::cmp::Ordering;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    numbers: Vec<u64>,
    pre: Option<String>,
}

impl Version {
    pub fn parse(tag: &str) -> Option<Self> {
        let start = tag.find(|c: char| c.is_ascii_digit())?;
        let rest = &tag[start..];
        let mut numbers = Vec::new();
        let mut consumed = 0;
        for (index, part) in rest.split('.').enumerate() {
            let digits: String = part.chars().take_while(char::is_ascii_digit).collect();
            if digits.is_empty() {
                break;
            }
            numbers.push(digits.parse().ok()?);
            consumed += digits.len() + usize::from(index > 0);
            if digits.len() != part.len() {
                break;
            }
        }
        let suffix = rest[consumed..].split('+').next().unwrap_or_default();
        let pre = suffix.trim_start_matches(['-', '.', '_', '~']);
        Some(Self {
            numbers,
            pre: (!pre.is_empty()).then(|| pre.to_owned()),
        })
    }

    pub const fn is_prerelease(&self) -> bool {
        self.pre.is_some()
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        let len = self.numbers.len().max(other.numbers.len());
        let number = |numbers: &[u64], index: usize| numbers.get(index).copied().unwrap_or(0);
        (0..len)
            .map(|index| number(&self.numbers, index).cmp(&number(&other.numbers, index)))
            .find(|ordering| ordering.is_ne())
            .unwrap_or_else(|| match (&self.pre, &other.pre) {
                (None, None) => Ordering::Equal,
                (None, Some(_)) => Ordering::Greater,
                (Some(_), None) => Ordering::Less,
                (Some(a), Some(b)) => compare_pre(a, b),
            })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Semver-style pre-release comparison: numeric identifiers compare numerically
/// and sort before alphanumeric ones; a shorter list of equal identifiers sorts first.
fn compare_pre(a: &str, b: &str) -> Ordering {
    let split = |s: &str| s.split(['.', '-']).map(str::to_owned).collect::<Vec<_>>();
    let (a, b) = (split(a), split(b));
    for (x, y) in a.iter().zip(&b) {
        let ordering = match (x.parse::<u64>(), y.parse::<u64>()) {
            (Ok(x), Ok(y)) => x.cmp(&y),
            (Ok(_), Err(_)) => Ordering::Less,
            (Err(_), Ok(_)) => Ordering::Greater,
            (Err(_), Err(_)) => x.cmp(y),
        };
        if ordering.is_ne() {
            return ordering;
        }
    }
    a.len().cmp(&b.len())
}

/// Orders two tags newest-first friendly: versions compare as versions, tags that
/// don't parse sort below ones that do, and fall back to plain string order.
pub fn compare_tags(a: &str, b: &str) -> Ordering {
    match (Version::parse(a), Version::parse(b)) {
        (Some(x), Some(y)) => x.cmp(&y).then_with(|| a.cmp(b)),
        (Some(_), None) => Ordering::Greater,
        (None, Some(_)) => Ordering::Less,
        (None, None) => a.cmp(b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(tag: &str) -> Version {
        Version::parse(tag).unwrap()
    }

    #[test]
    fn parses_prefixes_and_suffixes() {
        assert_eq!(v("v1.2.3"), v("1.2.3"));
        assert_eq!(v("cli-v0.4.0"), v("0.4.0"));
        assert!(v("v1.0.0-rc.1").is_prerelease());
        assert!(!v("v1.0.0+build.5").is_prerelease());
        assert!(v("2.0.0beta1").is_prerelease());
        assert!(Version::parse("nightly").is_none());
    }

    #[test]
    fn orders_like_semver() {
        let mut tags = vec![
            "v1.10.0",
            "v1.2.0",
            "v1.2.0-rc.2",
            "v1.2.0-rc.10",
            "v1.2.0-beta",
            "v1.2",
            "v0.9.9",
            "v2",
        ];
        tags.sort_by(|a, b| compare_tags(b, a));
        assert_eq!(
            tags,
            vec![
                "v2",
                "v1.10.0",
                "v1.2.0",
                "v1.2",
                "v1.2.0-rc.10",
                "v1.2.0-rc.2",
                "v1.2.0-beta",
                "v0.9.9"
            ]
        );
    }

    #[test]
    fn missing_components_are_zero() {
        assert_eq!(v("1.2").cmp(&v("1.2.0")), Ordering::Equal);
        assert!(v("1.2.0.1") > v("1.2"));
        assert!(v("2024.10.2") > v("2024.9.30"));
    }

    #[test]
    fn unparseable_tags_sort_last() {
        assert_eq!(compare_tags("v1.0", "latest"), Ordering::Greater);
        assert_eq!(compare_tags("nightly", "latest"), Ordering::Greater);
    }
}
