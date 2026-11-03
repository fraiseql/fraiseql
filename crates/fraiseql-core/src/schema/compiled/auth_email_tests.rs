#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;
use crate::schema::LocalAuthConfig;

fn body(text: &str) -> EmailTemplate {
    EmailTemplate {
        text: Some(text.to_string()),
        ..EmailTemplate::default()
    }
}

#[test]
fn render_substitutes_once_and_never_reads_a_value_again() {
    let rendered = render_email_template(
        "{code} {link} {code}",
        &[("code", "{link}"), ("link", "https://x/?a=1&b={code}")],
        false,
    );
    assert_eq!(rendered, "{link} https://x/?a=1&b={code} {link}");
}

#[test]
fn render_escapes_values_in_html_and_leaves_the_template_alone() {
    let rendered = render_email_template(
        "<a href=\"{link}\">{code}</a>",
        &[("link", "https://x/?a=1&b=\"<'>"), ("code", "123456")],
        true,
    );
    assert_eq!(rendered, "<a href=\"https://x/?a=1&amp;b=&quot;&lt;&#39;&gt;\">123456</a>");
}

/// A link value is percent-encoded, so an address or any value with `+ & = # ? %` or a space
/// cannot change the link's meaning.
#[test]
fn a_link_value_is_percent_encoded() {
    let link =
        render_link_template("https://x/v?t={token}&n=1", &[("token", "a+b c&d=e#f?g%h/é~ok-._")]);
    assert_eq!(link, "https://x/v?t=a%2Bb%20c%26d%3De%23f%3Fg%25h%2F%C3%A9~ok-._&n=1");
}

#[test]
fn render_keeps_braces_that_are_not_placeholders() {
    let rendered = render_email_template("a {color: red} {} {9x} {code}", &[("code", "1")], false);
    assert_eq!(rendered, "a {color: red} {} {9x} 1");
}

#[test]
fn a_sign_in_body_needs_its_code_or_a_link_it_can_build() {
    assert!(body("{code}").check(EmailKind::Otp, false).is_ok());
    assert!(body("{code} {9x} {color: red}").check(EmailKind::Otp, false).is_ok());
    assert!(body("{link}").check(EmailKind::Otp, true).is_ok());
    let none = body("hello").check(EmailKind::Otp, true).unwrap_err();
    assert!(none.contains("{code}") && none.contains("{link}"), "{none}");
    let no_template = body("{link}").check(EmailKind::Otp, false).unwrap_err();
    assert!(no_template.contains("magic_link_template"), "{no_template}");
}

#[test]
fn reset_and_verification_bodies_need_their_link_and_have_no_code() {
    for kind in [EmailKind::Reset, EmailKind::Verification] {
        assert!(body("{link} {expires_minutes}").check(kind, false).is_ok());
        assert!(body("{expires_minutes}").check(kind, false).unwrap_err().contains("{link}"));
        assert!(body("{link} {code}").check(kind, false).unwrap_err().contains("{code}"));
    }
}

#[test]
fn an_html_body_is_checked_like_the_text() {
    let template = EmailTemplate {
        text: Some("{code}".into()),
        html: Some("<p>{cdoe}</p>".into()),
        ..EmailTemplate::default()
    };
    let err = template.check(EmailKind::Otp, false).unwrap_err();
    assert!(err.contains("html_template") && err.contains("{cdoe}"), "{err}");
}

#[test]
fn a_subject_is_one_literal_line() {
    let subject = |s: &str| EmailTemplate {
        subject: Some(s.to_string()),
        ..EmailTemplate::default()
    };
    assert!(subject("Votre code").check(EmailKind::Otp, false).is_ok());
    assert!(
        subject("Code {code}")
            .check(EmailKind::Otp, false)
            .unwrap_err()
            .contains("subject")
    );
    for broken in ["a\nBcc: x@example.com", "a\rBcc: x@example.com"] {
        let err = subject(broken).check(EmailKind::Otp, false).unwrap_err();
        assert!(err.contains("line break"), "{broken:?}: {err}");
    }
}

#[test]
fn a_path_left_in_the_schema_is_refused() {
    let template = EmailTemplate {
        html_template: Some("t.html".into()),
        ..body("{code}")
    };
    let err = template.check(EmailKind::Otp, false).unwrap_err();
    assert!(err.contains("t.html") && err.contains("fraiseql compile"), "{err}");
}

#[test]
fn check_email_covers_the_sender_name_and_every_kind() {
    let config = |email: LocalEmailConfig, name: Option<&str>| LocalAuthConfig {
        email_from_name: name.map(str::to_string),
        email: Some(Box::new(email)),
        ..LocalAuthConfig::default()
    };
    let named = config(LocalEmailConfig::default(), Some("A\r\nBcc: x"));
    assert!(named.check_email().unwrap_err().contains("email_from_name"));
    for (kind, email) in [
        (
            "reset",
            LocalEmailConfig {
                reset: Some(body("{code}")),
                ..LocalEmailConfig::default()
            },
        ),
        (
            "verification",
            LocalEmailConfig {
                verification: Some(body("{code}")),
                ..LocalEmailConfig::default()
            },
        ),
        (
            "otp",
            LocalEmailConfig {
                otp: Some(body("x")),
                ..LocalEmailConfig::default()
            },
        ),
    ] {
        let err = config(email, None).check_email().unwrap_err();
        assert!(err.contains(&format!("[auth.local.email.{kind}]")), "{err}");
    }
}
