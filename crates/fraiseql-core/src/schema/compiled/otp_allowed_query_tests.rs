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
        otp_allowed_statement("SELECT 1 FROM m WHERE note = '$email; x' AND a = $email").as_deref(),
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
