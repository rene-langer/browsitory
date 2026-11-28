//! git's `wildmatch` (`wildmatch.c`), the glob matcher of its gitignore-style patterns, on bytes:
//! `*`, `**`, `?`, `[...]` classes with ranges, `!`/`^` negation and `[:name:]`, `\` escapes.
//! The sparse checkout's non-cone patterns use it (#1187).

/// How a pattern matches (`WM_PATHNAME`, `WM_CASEFOLD`).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Flags {
    /// `*` and `?` don't match `/`; `**` between slashes matches any number of folders.
    pub(crate) pathname: bool,
    /// ASCII letters match either case.
    pub(crate) casefold: bool,
}

/// git's `is_glob_special`.
pub(crate) fn is_glob_special(byte: u8) -> bool {
    matches!(byte, b'*' | b'?' | b'[' | b'\\')
}

/// Whether `text` matches all of `pattern`.
pub(crate) fn wildmatch(pattern: &[u8], text: &[u8], flags: Flags) -> bool {
    dowild(pattern, text, flags) == Wild::Match
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Wild {
    Match,
    NoMatch,
    /// The text ran out: no shorter tail can match either.
    AbortAll,
    /// A `*` reached a `/` it can't cross: only an outer `**` may try further.
    AbortToStarStar,
}

/// The byte at `at`, 0 past the end (the C string's terminator).
fn at(bytes: &[u8], at: usize) -> u8 {
    bytes.get(at).copied().unwrap_or(0)
}

fn fold(byte: u8, flags: Flags) -> u8 {
    if flags.casefold {
        byte.to_ascii_lowercase()
    } else {
        byte
    }
}

fn dowild(pattern: &[u8], text: &[u8], flags: Flags) -> Wild {
    let mut p = 0;
    let mut t = 0;
    while at(pattern, p) != 0 {
        let mut p_ch = at(pattern, p);
        let mut t_ch = at(text, t);
        if t_ch == 0 && p_ch != b'*' {
            return Wild::AbortAll;
        }
        t_ch = fold(t_ch, flags);
        p_ch = fold(p_ch, flags);
        match p_ch {
            b'?' => {
                if flags.pathname && t_ch == b'/' {
                    return Wild::NoMatch;
                }
            }
            b'*' => {
                p += 1;
                let match_slash;
                if at(pattern, p) == b'*' {
                    let before = p.checked_sub(2);
                    while at(pattern, p) == b'*' {
                        p += 1;
                    }
                    let next = at(pattern, p);
                    if !flags.pathname {
                        match_slash = true;
                    } else if before.is_none_or(|before| pattern[before] == b'/')
                        && (next == 0
                            || next == b'/'
                            || (next == b'\\' && at(pattern, p + 1) == b'/'))
                    {
                        // `foo/**/bar` matches `foo/bar` too: the `**/` matching nothing.
                        if next == b'/'
                            && dowild(&pattern[p + 1..], &text[t..], flags) == Wild::Match
                        {
                            return Wild::Match;
                        }
                        match_slash = true;
                    } else {
                        match_slash = false;
                    }
                } else {
                    match_slash = !flags.pathname;
                }
                let next = at(pattern, p);
                if next == 0 {
                    // A trailing `**` matches everything, a trailing `*` no further `/`.
                    if !match_slash && text[t..].contains(&b'/') {
                        return Wild::NoMatch;
                    }
                    return Wild::Match;
                }
                if !match_slash && next == b'/' {
                    // One `*` before a `/` matches the rest of this folder name.
                    let Some(slash) = text[t..].iter().position(|byte| *byte == b'/') else {
                        return Wild::NoMatch;
                    };
                    t += slash;
                    // The slash is matched below, as the pattern's next byte.
                    p += 1;
                    t += 1;
                    continue;
                }
                loop {
                    if t_ch == 0 {
                        break;
                    }
                    if !is_glob_special(next) {
                        // Skip ahead to the next byte that can match the literal after `*`.
                        let wanted = fold(next, flags);
                        loop {
                            t_ch = fold(at(text, t), flags);
                            if t_ch == 0 || (!match_slash && t_ch == b'/') || t_ch == wanted {
                                break;
                            }
                            t += 1;
                        }
                        if t_ch != wanted {
                            return Wild::NoMatch;
                        }
                    }
                    let matched = dowild(&pattern[p..], &text[t..], flags);
                    if matched != Wild::NoMatch {
                        if !match_slash || matched != Wild::AbortToStarStar {
                            return matched;
                        }
                    } else if !match_slash && t_ch == b'/' {
                        return Wild::AbortToStarStar;
                    }
                    t += 1;
                    t_ch = at(text, t);
                }
                return Wild::AbortAll;
            }
            b'[' => {
                p += 1;
                p_ch = at(pattern, p);
                if p_ch == b'^' {
                    p_ch = b'!';
                }
                let negated = p_ch == b'!';
                if negated {
                    p += 1;
                    p_ch = at(pattern, p);
                }
                let mut prev_ch = 0;
                let mut matched = false;
                loop {
                    if p_ch == 0 {
                        return Wild::AbortAll;
                    }
                    if p_ch == b'\\' {
                        p += 1;
                        p_ch = at(pattern, p);
                        if p_ch == 0 {
                            return Wild::AbortAll;
                        }
                        if t_ch == p_ch {
                            matched = true;
                        }
                    } else if p_ch == b'-'
                        && prev_ch != 0
                        && at(pattern, p + 1) != 0
                        && at(pattern, p + 1) != b']'
                    {
                        p += 1;
                        p_ch = at(pattern, p);
                        if p_ch == b'\\' {
                            p += 1;
                            p_ch = at(pattern, p);
                            if p_ch == 0 {
                                return Wild::AbortAll;
                            }
                        }
                        if (prev_ch..=p_ch).contains(&t_ch)
                            || (flags.casefold
                                && t_ch.is_ascii_lowercase()
                                && (prev_ch..=p_ch).contains(&t_ch.to_ascii_uppercase()))
                        {
                            matched = true;
                        }
                        // The range is used up: no `-` after it starts another.
                        p_ch = 0;
                    } else if p_ch == b'[' && at(pattern, p + 1) == b':' {
                        let start = p + 2;
                        p = start;
                        while at(pattern, p) != 0 && at(pattern, p) != b']' {
                            p += 1;
                        }
                        if at(pattern, p) == 0 {
                            return Wild::AbortAll;
                        }
                        if p == start || pattern[p - 1] != b':' {
                            // No `:]`: an ordinary `[` in the set.
                            p = start - 2;
                            p_ch = b'[';
                            if t_ch == p_ch {
                                matched = true;
                            }
                        } else {
                            match class(&pattern[start..p - 1], t_ch, flags) {
                                Some(true) => matched = true,
                                Some(false) => {}
                                None => return Wild::AbortAll,
                            }
                            p_ch = 0;
                        }
                    } else if t_ch == p_ch {
                        matched = true;
                    }
                    prev_ch = p_ch;
                    p += 1;
                    p_ch = at(pattern, p);
                    if p_ch == b']' {
                        break;
                    }
                }
                if matched == negated || (flags.pathname && t_ch == b'/') {
                    return Wild::NoMatch;
                }
            }
            b'\\' => {
                // The next byte literally, as given (not folded).
                p += 1;
                if t_ch != at(pattern, p) {
                    return Wild::NoMatch;
                }
            }
            _ => {
                if t_ch != p_ch {
                    return Wild::NoMatch;
                }
            }
        }
        p += 1;
        t += 1;
    }
    if at(text, t) == 0 {
        Wild::Match
    } else {
        Wild::NoMatch
    }
}

/// Whether `byte` is in the `[:name:]` class, `None` for a name git doesn't know. git's own
/// ASCII character classes (`sane_ctype`).
fn class(name: &[u8], byte: u8, flags: Flags) -> Option<bool> {
    Some(match name {
        b"alnum" => byte.is_ascii_alphanumeric(),
        b"alpha" => byte.is_ascii_alphabetic(),
        b"blank" => matches!(byte, b' ' | b'\t'),
        b"cntrl" => byte.is_ascii_control(),
        b"digit" => byte.is_ascii_digit(),
        b"graph" => byte.is_ascii_graphic(),
        b"lower" => byte.is_ascii_lowercase(),
        b"print" => byte.is_ascii_graphic() || byte == b' ',
        b"punct" => byte.is_ascii_punctuation(),
        b"space" => matches!(byte, b' ' | b'\t' | b'\n' | b'\r'),
        b"upper" => byte.is_ascii_uppercase() || (flags.casefold && byte.is_ascii_lowercase()),
        b"xdigit" => byte.is_ascii_hexdigit(),
        _ => return None,
    })
}
