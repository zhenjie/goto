pub fn match_path(path: &str, query: &str) -> bool {
    let path_lower = path.to_lowercase();
    let query_lower = query.to_lowercase();
    let tokens: Vec<&str> = query_lower.split_whitespace().collect();

    if tokens.is_empty() {
        return true;
    }

    let mut current_pos = 0;

    // Pre-calculate segment boundaries
    let mut segments = Vec::new();
    let mut start = 0;
    for (i, c) in path_lower.char_indices() {
        if c == '/' {
            segments.push((start, i));
            start = i + 1;
        }
    }
    segments.push((start, path_lower.len()));

    for token in tokens {
        let is_start_anchored = token.starts_with('^');
        let is_end_anchored = token.ends_with('$');

        if is_start_anchored || is_end_anchored {
            let mut inner = token;
            if is_start_anchored {
                inner = &inner[1..];
            }
            if is_end_anchored {
                inner = &inner[..inner.len() - 1];
            }

            let mut found = false;
            for &(s_start, s_end) in &segments {
                // Segment must start at or after current_pos
                if s_start < current_pos && !is_start_anchored {
                    // If not start anchored, we might still match inside the segment starting at current_pos
                    // but Requirement 3 says ^ and $ are for segments.
                    // Usually if a token is anchored, it refers to the segment as a whole.
                }

                if s_end < current_pos {
                    continue;
                }

                let segment_text = &path_lower[s_start..s_end];

                let matches = match (is_start_anchored, is_end_anchored) {
                    (true, true) => segment_text == inner,
                    (true, false) => segment_text.starts_with(inner),
                    (false, true) => segment_text.ends_with(inner),
                    _ => unreachable!(),
                };

                if matches {
                    // Update current_pos to after the match within this segment
                    // To be safe and follow "in order", we move to the end of the matched part.
                    if is_start_anchored {
                        current_pos = s_start + inner.len();
                    } else {
                        // end anchored, match is at s_end - inner.len()
                        current_pos = s_end;
                    }
                    found = true;
                    break;
                }
            }

            if !found {
                return false;
            }
        } else {
            // Substring match in the remaining path
            if let Some(pos) = path_lower[current_pos..].find(token) {
                current_pos += pos + token.len();
            } else {
                return false;
            }
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ordered_matching() {
        assert!(match_path("/foo/bar", "fo ba"));
        assert!(!match_path("/bar/foo", "fo ba"));
    }

    #[test]
    fn test_slash_matching() {
        assert!(match_path("/foo/bar", "fo / ba"));
        assert!(!match_path("/foobar", "fo / ba"));
    }

    #[test]
    fn test_anchors() {
        assert!(match_path("/abc/foooooo/bbbbar", "^fo bar$"));
        assert!(!match_path("/foo/barrr", "^fo bar$"));
        assert!(match_path("/foo/bar", "^foo bar$"));
    }

    #[test]
    fn test_case_insensitivity() {
        assert!(match_path("/FOO/BAR", "foo bar"));
    }
}
