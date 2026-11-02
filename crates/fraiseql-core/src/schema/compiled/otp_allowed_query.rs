//! The statement behind `[auth.local] otp_allowed_query` (#1852).

/// Check an `otp_allowed_query` and lower its `$email` to the bound parameter `$1`.
///
/// Both the compiler and the server's boot check call this, so a hand-written config meets
/// the same rule. The address is always bound, never written into the text, and the rule
/// exists to refuse a query that could not work: this is the operator's own SQL, and what
/// stops it changing anything is the read-only transaction it runs in.
///
/// # Errors
///
/// Returns a message naming the problem unless the query is one `SELECT` that refers to
/// `$email`, with no other parameter, no comment, and no second statement.
pub fn otp_allowed_statement(query: &str) -> Result<String, String> {
    let query = query.trim();
    let first_word = query.split_whitespace().next().unwrap_or_default();
    if !first_word.eq_ignore_ascii_case("select") {
        return Err("it must be a single SELECT".to_string());
    }

    let mut statement = String::with_capacity(query.len());
    let mut refers_to_email = false;
    let mut quote = None;
    let mut rest = query;
    while let Some(c) = rest.chars().next() {
        if let Some(open) = quote {
            if c == open {
                quote = None;
            }
        } else if c == '\'' || c == '"' {
            quote = Some(c);
        } else if c == ';' {
            return Err("it must be a single statement, with no ';'".to_string());
        } else if rest.starts_with("--") || rest.starts_with("/*") {
            return Err("it must not contain a comment".to_string());
        } else if c == '$' {
            let after = rest.strip_prefix("$email").filter(|after| {
                !after.chars().next().is_some_and(|next| next.is_alphanumeric() || next == '_')
            });
            let Some(after) = after else {
                return Err(
                    "$email is the only parameter it may use, and it may not use dollar quoting"
                        .to_string(),
                );
            };
            statement.push_str("$1");
            refers_to_email = true;
            rest = after;
            continue;
        }
        statement.push(c);
        rest = &rest[c.len_utf8()..];
    }
    if quote.is_some() {
        return Err("it has an unterminated quote".to_string());
    }
    if !refers_to_email {
        return Err("it must refer to the address as $email".to_string());
    }
    Ok(statement)
}

#[cfg(test)]
mod tests {
    use super::otp_allowed_statement;

    #[test]
    fn email_is_lowered_to_the_bound_parameter_everywhere_it_appears() {
        assert_eq!(
            otp_allowed_statement(" select 1 FROM m WHERE a = $email OR b = $email ").as_deref(),
            Ok("select 1 FROM m WHERE a = $1 OR b = $1")
        );
    }

    #[test]
    fn a_quoted_email_is_text_not_the_parameter() {
        assert_eq!(
            otp_allowed_statement("SELECT 1 FROM m WHERE note = '$email; x' AND a = $email")
                .as_deref(),
            Ok("SELECT 1 FROM m WHERE note = '$email; x' AND a = $1")
        );
        assert!(otp_allowed_statement("SELECT 1 FROM m WHERE note = '$email'").is_err());
    }

    #[test]
    fn multibyte_text_survives() {
        assert_eq!(
            otp_allowed_statement("SELECT 1 FROM m WHERE é = 'ç' AND a = $email").as_deref(),
            Ok("SELECT 1 FROM m WHERE é = 'ç' AND a = $1")
        );
    }

    #[test]
    fn what_could_not_work_is_refused() {
        for query in [
            "",
            "DELETE FROM m WHERE a = $email",
            "SELECT 1 FROM m",
            "SELECT 1 FROM m WHERE a = $1",
            "SELECT 1 FROM m WHERE a = $emails",
            "SELECT 1 FROM m WHERE a = $email; DROP TABLE m",
            "SELECT 1 FROM m WHERE a = $email;",
            "SELECT 1 FROM m WHERE a = $email -- note",
            "SELECT 1 /* note */ FROM m WHERE a = $email",
            "SELECT $$x$$ FROM m WHERE a = $email",
            "SELECT 1 FROM m WHERE a = 'x AND b = $email",
            "SELECT 1 FROM m WHERE b = $email AND a = 'x",
            "SELECTED FROM m WHERE a = $email",
        ] {
            assert!(otp_allowed_statement(query).is_err(), "must refuse: {query}");
        }
    }
}
