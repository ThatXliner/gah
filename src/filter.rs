use regex::Regex;

#[cfg(test)]
use crate::diff::DiffLine;
use crate::diff::Hunk;

#[derive(Debug, Clone, Default)]
pub struct HunkFilter {
    pub indices: Option<Vec<usize>>,
    pub anchors: Option<Vec<String>>,
    pub grep: Option<Regex>,
    pub grep_invert: bool,
    pub lines: Option<Vec<(u32, u32)>>,
}

impl HunkFilter {
    pub fn matches(&self, hunk: &Hunk) -> bool {
        // Index filter
        if let Some(ref indices) = self.indices
            && !indices.contains(&hunk.index)
        {
            return false;
        }

        // Anchor filter
        if let Some(ref anchors) = self.anchors {
            let matches = anchors
                .iter()
                .any(|a| hunk.anchor.starts_with(a) || a.starts_with(&hunk.anchor));
            if !matches {
                return false;
            }
        }

        // Grep filter
        if let Some(ref pattern) = self.grep {
            let content = hunk.content();
            let matches = pattern.is_match(&content);
            if self.grep_invert {
                if matches {
                    return false;
                }
            } else if !matches {
                return false;
            }
        }

        // Line range filter (checks if hunk overlaps any specified range)
        if let Some(ref ranges) = self.lines {
            let hunk_start = hunk.new_start;
            let hunk_end = hunk.new_end();

            let overlaps = ranges
                .iter()
                .any(|(start, end)| hunk_start <= *end && hunk_end >= *start);

            if !overlaps {
                return false;
            }
        }

        true
    }
}

pub fn parse_indices(input: &str) -> Result<Vec<usize>, String> {
    let mut result = Vec::new();

    for part in input.split(',') {
        let part = part.trim();
        if part.contains('-') {
            let mut split = part.split('-');
            let start: usize = split
                .next()
                .ok_or("invalid range")?
                .parse()
                .map_err(|_| format!("invalid number in range: {part}"))?;
            let end: usize = split
                .next()
                .ok_or("invalid range")?
                .parse()
                .map_err(|_| format!("invalid number in range: {part}"))?;
            if start > end {
                return Err(format!("invalid range {start}-{end}: start > end"));
            }
            result.extend(start..=end);
        } else {
            let n: usize = part
                .parse()
                .map_err(|_| format!("invalid hunk index: {part}"))?;
            result.push(n);
        }
    }

    Ok(result)
}

pub fn parse_line_ranges(input: &str) -> Result<Vec<(u32, u32)>, String> {
    let mut result = Vec::new();

    for part in input.split(',') {
        let part = part.trim();
        if part.contains('-') {
            let mut split = part.split('-');
            let start: u32 = split
                .next()
                .ok_or("invalid range")?
                .parse()
                .map_err(|_| format!("invalid line number: {part}"))?;
            let end: u32 = split
                .next()
                .ok_or("invalid range")?
                .parse()
                .map_err(|_| format!("invalid line number: {part}"))?;
            if start > end {
                return Err(format!("invalid range {start}-{end}: start > end"));
            }
            result.push((start, end));
        } else {
            let n: u32 = part
                .parse()
                .map_err(|_| format!("invalid line number: {part}"))?;
            result.push((n, n));
        }
    }

    Ok(result)
}

pub fn filter_hunks<'a>(hunks: &'a [Hunk], filter: &HunkFilter) -> Vec<&'a Hunk> {
    hunks.iter().filter(|h| filter.matches(h)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_hunk(index: usize, new_start: u32, new_count: u32, lines: Vec<DiffLine>) -> Hunk {
        let mut hunk = Hunk {
            index,
            anchor: String::new(),
            header: format!("@@ -1,1 +{new_start},{new_count} @@"),
            old_start: 1,
            old_count: 1,
            new_start,
            new_count,
            lines,
            function_context: None,
        };
        hunk.anchor = Hunk::compute_anchor(&hunk.content());
        hunk
    }

    #[test]
    fn test_parse_indices() {
        assert_eq!(parse_indices("1,3,5").unwrap(), vec![1, 3, 5]);
        assert_eq!(parse_indices("1-3").unwrap(), vec![1, 2, 3]);
        assert_eq!(parse_indices("1-3,7").unwrap(), vec![1, 2, 3, 7]);
        assert_eq!(parse_indices("1, 2, 3").unwrap(), vec![1, 2, 3]);
    }

    #[test]
    fn test_parse_line_ranges() {
        assert_eq!(parse_line_ranges("100-150").unwrap(), vec![(100, 150)]);
        assert_eq!(
            parse_line_ranges("100-150,200-250").unwrap(),
            vec![(100, 150), (200, 250)]
        );
        assert_eq!(parse_line_ranges("50").unwrap(), vec![(50, 50)]);
    }

    #[test]
    fn test_filter_by_index() {
        let hunks = vec![
            make_hunk(1, 1, 5, vec![]),
            make_hunk(2, 10, 5, vec![]),
            make_hunk(3, 20, 5, vec![]),
        ];

        let filter = HunkFilter {
            indices: Some(vec![1, 3]),
            ..Default::default()
        };

        let filtered = filter_hunks(&hunks, &filter);
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].index, 1);
        assert_eq!(filtered[1].index, 3);
    }

    #[test]
    fn test_filter_by_grep() {
        let hunks = vec![
            make_hunk(1, 1, 5, vec![DiffLine::Add("function foo()".to_string())]),
            make_hunk(2, 10, 5, vec![DiffLine::Add("function bar()".to_string())]),
        ];

        let filter = HunkFilter {
            grep: Some(Regex::new("foo").unwrap()),
            ..Default::default()
        };

        let filtered = filter_hunks(&hunks, &filter);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].index, 1);
    }

    #[test]
    fn test_filter_by_lines() {
        let hunks = vec![
            make_hunk(1, 1, 10, vec![]),   // lines 1-10
            make_hunk(2, 50, 10, vec![]),  // lines 50-59
            make_hunk(3, 100, 10, vec![]), // lines 100-109
        ];

        let filter = HunkFilter {
            lines: Some(vec![(45, 55)]),
            ..Default::default()
        };

        let filtered = filter_hunks(&hunks, &filter);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].index, 2);
    }
}
