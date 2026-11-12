//! XSD regular expressions (XSD 1.0, Appendix F) for the `pattern` facet.
//!
//! The grammar of XSD differs from the syntax of the `regex` crate:
//! a pattern always matches the whole value (no `^` and `$` operators),
//! `.` excludes only line ends, `\s` and `\d` have their own definitions,
//! and the syntax of the `regex` crate that XSD does not define is an
//! error. The parser accepts the XSD grammar only and writes an
//! anchored `regex` expression. Pattern constructs outside the supported
//! subset are errors, so that a boot fails and no value passes by
//! accident. Compilation has limits for the size of the pattern and of
//! the automaton.

use regex::Regex;

/// Compiles an XSD pattern into a regular expression that matches
/// the whole value.
pub fn compile(_pattern: &str) -> Result<Regex, String> {
    Err("not implemented".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn re(p: &str) -> Regex {
        compile(p).unwrap_or_else(|e| panic!("{p:?} must compile: {e}"))
    }

    fn matches(p: &str, v: &str) -> bool {
        re(p).is_match(v)
    }

    fn refused(p: &str) {
        assert!(compile(p).is_err(), "{p:?} must be refused");
    }

    #[test]
    fn a_pattern_matches_the_whole_value() {
        assert!(matches("ab", "ab"));
        assert!(!matches("ab", "xab"));
        assert!(!matches("ab", "abx"));
        assert!(!matches("ab", "ab\n"));
        assert!(matches("", ""));
        assert!(!matches("", "a"));
        assert!(matches("a|", "a") && matches("a|", ""));
    }

    #[test]
    fn caret_and_dollar_are_literal_characters() {
        assert!(matches("^a$", "^a$"));
        assert!(!matches("^a$", "a"));
    }

    #[test]
    fn dot_excludes_only_line_ends() {
        assert!(matches(".", "x") && matches(".", "\u{e4}") && matches(".", "\t"));
        assert!(!matches(".", "\n") && !matches(".", "\r"));
        assert!(matches(".{19}Z", "2026-11-12T10:00:00Z"));
        assert!(!matches(".{19}Z", "2026-11-12T10:00:00"));
    }

    #[test]
    fn backslash_s_is_the_four_xml_white_space_characters() {
        for c in [" ", "\t", "\n", "\r"] {
            assert!(matches(r"\s", c));
        }
        for c in ["\u{a0}", "\u{2003}", "\u{c}", "a"] {
            assert!(!matches(r"\s", c), "{c:?}");
        }
        assert!(matches(r"\S", "a") && !matches(r"\S", " "));
    }

    #[test]
    fn backslash_d_is_the_decimal_digit_category() {
        assert!(matches(r"\d", "5") && matches(r"\d", "\u{663}"));
        assert!(!matches(r"\d", "a"));
        assert!(matches(r"\d{4}-\d{2}-\d{2}", "2026-11-12"));
        assert!(!matches(r"\d{4}-\d{2}-\d{2}", "2026-1-12"));
    }

    #[test]
    fn backslash_w_excludes_punctuation_separators_and_other() {
        assert!(matches(r"\w+", "ab\u{e9}1"));
        for c in [" ", ".", "-", "\n", "\u{2003}"] {
            assert!(!matches(r"\w", c), "{c:?}");
        }
        assert!(matches(r"\W", "-") && !matches(r"\W", "a"));
    }

    #[test]
    fn character_classes() {
        assert!(matches("[0-9a-fA-F]{8}", "09afAF00"));
        assert!(!matches("[0-9a-fA-F]{8}", "09afAG00"));
        assert!(matches("[^abc]", "d") && !matches("[^abc]", "b"));
        assert!(matches("[a-]", "-") && matches("[a-]", "a"));
        assert!(matches("[-a]", "-"));
        assert!(matches(r"[\n\t]", "\t") && matches(r"[\\]", "\\") && matches(r"[\-]", "-"));
        assert!(matches(r"[\d_]", "_") && matches(r"[\d_]", "7"));
        assert!(matches("[^^]", "a") && !matches("[^^]", "^"));
        assert!(matches("[a^]", "^"));
        assert!(matches(r"[\p{Lu}0-9]", "\u{c4}"));
    }

    #[test]
    fn regex_crate_class_operators_are_literal_characters_in_xsd() {
        assert!(matches("[a&&b]", "&") && matches("[a&&b]", "a") && matches("[a&&b]", "b"));
        assert!(matches("[a~~b]", "~"));
        assert!(matches("[a|b]", "|"));
    }

    #[test]
    fn groups_alternation_and_quantifiers() {
        assert!(matches("(a|b)+c", "abbac") && !matches("(a|b)+c", "c"));
        assert!(matches("a{2,3}", "aa") && matches("a{2,3}", "aaa"));
        assert!(!matches("a{2,3}", "a") && !matches("a{2,3}", "aaaa"));
        assert!(matches("a{2}", "aa") && matches("a{2,}", "aaaaa"));
        assert!(matches("a?b*c+", "c") && matches("a?b*c+", "abbcc"));
        assert!(matches(r"\.\?\*\+\{\}\(\)\|\[\]\^\$", ".?*+{}()|[]^$"));
    }

    #[test]
    fn general_categories() {
        assert!(matches(r"\p{Lu}+", "\u{c4}B") && !matches(r"\p{Lu}", "a"));
        assert!(matches(r"\P{Lu}", "a") && !matches(r"\P{Lu}", "A"));
        assert!(matches(r"\p{N}", "5") && matches(r"\p{Sc}", "$"));
    }

    #[test]
    fn the_erru_patterns_work() {
        assert!(matches(
            "[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}",
            "123e4567-e89b-12d3-a456-426614174000"
        ));
        assert!(matches("[A-HJ-NPR-Z0-9]{17}", "WVWZZZ1JZXW000001"));
    }

    #[test]
    fn syntax_that_only_the_regex_crate_knows_is_refused() {
        for p in [
            "(?i)a",
            "(?:a)",
            "\\b",
            "\\B",
            "\\A",
            "\\z",
            "a*?",
            "a+?",
            "a{2,}?",
            "\\x41",
            "\\u0041",
            "\\x{41}",
            "[[:alpha:]]",
            "(?P<n>a)",
            "\\1",
            "\\pL",
            "a**",
            "a{,3}",
            "\\e",
            "\\a",
            "\\f",
            "\\v",
            "[a&&b]",
            "[a~~b]",
        ] {
            refused(p);
        }
    }

    #[test]
    fn constructs_that_xsd_defines_but_the_subset_leaves_out_are_refused() {
        for p in [
            r"\i",
            r"\I",
            r"\c",
            r"\C",
            r"\p{IsBasicLatin}",
            r"\P{IsGreek}",
            "[a-z-[aeiou]]",
            r"[\d-[5]]",
            r"\p{Foo}",
            r"\p{}",
        ] {
            refused(p);
        }
    }

    #[test]
    fn malformed_patterns_are_refused() {
        for p in [
            "(a", "a)", "[a", "a]", "[]", "[^]", "a{", "a}", "a{3,2}", "a{x}", "*a", "+a", "?a",
            "{2}", "|*", "[z-a]", "a\\", "\\", "[a-b-c]",
        ] {
            refused(p);
        }
    }

    #[test]
    fn limits_stop_huge_patterns() {
        refused(&"a".repeat(5_000));
        refused(&format!("{}a{}", "(".repeat(100), ")".repeat(100)));
        refused("a{1,100000}");
        refused("(a{1000}){1000}");
        refused(r"\p{L}{1000}{1000}");
    }
}
