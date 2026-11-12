//! The installed font families, for the code font suggestions in Preferences (#858). Read from
//! the system's font folders each time they're asked for; nothing is stored or sent anywhere.

/// One installed font family: the name CSS knows it by, and whether it's monospaced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontFamily {
    pub name: String,
    pub monospace: bool,
}

/// The font families installed on this computer, as [`families_of`] lists them.
pub fn installed_font_families() -> Vec<FontFamily> {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    // A face's first family name is its English one, which CSS matches on every platform.
    families_of(db.faces().filter_map(|face| {
        face.families
            .first()
            .map(|(name, _)| (name.as_str(), face.monospaced))
    }))
}

/// One entry per family name of `faces` (each face's family name, and whether that face is
/// monospaced). A family is monospaced only when all its faces are. Names are trimmed; blank
/// names and names starting with `.` (macOS's hidden system fonts, which CSS can't name) are
/// left out. Sorted by name, ignoring case, then exactly, so the order is stable.
pub fn families_of<'a>(faces: impl IntoIterator<Item = (&'a str, bool)>) -> Vec<FontFamily> {
    let mut families = std::collections::BTreeMap::<&str, bool>::new();
    for (name, monospaced) in faces {
        let name = name.trim();
        if name.is_empty() || name.starts_with('.') {
            continue;
        }
        families
            .entry(name)
            .and_modify(|monospace| *monospace &= monospaced)
            .or_insert(monospaced);
    }
    let mut families: Vec<FontFamily> = families
        .into_iter()
        .map(|(name, monospace)| FontFamily {
            name: name.to_string(),
            monospace,
        })
        .collect();
    families.sort_by_cached_key(|family| (family.name.to_lowercase(), family.name.clone()));
    families
}

#[cfg(test)]
mod tests {
    use super::*;

    fn family(name: &str, monospace: bool) -> FontFamily {
        FontFamily {
            name: name.to_string(),
            monospace,
        }
    }

    #[test]
    fn faces_of_one_family_give_one_entry() {
        let families = families_of([
            ("Fira Code", true),
            ("Arial", false),
            ("Fira Code", true),
            ("Arial", false),
        ]);
        assert_eq!(
            families,
            [family("Arial", false), family("Fira Code", true)]
        );
    }

    #[test]
    fn a_family_is_monospaced_only_when_every_face_is() {
        for faces in [
            [("Mixed", true), ("Mixed", false)],
            [("Mixed", false), ("Mixed", true)],
        ] {
            assert_eq!(families_of(faces), [family("Mixed", false)], "{faces:?}");
        }
        assert_eq!(
            families_of([("Mono", true), ("Mono", true)]),
            [family("Mono", true)]
        );
    }

    #[test]
    fn hidden_and_blank_names_are_left_out_and_names_are_trimmed() {
        let families = families_of([
            (".SF NS", false),
            ("", false),
            ("  ", true),
            (" Source Code Pro ", true),
            ("Source Code Pro", true),
        ]);
        assert_eq!(families, [family("Source Code Pro", true)]);
    }

    #[test]
    fn families_are_sorted_ignoring_case_then_exactly() {
        let names: Vec<String> = families_of([
            ("b", false),
            ("a", false),
            ("B", false),
            ("Courier", true),
            ("A", false),
        ])
        .into_iter()
        .map(|f| f.name)
        .collect();
        assert_eq!(names, ["A", "a", "B", "b", "Courier"]);
    }

    #[test]
    fn this_computers_fonts_are_listed_sorted_without_hidden_names() {
        let families = installed_font_families();
        // macOS and Windows always have fonts; a Linux build machine may have none installed.
        if cfg!(any(target_os = "macos", windows)) {
            assert!(!families.is_empty());
            assert!(families.iter().any(|f| f.monospace), "{families:?}");
        }
        // Where fontconfig lists fonts, they are found too.
        if cfg!(target_os = "linux") {
            let listed = std::process::Command::new("fc-list")
                .args([":", "family"])
                .output()
                .ok()
                .filter(|out| out.status.success())
                .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
                .unwrap_or_default();
            // `fc-list` prints each family (with its localized names after commas) per line.
            let first_names: Vec<&str> = listed
                .lines()
                .filter_map(|line| line.split(',').next())
                .map(str::trim)
                .filter(|name| !name.is_empty() && !name.starts_with('.'))
                .collect();
            if !first_names.is_empty() {
                assert!(
                    first_names
                        .iter()
                        .any(|name| families.iter().any(|f| f.name == *name)),
                    "fc-list: {first_names:?}, found: {families:?}"
                );
            }
        }
        assert!(families
            .iter()
            .all(|f| !f.name.is_empty() && !f.name.starts_with('.') && f.name.trim() == f.name));
        let mut sorted = families.clone();
        sorted.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then_with(|| a.name.cmp(&b.name))
        });
        sorted.dedup_by(|a, b| a.name == b.name);
        assert_eq!(families, sorted);
    }
}
