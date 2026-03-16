use nucleo_matcher::{
    pattern::{AtomKind, CaseMatching, Normalization, Pattern},
    Config, Matcher, Utf32Str,
};

pub struct FuzzyScorer {
    matcher: Matcher,
    pattern: Pattern,
}

impl FuzzyScorer {
    pub fn new(query: &str, case_sensitive: bool) -> Self {
        let matcher = Matcher::new(Config::DEFAULT);
        let case = if case_sensitive { CaseMatching::Respect } else { CaseMatching::Ignore };
        let pattern = Pattern::new(
            query,
            case,
            Normalization::Smart,
            AtomKind::Fuzzy,
        );
        Self { matcher, pattern }
    }

    /// Returns Some(score) if `haystack` matches, None otherwise.
    pub fn score(&mut self, haystack: &str) -> Option<u32> {
        let mut buf = Vec::new();
        let s = Utf32Str::new(haystack, &mut buf);
        self.pattern.score(s, &mut self.matcher)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match_scores() {
        let mut scorer = FuzzyScorer::new("hello", false);
        assert!(scorer.score("hello").is_some());
    }

    #[test]
    fn non_match_returns_none() {
        let mut scorer = FuzzyScorer::new("zzz", false);
        assert!(scorer.score("hello world").is_none());
    }

    #[test]
    fn prefix_scores_higher_than_partial() {
        let mut scorer = FuzzyScorer::new("rep", false);
        let prefix_score = scorer.score("report.pdf").expect("prefix should match");
        let partial_score = scorer.score("unrelated_rep_file").expect("partial should match");
        assert!(prefix_score > partial_score);
    }

    #[test]
    fn empty_query_scores_everything() {
        let mut scorer = FuzzyScorer::new("", false);
        assert!(scorer.score("anything").is_some());
    }

    #[test]
    fn case_insensitive_matches() {
        let mut scorer = FuzzyScorer::new("Hello", false);
        assert!(scorer.score("hello world").is_some());
    }

    #[test]
    fn case_sensitive_no_match() {
        let mut scorer = FuzzyScorer::new("Hello", true);
        assert!(scorer.score("hello world").is_none());
    }

    #[test]
    fn multiple_calls_consistent() {
        let mut scorer = FuzzyScorer::new("foo", false);
        let first = scorer.score("foobar");
        let second = scorer.score("foobar");
        assert_eq!(first, second);
    }
}
