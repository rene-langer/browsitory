//! User command lines (#893): split like the external tools' commands, then each `${…}` expanded
//! inside its own word, so no value can add, split or quote arguments.

// Hide the developer's / CI runner's global and system git config (#111).
test_support::hermetic_git!();

use git_core::process::{CommandStyle, SplitError};
use git_core::user_command::{parse, Expanded, MissingValue, TemplateError, Values, Variable};

const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

fn values() -> Values {
    Values {
        repo: "/work/my repo".to_string(),
        commit: Some(COMMIT.to_string()),
        short_commit: Some("0123456".to_string()),
        branch: Some("feature/x".to_string()),
        git_ref: Some("refs/heads/feature/x".to_string()),
        path: Some("/work/my repo/a b.txt".to_string()),
        paths: Some(vec![
            "/work/my repo/a b.txt".to_string(),
            "/work/my repo/$(rm -rf ~).txt".to_string(),
        ]),
    }
}

fn args(command_line: &str, values: &Values) -> Vec<String> {
    parse(command_line, None, CommandStyle::Posix)
        .expect("valid command")
        .expand(values)
        .expect("all values present")
        .args
}

#[test]
fn splits_then_expands_each_variable_into_one_argument() {
    assert_eq!(
        args("git log -1 --format=%H ${commit} -- ${path}", &values()),
        [
            "git",
            "log",
            "-1",
            "--format=%H",
            COMMIT,
            "--",
            "/work/my repo/a b.txt"
        ]
    );
}

#[test]
fn values_with_spaces_quotes_and_shell_syntax_stay_one_argument() {
    let mut values = values();
    values.branch = Some("a b\"c'd;$(x) `y` ${commit}".to_string());
    assert_eq!(
        args("echo ${branch} end", &values),
        ["echo", "a b\"c'd;$(x) `y` ${commit}", "end"]
    );
}

#[test]
fn paths_becomes_one_argument_per_path() {
    assert_eq!(
        args("lint ${paths} --fix", &values()),
        [
            "lint",
            "/work/my repo/a b.txt",
            "/work/my repo/$(rm -rf ~).txt",
            "--fix"
        ]
    );
    let mut none = values();
    none.paths = Some(Vec::new());
    assert_eq!(args("lint ${paths} --fix", &none), ["lint", "--fix"]);
}

#[test]
fn variables_expand_inside_a_word_and_inside_quotes() {
    assert_eq!(
        args(
            "${repo}/scripts/tag.sh --ref=${ref} \"${shortCommit} on ${branch}\"",
            &values()
        ),
        [
            "/work/my repo/scripts/tag.sh",
            "--ref=refs/heads/feature/x",
            "0123456 on feature/x"
        ]
    );
}

#[test]
fn a_doubled_dollar_before_a_brace_is_a_literal() {
    assert_eq!(
        args("echo $${HOME} $HOME $$ a$b", &values()),
        ["echo", "${HOME}", "$HOME", "$$", "a$b"]
    );
}

#[test]
fn rejects_an_unknown_variable_and_names_it() {
    let error = parse("echo ${foo}", None, CommandStyle::Posix).unwrap_err();
    assert_eq!(error, TemplateError::UnknownVariable("foo".to_string()));
    let message = error.to_string();
    assert!(message.contains("${foo}"), "{message}");
    assert!(message.contains("${shortCommit}"), "{message}");
    // Names are case-sensitive.
    assert_eq!(
        parse("echo ${Commit}", None, CommandStyle::Posix).unwrap_err(),
        TemplateError::UnknownVariable("Commit".to_string())
    );
}

#[test]
fn rejects_an_unclosed_variable() {
    assert_eq!(
        parse("echo ${commit", None, CommandStyle::Posix).unwrap_err(),
        TemplateError::UnclosedVariable
    );
}

#[test]
fn rejects_paths_inside_a_longer_argument() {
    assert_eq!(
        parse("tool --files=${paths}", None, CommandStyle::Posix).unwrap_err(),
        TemplateError::PathsNotAlone
    );
    assert_eq!(
        parse("tool \"${paths} x\"", None, CommandStyle::Posix).unwrap_err(),
        TemplateError::PathsNotAlone
    );
}

