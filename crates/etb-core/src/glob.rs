//! A very small glob matcher, for the patterns in a toolchain recipe: the
//! prune rules, and the paths a bundle's own compiler is allowed to write.
//!
//! Only `*` is supported, and it matches any sequence of characters **including
//! `/`**, so `internal/c/*.o` covers a file in a directory below. One matcher
//! for both, because a pattern that meant different things to the fetch and to
//! the integrity check would be worse than either.

/// Does `text` match `pattern`?
pub fn matches(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    is_match(&p, &t)
}

fn is_match(p: &[char], t: &[char]) -> bool {
    // Iterative backtracking: linear in the common case, and no recursion depth
    // worries on long paths.
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star, mut mark) = (usize::MAX, 0usize);

    while ti < t.len() {
        if pi < p.len() && p[pi] == '*' {
            star = pi;
            mark = ti;
            pi += 1;
        } else if pi < p.len() && p[pi] == t[ti] {
            pi += 1;
            ti += 1;
        } else if star != usize::MAX {
            pi = star + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

#[cfg(test)]
mod tests {
    use super::matches;

    #[test]
    fn literal_paths_match_exactly() {
        assert!(matches("bundle.toml", "bundle.toml"));
        assert!(!matches("bundle.toml", "bundle.tom"));
        assert!(!matches("bundle.toml", "xbundle.toml"));
    }

    #[test]
    fn a_star_crosses_directory_separators() {
        // This is the property the recipe's rules rely on.
        assert!(matches(
            "*/sysroot/usr/lib64/*",
            "x86_64-conda/sysroot/usr/lib64/libc.a"
        ));
        assert!(matches(
            "*/sysroot/usr/lib64/*",
            "x86_64-conda/sysroot/usr/lib64/nested/deep/thing.o"
        ));
        assert!(matches(
            "lib/gcc/*/*/finclude/*",
            "lib/gcc/trip/16.2.0/finclude/omp_lib.mod"
        ));
    }

    #[test]
    fn prefix_and_suffix_stars_work() {
        assert!(matches("bin/*-fbc", "bin/x86_64-w64-mingw32-fbc"));
        assert!(matches("lib/libzstd.so*", "lib/libzstd.so.1"));
        assert!(matches("lib/libzstd.so*", "lib/libzstd.so"));
        assert!(!matches("bin/*-fbc", "bin/fbc-wrapper"));
    }

    #[test]
    fn the_rules_that_were_validated_still_select_what_they_did() {
        // The Windows recipe's drop list, and what it is aimed at.
        let drop = [
            "internal/c/c_compiler/aarch64-w64-mingw32/*",
            "internal/c/c_compiler/lib/clang/*/lib/linux/*",
            "internal/c/c_compiler/bin/lldb*",
        ];
        let dropped = |f: &str| drop.iter().any(|p| matches(p, f));
        assert!(dropped(
            "internal/c/c_compiler/aarch64-w64-mingw32/lib/libm.a"
        ));
        assert!(dropped(
            "internal/c/c_compiler/lib/clang/22/lib/linux/libclang_rt.asan.a"
        ));
        assert!(dropped("internal/c/c_compiler/bin/lldb-server.exe"));
        // and what a build actually opens, which must survive it
        assert!(!dropped("internal/c/c_compiler/bin/clang-22.exe"));
        assert!(!dropped(
            "internal/c/c_compiler/lib/clang/22/lib/windows/libclang_rt.builtins-x86_64.a"
        ));
        assert!(!dropped(
            "internal/c/c_compiler/x86_64-w64-mingw32/lib/libgdi32.a"
        ));
    }

    #[test]
    fn a_lone_star_matches_everything_and_empty_matches_only_empty() {
        assert!(matches("*", "anything/at/all"));
        assert!(matches("", ""));
        assert!(!matches("", "x"));
        assert!(matches("**", "anything"));
    }

    #[test]
    fn backtracking_terminates_on_pathological_input() {
        // Naive implementations blow up here.
        let text = "a".repeat(64);
        assert!(!matches("*a*a*a*a*a*a*a*b", &text));
    }
}
