//! Validation of one parsed message against the model.
//!
//! The walk is deterministic. The UPA check of the model guarantees
//! that the name of the next element decides which particle matches.
//! A violation names the element path, a rule code and the position in
//! the schema. It never holds a value from the message.

use super::model::Model;
use crate::inbound::dom::Element;

/// The most violations that one message reports.
pub const MAX_VIOLATIONS: usize = 10;

/// One rule that a message breaks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    /// Element path, for example `/Msg/Body/Item[2]/@code`.
    pub path: String,
    /// Stable rule code, for example `enumeration`.
    pub code: &'static str,
    /// Line of the element in the message (0 = unknown).
    pub line: u32,
    /// Schema file, relative to the schema directory.
    pub schema_file: String,
    /// Line of the rule in the schema file (0 = unknown).
    pub schema_line: u32,
}

/// Validates `root`. The list is empty when the message is valid. It
/// holds at most [`MAX_VIOLATIONS`] entries.
pub fn validate(_model: &Model, _root: &Element) -> Vec<Violation> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inbound::dom;
    use std::path::PathBuf;

    const NS: &str = "urn:t";

    fn no_files(_: &str) -> Option<(String, PathBuf, String)> {
        None
    }

    fn model(types_and_root: &str) -> Model {
        let xsd = format!(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" xmlns:t="{NS}" targetNamespace="{NS}" elementFormDefault="qualified">{types_and_root}</xs:schema>"#
        );
        super::super::build(&[("m.xsd".into(), xsd)], &no_files, &["Msg"])
            .unwrap_or_else(|p| panic!("model must build: {p:?}"))
    }

    fn check(m: &Model, xml: &str) -> Vec<Violation> {
        let root = dom::parse_message(xml).expect("message must parse");
        validate(m, &root)
    }

    fn codes(m: &Model, xml: &str) -> Vec<&'static str> {
        check(m, xml).iter().map(|v| v.code).collect()
    }

    /// A message root in the schema namespace.
    fn doc(inner: &str) -> String {
        format!(r#"<Msg xmlns="{NS}">{inner}</Msg>"#)
    }

    const SIMPLE: &str = r#"
      <xs:simpleType name="Code"><xs:restriction base="xs:token">
        <xs:whiteSpace value="collapse"/><xs:enumeration value="A"/><xs:enumeration value="B C"/>
      </xs:restriction></xs:simpleType>
      <xs:element name="Msg"><xs:complexType><xs:sequence>
        <xs:element name="code" type="t:Code"/>
        <xs:element name="n" type="xs:nonNegativeInteger" minOccurs="0"/>
      </xs:sequence></xs:complexType></xs:element>"#;

    #[test]
    fn a_valid_message_has_no_violations() {
        let m = model(SIMPLE);
        assert!(check(&m, &doc("<code>A</code><n>5</n>")).is_empty());
        assert!(check(&m, &doc("<code>A</code>")).is_empty());
    }

    #[test]
    fn a_root_that_the_model_does_not_know_is_a_violation() {
        let m = model(SIMPLE);
        assert_eq!(
            codes(&m, r#"<Other xmlns="urn:t"><code>A</code></Other>"#),
            ["root_unknown"]
        );
        assert_eq!(
            codes(&m, r#"<Msg xmlns="urn:other"><code>A</code></Msg>"#),
            ["root_unknown"]
        );
    }

    #[test]
    fn a_missing_required_element_is_a_violation() {
        let m = model(SIMPLE);
        let v = check(&m, &doc("<n>5</n>"));
        assert_eq!(
            v.iter().map(|v| v.code).collect::<Vec<_>>(),
            ["element_missing"]
        );
        assert_eq!(v[0].path, "/Msg");
        assert_eq!(v[0].schema_file, "m.xsd");
        assert!(v[0].schema_line > 0);
    }

    #[test]
    fn an_unexpected_element_or_a_wrong_order_is_a_violation() {
        let m = model(SIMPLE);
        let v = check(&m, &doc("<code>A</code><x/>"));
        assert_eq!(v[0].code, "element_unexpected");
        assert_eq!(v[0].path, "/Msg/x");
        assert_eq!(
            codes(&m, &doc("<n>1</n><code>A</code>"))[0],
            "element_unexpected"
        );
        assert_eq!(
            codes(&m, &doc("<code>A</code><n>1</n><n>2</n>"))[0],
            "element_unexpected"
        );
        assert_eq!(
            codes(
                &m,
                r#"<Msg xmlns="urn:t"><code xmlns="urn:other">A</code></Msg>"#
            )[0],
            "element_missing"
        );
    }

    #[test]
    fn occurrence_limits_are_checked() {
        let m = model(
            r#"<xs:element name="Msg"><xs:complexType><xs:sequence>
                 <xs:element name="i" type="xs:string" minOccurs="2" maxOccurs="3"/>
               </xs:sequence></xs:complexType></xs:element>"#,
        );
        assert_eq!(codes(&m, &doc("<i>a</i>")), ["element_missing"]);
        assert!(check(&m, &doc("<i>a</i><i>b</i>")).is_empty());
        assert!(check(&m, &doc("<i>a</i><i>b</i><i>c</i>")).is_empty());
        assert_eq!(
            codes(&m, &doc("<i>a</i><i>b</i><i>c</i><i>d</i>"))[0],
            "element_unexpected"
        );
    }

    #[test]
    fn a_choice_takes_exactly_one_branch() {
        let m = model(
            r#"<xs:element name="Msg"><xs:complexType><xs:sequence>
                 <xs:choice>
                   <xs:element name="a" type="xs:string"/>
                   <xs:sequence><xs:element name="b" type="xs:string"/><xs:element name="c" type="xs:string"/></xs:sequence>
                 </xs:choice>
                 <xs:element name="end" type="xs:string"/>
               </xs:sequence></xs:complexType></xs:element>"#,
        );
        assert!(check(&m, &doc("<a>1</a><end>e</end>")).is_empty());
        assert!(check(&m, &doc("<b>1</b><c>2</c><end>e</end>")).is_empty());
        assert!(!check(&m, &doc("<end>e</end>")).is_empty());
        assert!(!check(&m, &doc("<a>1</a><b>1</b><c>2</c><end>e</end>")).is_empty());
        assert!(!check(&m, &doc("<b>1</b><end>e</end>")).is_empty());
    }

    #[test]
    fn a_repeated_group_matches_each_repetition() {
        let m = model(
            r#"<xs:element name="Msg"><xs:complexType><xs:sequence maxOccurs="unbounded">
                 <xs:element name="k" type="xs:string"/><xs:element name="v" type="xs:string" minOccurs="0"/>
               </xs:sequence></xs:complexType></xs:element>"#,
        );
        assert!(check(&m, &doc("<k>1</k><v>a</v><k>2</k><k>3</k><v>c</v>")).is_empty());
        assert!(!check(&m, &doc("<v>a</v>")).is_empty());
        assert!(!check(&m, &doc("")).is_empty());
    }

    #[test]
    fn extension_content_is_the_base_content_then_the_own_content() {
        let m = model(
            r#"<xs:complexType name="Base"><xs:sequence><xs:element name="a" type="xs:string"/></xs:sequence>
                 <xs:attribute name="id" type="xs:string" use="required"/></xs:complexType>
               <xs:element name="Msg"><xs:complexType><xs:complexContent>
                 <xs:extension base="t:Base"><xs:sequence><xs:element name="b" type="xs:string"/></xs:sequence>
                   <xs:attribute name="x" type="xs:string"/></xs:extension>
               </xs:complexContent></xs:complexType></xs:element>"#,
        );
        let ok = format!(r#"<Msg xmlns="{NS}" id="1" x="2"><a>1</a><b>2</b></Msg>"#);
        assert!(check(&m, &ok).is_empty());
        let wrong = format!(r#"<Msg xmlns="{NS}" id="1"><b>2</b><a>1</a></Msg>"#);
        assert!(!check(&m, &wrong).is_empty());
        let no_id = format!(r#"<Msg xmlns="{NS}"><a>1</a><b>2</b></Msg>"#);
        assert_eq!(codes(&m, &no_id), ["attribute_missing"]);
    }

    fn typed(ty: &str) -> Model {
        model(&format!(
            r#"{ty}<xs:element name="Msg"><xs:complexType><xs:sequence>
               <xs:element name="v" type="t:V"/></xs:sequence></xs:complexType></xs:element>"#
        ))
    }

    fn value_codes(m: &Model, v: &str) -> Vec<&'static str> {
        codes(m, &doc(&format!("<v>{v}</v>")))
    }

    #[test]
    fn enumeration_and_white_space_apply_in_this_order() {
        let m = typed(
            SIMPLE
                .split("<xs:element")
                .next()
                .unwrap()
                .replace("\"Code\"", "\"V\"")
                .as_str(),
        );
        assert!(value_codes(&m, "A").is_empty());
        assert!(value_codes(&m, "  B   C  ").is_empty());
        assert_eq!(value_codes(&m, "B  D"), ["enumeration"]);
    }

    #[test]
    fn pattern_and_length_facets() {
        let m = typed(
            r#"<xs:simpleType name="V"><xs:restriction base="xs:string">
                 <xs:pattern value="[A-Z]{2}\d{3}"/><xs:pattern value="XX.*"/>
                 <xs:minLength value="2"/><xs:maxLength value="5"/>
               </xs:restriction></xs:simpleType>"#,
        );
        assert!(value_codes(&m, "AB123").is_empty());
        assert!(
            value_codes(&m, "XX").is_empty(),
            "patterns of one step are OR"
        );
        assert_eq!(value_codes(&m, "ab123"), ["pattern"]);
        assert_eq!(value_codes(&m, "XXXXXX"), ["max_length"]);
        assert_eq!(value_codes(&m, "X"), ["pattern", "min_length"]);
        let l = typed(
            r#"<xs:simpleType name="V"><xs:restriction base="xs:string"><xs:length value="3"/></xs:restriction></xs:simpleType>"#,
        );
        assert!(
            value_codes(&l, "a\u{e4}\u{20ac}").is_empty(),
            "length counts characters"
        );
        assert_eq!(value_codes(&l, "ab"), ["length"]);
    }

    #[test]
    fn patterns_of_different_derivation_steps_are_and() {
        let m = typed(
            r#"<xs:simpleType name="B"><xs:restriction base="xs:string"><xs:pattern value="[a-z]+"/></xs:restriction></xs:simpleType>
               <xs:simpleType name="V"><xs:restriction base="t:B"><xs:pattern value=".{3}"/></xs:restriction></xs:simpleType>"#,
        );
        assert!(value_codes(&m, "abc").is_empty());
        assert_eq!(value_codes(&m, "abcd"), ["pattern"]);
        assert_eq!(value_codes(&m, "ab1"), ["pattern"]);
    }

    #[test]
    fn built_in_types_check_the_lexical_form() {
        let cases: [(&str, &[&str], &[&str]); 6] = [
            (
                "boolean",
                &["true", "false", "1", "0", " true "],
                &["yes", "TRUE", ""],
            ),
            (
                "nonNegativeInteger",
                &["0", "7", "+5", "-0", "007"],
                &["-1", "1.5", "a", ""],
            ),
            (
                "positiveInteger",
                &["1", "+5", "0010"],
                &["0", "-1", "00", ""],
            ),
            (
                "date",
                &[
                    "2026-11-12",
                    "2024-02-29",
                    "2026-11-12Z",
                    "2026-11-12+02:00",
                    "-0044-03-15",
                ],
                &[
                    "2026-02-30",
                    "2025-02-29",
                    "2026-13-01",
                    "26-11-12",
                    "2026-11-12T10:00:00Z",
                    "2026-11-12+15:00",
                ],
            ),
            (
                "dateTime",
                &[
                    "2026-11-12T10:00:00Z",
                    "2026-11-12T10:00:00.123+02:00",
                    "2026-11-12T24:00:00",
                    "2026-11-12T10:00:00",
                ],
                &[
                    "2026-11-12",
                    "2026-11-12T25:00:00",
                    "2026-11-12T10:00",
                    "2026-11-12T24:00:01",
                    "2026-11-12 10:00:00Z",
                ],
            ),
            ("string", &["anything", ""], &[]),
        ];
        for (ty, good, bad) in cases {
            let m = typed(&format!(
                r#"<xs:simpleType name="V"><xs:restriction base="xs:{ty}"/></xs:simpleType>"#
            ));
            for v in good {
                assert!(value_codes(&m, v).is_empty(), "{ty}: {v:?} must be valid");
            }
            for v in bad {
                assert_eq!(
                    value_codes(&m, v),
                    ["type_invalid"],
                    "{ty}: {v:?} must be invalid"
                );
            }
        }
    }

    #[test]
    fn min_inclusive_compares_by_value() {
        let m = typed(
            r#"<xs:simpleType name="V"><xs:restriction base="xs:dateTime">
                 <xs:minInclusive value="1753-01-01T00:00:00Z"/></xs:restriction></xs:simpleType>"#,
        );
        assert!(value_codes(&m, "1753-01-01T00:00:00Z").is_empty());
        assert!(value_codes(&m, "1753-01-01T02:00:00+02:00").is_empty());
        assert!(value_codes(&m, "2026-01-01T00:00:00").is_empty());
        assert_eq!(value_codes(&m, "1752-12-31T23:59:59Z"), ["min_inclusive"]);
        assert_eq!(
            value_codes(&m, "1753-01-01T00:00:00+02:00"),
            ["min_inclusive"]
        );
        let n = typed(
            r#"<xs:simpleType name="V"><xs:restriction base="xs:positiveInteger">
                 <xs:minInclusive value="10"/></xs:restriction></xs:simpleType>"#,
        );
        assert!(value_codes(&n, "10").is_empty() && value_codes(&n, "100").is_empty());
        assert_eq!(value_codes(&n, "9"), ["min_inclusive"]);
    }

    #[test]
    fn attributes_are_checked() {
        let m = model(
            r#"<xs:element name="Msg"><xs:complexType><xs:sequence/>
                 <xs:attribute name="req" type="xs:string" use="required"/>
                 <xs:attribute name="ver" type="xs:string" fixed="3.5"/>
                 <xs:attribute name="n" type="xs:nonNegativeInteger"/>
               </xs:complexType></xs:element>"#,
        );
        let msg = |a: &str| format!(r#"<Msg xmlns="{NS}" {a}/>"#);
        assert!(check(&m, &msg(r#"req="x" ver="3.5" n="1""#)).is_empty());
        assert!(check(&m, &msg(r#"req="x""#)).is_empty());
        assert_eq!(codes(&m, &msg("")), ["attribute_missing"]);
        assert_eq!(codes(&m, &msg(r#"req="x" ver="3.6""#)), ["attribute_fixed"]);
        assert_eq!(codes(&m, &msg(r#"req="x" n="-1""#)), ["type_invalid"]);
        assert_eq!(
            codes(&m, &msg(r#"req="x" other="1""#)),
            ["attribute_unexpected"]
        );
        let v = check(&m, &msg(r#"req="x" other="1""#));
        assert_eq!(v[0].path, "/Msg/@other");
    }

    #[test]
    fn xsi_type_and_nil_are_refused_and_schema_location_is_ignored() {
        let m = model(
            r#"<xs:element name="Msg"><xs:complexType><xs:sequence/></xs:complexType></xs:element>"#,
        );
        let xsi = r#"xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance""#;
        let msg = |a: &str| format!(r#"<Msg xmlns="{NS}" {xsi} {a}/>"#);
        assert_eq!(
            codes(&m, &msg(r#"xsi:type="t:Other""#)),
            ["xsi_not_allowed"]
        );
        assert_eq!(codes(&m, &msg(r#"xsi:nil="true""#)), ["xsi_not_allowed"]);
        assert!(check(&m, &msg(r#"xsi:schemaLocation="urn:t http://x/y.xsd""#)).is_empty());
        assert!(check(&m, &msg(r#"xsi:noNamespaceSchemaLocation="y.xsd""#)).is_empty());
    }

    #[test]
    fn text_is_valid_only_where_the_type_allows_it() {
        let m = model(
            r#"<xs:element name="Msg"><xs:complexType><xs:sequence>
                 <xs:element name="a" type="xs:string"/>
                 <xs:element name="e"><xs:complexType/></xs:element>
               </xs:sequence></xs:complexType></xs:element>"#,
        );
        assert!(check(&m, &doc("\n  <a>x</a>\n  <e/>\n")).is_empty());
        assert_eq!(codes(&m, &doc("text<a>x</a><e/>")), ["text_not_allowed"]);
        assert_eq!(codes(&m, &doc("<a>x</a><e>text</e>")), ["text_not_allowed"]);
        assert_eq!(
            codes(&m, &doc("<a><b/></a><e/>")),
            ["simple_content_expected"]
        );
    }

    #[test]
    fn a_violation_never_holds_a_value_or_an_absolute_path() {
        let m = typed(
            r#"<xs:simpleType name="V"><xs:restriction base="xs:token"><xs:enumeration value="A"/></xs:restriction></xs:simpleType>"#,
        );
        let v = check(&m, &doc("<v>SECRET-4711</v>"));
        assert_eq!(v.len(), 1);
        let text = format!("{v:?}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!v[0].schema_file.contains('/'), "{}", v[0].schema_file);
        let a = model(
            r#"<xs:element name="Msg"><xs:complexType><xs:sequence/></xs:complexType></xs:element>"#,
        );
        let v = check(&a, &format!(r#"<Msg xmlns="{NS}" SECRET-4711="1"/>"#));
        assert_eq!(v[0].code, "attribute_unexpected");
        assert!(!v[0].path.contains("SECRET-4711") || v[0].path.ends_with("@SECRET-4711"));
    }

    #[test]
    fn repeated_siblings_get_an_index_in_the_path() {
        let m = model(
            r#"<xs:element name="Msg"><xs:complexType><xs:sequence>
                 <xs:element name="i" maxOccurs="unbounded"><xs:complexType><xs:sequence/>
                   <xs:attribute name="c" type="xs:nonNegativeInteger"/></xs:complexType></xs:element>
               </xs:sequence></xs:complexType></xs:element>"#,
        );
        let v = check(&m, &doc(r#"<i c="1"/><i c="x"/>"#));
        assert_eq!(v[0].path, "/Msg/i[2]/@c");
        let v = check(&m, &doc(r#"<i c="x"/>"#));
        assert_eq!(v[0].path, "/Msg/i/@c");
    }

    #[test]
    fn a_message_reports_at_most_ten_violations() {
        let m = model(
            r#"<xs:element name="Msg"><xs:complexType><xs:sequence>
                 <xs:element name="i" type="xs:nonNegativeInteger" maxOccurs="unbounded"/>
               </xs:sequence></xs:complexType></xs:element>"#,
        );
        let items = "<i>x</i>".repeat(50);
        assert_eq!(check(&m, &doc(&items)).len(), MAX_VIOLATIONS);
    }

    #[test]
    fn the_message_line_is_reported() {
        let m = typed(
            r#"<xs:simpleType name="V"><xs:restriction base="xs:token"><xs:enumeration value="A"/></xs:restriction></xs:simpleType>"#,
        );
        let v = check(&m, &doc("\n\n<v>B</v>"));
        assert_eq!(v[0].line, 3);
    }
}