#[test]
fn rejects_an_empty_command_and_an_unclosed_quote() {
    assert_eq!(
        parse("   ", None, CommandStyle::Posix).unwrap_err(),
        TemplateError::Split(SplitError::Empty)
    );
    assert_eq!(
        parse("echo 'x", None, CommandStyle::Posix).unwrap_err(),
        TemplateError::Split(SplitError::UnclosedQuote)
    );
}

#[test]
fn lists_the_variables_it_uses_once_each_in_a_fixed_order() {
    let template = parse(
        "x ${path} ${commit} ${shortCommit} ${commit}",
        Some("${repo}/sub"),
        CommandStyle::Posix,
    )
    .unwrap();
    assert_eq!(
        template.variables(),
        [
            Variable::Repo,
            Variable::Commit,
            Variable::ShortCommit,
            Variable::Path
        ]
    );
    assert!(parse("make", None, CommandStyle::Posix)
        .unwrap()
        .variables()
        .is_empty());
}

#[test]
fn shows_the_words_as_split_with_variables_unexpanded() {
    let template = parse("tool '${commit} x' $${y} --a=b", None, CommandStyle::Posix).unwrap();
    assert_eq!(template.words(), ["tool", "${commit} x", "$${y}", "--a=b"]);
    assert_eq!(template.literal_program(), Some("tool"));
    let in_repo = parse("${repo}/run.sh", None, CommandStyle::Posix).unwrap();
    assert_eq!(in_repo.literal_program(), None);
}

#[test]
fn a_missing_value_names_its_variable() {
    let mut values = values();
    values.commit = None;
    let template = parse("git show ${commit}", None, CommandStyle::Posix).unwrap();
    let missing = template.expand(&values).unwrap_err();
    assert_eq!(missing, MissingValue(Variable::Commit));
    assert!(missing.to_string().contains("${commit}"), "{missing}");

    values.paths = None;
    let template = parse("lint ${paths}", None, CommandStyle::Posix).unwrap();
    assert_eq!(
        template.expand(&values).unwrap_err(),
        MissingValue(Variable::Paths)
    );
}

#[test]
fn the_working_directory_is_one_word_and_blank_means_the_repository() {
    let template = parse("make", Some("  ${repo}/sub dir  "), CommandStyle::Posix).unwrap();
    assert_eq!(
        template.expand(&values()).unwrap(),
        Expanded {
            args: vec!["make".to_string()],
            directory: Some("/work/my repo/sub dir".to_string()),
        }
    );
    let blank = parse("make", Some("   "), CommandStyle::Posix).unwrap();
    assert_eq!(blank.expand(&values()).unwrap().directory, None);
    assert_eq!(
        parse("make", None, CommandStyle::Posix)
            .unwrap()
            .expand(&values())
            .unwrap()
            .directory,
        None
    );
}

#[test]
fn the_working_directory_checks_its_variables_too() {
    assert_eq!(
        parse("make", Some("${paths}"), CommandStyle::Posix).unwrap_err(),
        TemplateError::PathsInDirectory
    );
    assert_eq!(
        parse("make", Some("${nope}"), CommandStyle::Posix).unwrap_err(),
        TemplateError::UnknownVariable("nope".to_string())
    );
}

#[test]
fn windows_rules_keep_backslashes_in_paths() {
    let template = parse(
        r#""C:\Program Files\Tool\tool.exe" ${path} C:\out\"#,
        None,
        CommandStyle::Windows,
    )
    .unwrap();
    let mut values = values();
    values.path = Some(r"C:\work\a b.txt".to_string());
    assert_eq!(
        template.expand(&values).unwrap().args,
        [
            r"C:\Program Files\Tool\tool.exe",
            r"C:\work\a b.txt",
            r"C:\out\"
        ]
    );
}

#[test]
fn variable_names_round_trip() {
    for variable in Variable::ALL {
        assert_eq!(Variable::from_name(variable.name()), Some(variable));
        assert_eq!(variable.to_string(), format!("${{{}}}", variable.name()));
    }
    assert_eq!(
        Variable::ALL.map(Variable::name),
        [
            "repo",
            "commit",
            "shortCommit",
            "branch",
            "ref",
            "path",
            "paths"
        ]
    );
    assert_eq!(Variable::from_name("nope"), None);
}
