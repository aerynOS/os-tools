// SPDX-FileCopyrightText: 2026 AerynOS Developers
// SPDX-License-Identifier: MPL-2.0

mod single_glob_tests {
    use fnmatch::Pattern;

    #[test]
    fn pattern_doesnt_match_literal() {
        let p = Pattern::new("/usr/bin");
        assert!(p.matches("/usr/lib64").is_none());
    }

    #[test]
    fn pattern_matches_literal() {
        let p = Pattern::new("/usr/bin");
        assert!(p.matches("/usr/bin").is_some());
    }

    #[test]
    fn glob_one_doesnt_match_separator() {
        let p = Pattern::new("?usr/bin/moss");
        assert!(p.matches("/usr/bin/moss").is_none());
    }

    #[test]
    fn glob_one_doesnt_match_end_of_file_name() {
        let p = Pattern::new("/usr/bin?/moss");
        assert!(p.matches("/usr/bin/moss").is_none());
    }

    #[test]
    fn glob_one_matches_character() {
        let p = Pattern::new("/usr/bin/mos?");
        assert!(p.matches("/usr/bin/moss").is_some());
    }

    #[test]
    fn glob_one_matches_unicode_characters() {
        let p = Pattern::new("/tmp/?Abcd/moss");
        assert!(p.matches("/tmp/🌍Abcd/moss").is_some());
    }

    #[test]
    fn glob_any_doesnt_capture_separator() {
        let p = Pattern::new("*usr/bin/moss");
        assert!(p.matches("/usr/bin/moss").is_none());
    }

    #[test]
    fn glob_any_doesnt_match_wrong_prefix() {
        let p = Pattern::new("/usr/b*/moss");
        assert!(p.matches("/usr/sbin/moss").is_none());
    }

    #[test]
    fn glob_any_matches_whole_file_name() {
        let p = Pattern::new("/usr/*/moss");
        assert!(p.matches("/usr/bin/moss").is_some());
    }

    #[test]
    fn glob_any_matches_end_of_file_name() {
        // Because Matcher::Any matches *zero* or more characters.
        let p = Pattern::new("/usr/bin/moss*");
        assert!(p.matches("/usr/bin/moss").is_some());
    }

    #[test]
    fn glob_any_matches_unicode_characters() {
        let p = Pattern::new("/tmp/*/moss");
        assert!(p.matches("/tmp/🌍Abcd/moss").is_some());
    }

    #[test]
    fn glob_any_matches_text_on_last_file() {
        let p = Pattern::new("lib/systemd/boot/efi/*.efi");
        assert!(p.matches("lib/systemd/boot/efi/systemd-bootx64.efi").is_some());
    }

    #[test]
    fn glob_any_doesnt_match_text_on_last_file_with_suffix() {
        let p = Pattern::new("lib/systemd/boot/efi/*.efi");
        assert!(p.matches("lib/systemd/boot/efi/systemd-bootx64.efi.stub").is_none());
    }

    #[test]
    fn pattern_matches_group_partial_filename() {
        let p = Pattern::new("/usr/b(partname:*)/moss");
        let matches = p.matches("/usr/bin/moss").unwrap();
        assert!(matches.get("partname").is_some_and(|value| value == "in"));
    }

    #[test]
    fn pattern_matches_group_whole_filename() {
        let p = Pattern::new("/usr/(bindir:*)/moss");
        let matches = p.matches("/usr/bin/moss").unwrap();
        assert!(matches.get("bindir").is_some_and(|value| value == "bin"));
    }

    #[test]
    fn pattern_escapes_literal_glob_characters() {
        let p = Pattern::new(r"/usr/\*/moss");
        assert!(p.matches("/usr/*/moss").is_some());
        assert!(p.matches("/usr/bin/moss").is_none());
    }

    #[test]
    fn pattern_escapes_literal_question_mark() {
        let p = Pattern::new(r"/usr/\?/bin/moss");
        assert!(p.matches("/usr/?/bin/moss").is_some());
        assert!(p.matches("/usr/x/bin/moss").is_none());
    }

    #[test]
    fn pattern_matches_named_glob_with_empty_capture() {
        let p = Pattern::new("/tmp/(empty:*)");
        let matches = p.matches("/tmp/").unwrap();
        assert_eq!(matches.get("empty").unwrap(), "");
    }
}

mod multiple_globs_tests {
    use fnmatch::Pattern;

    #[test]
    fn glob_one_doesnt_match_end_of_file_name() {
        let p = Pattern::new("/us?/bin?/moss");
        assert!(p.matches("/usr/bin/moss").is_none());
    }

    #[test]
    fn pattern_matches_two_glob_ones() {
        let p = Pattern::new("/us?/bi?/moss");
        assert!(p.matches("/usr/bin/moss").is_some());
    }

    #[test]
    fn pattern_doesnt_match_two_glob_anys() {
        let paths = [
            (Pattern::new("/usr/s*/mos*"), "/usr/bin/moss"),
            (Pattern::new("/usr/*/*os*"), "/usr/bin/NOPE"),
        ];
        for (pattern, path) in paths {
            assert_eq!(pattern.matches(path), None);
        }
    }

    #[test]
    fn pattern_matches_two_glob_anys_different_filenames() {
        let p = Pattern::new("/usr/*/*os*");
        assert!(p.matches("/usr/bin/moss").is_some());
    }

    #[test]
    fn pattern_matches_two_glob_anys_same_filename() {
        const PATH: &str = "/tmp/systemd-private-b6c2bb689c";

        let p = Pattern::new("/tmp/systemd-*-b6*");
        assert!(p.matches(PATH).is_some());

        let p = Pattern::new("/tmp/systemd-(any1:*)-b6(any2:*)");
        let matches = p.matches(PATH).unwrap();
        assert_eq!(matches.get("any1").unwrap(), "private");
        assert_eq!(matches.get("any2").unwrap(), "c2bb689c");
    }

    #[test]
    fn pattern_matches_consecutive_glob_any_and_one() {
        const PATH: &str = "/tmp/systemd-private-b6c2bb689c";

        let p = Pattern::new("/tmp/systemd-*?-b6c2bb689c");
        assert!(p.matches(PATH).is_some());

        let p = Pattern::new("/tmp/systemd-(any:*)(one:?)-b6c2bb689c");
        let matches = p.matches(PATH).unwrap();
        assert_eq!(matches.get("any").unwrap(), "privat");
        assert_eq!(matches.get("one").unwrap(), "e");
    }

    #[test]
    fn pattern_matches_consecutive_glob_ones() {
        const PATH: &str = "/tmp/systemd-private-b6c2bb689c";

        let p = Pattern::new("/tmp/systemd-??ivate-b6c2bb689c");
        assert!(p.matches(PATH).is_some());

        let p = Pattern::new("/tmp/systemd-(one1:?)(one2:?)ivate-b6c2bb689c");
        let matches = p.matches(PATH).unwrap();
        assert_eq!(matches.get("one1").unwrap(), "p");
        assert_eq!(matches.get("one2").unwrap(), "r");
    }
}
