
#[derive(Default)]
pub struct Pattern<'a>(Vec<(&'a str, Pattern<'a>)>);

impl<'a> std::fmt::Debug for Pattern<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0[..] {
            [] => f.write_str("@"),
            [single] => {
                f.write_str(single.0)?;
                f.write_str("/")?;
                std::fmt::Debug::fmt(&single.1, f)
            }
            rest => {
                f.write_str("[")?;
                f.write_str(rest[0].0)?;
                f.write_str("/")?;
                std::fmt::Debug::fmt(&rest[0].1, f)?;
                for single in &rest[1..] {
                    f.write_str(",")?;
                    f.write_str(single.0)?;
                    f.write_str("/")?;
                    std::fmt::Debug::fmt(&single.1, f)?;
                }
                f.write_str("]")
            }
        }
    }
}

impl<'a> Pattern<'a> {
    pub fn new(pattern: &'a str) -> Pattern<'a> {
        if pattern.starts_with('{') {
            assert!(pattern.ends_with('}'));
            let mut acc = Pattern::default();
            let mut depth = 0;
            let mut start = 1;
            for (i, byte) in pattern[0..pattern.len() - 1].bytes().enumerate().skip(1) {
                match byte {
                    b'{' => {
                        depth += 1;
                    }
                    b'}' => {
                        assert_ne!(depth, 0, "unmatched closing brace in pattern");
                        depth -= 1;
                    }
                    b',' if depth == 0 => {
                        acc.0.append(&mut Pattern::new(&pattern[start..i]).0);
                        start = i + 1;
                    }
                    _ => {}
                }
            }
            acc.0.append(&mut Pattern::new(&pattern[start..pattern.len() - 1]).0);
            acc
        } else if pattern.is_empty() {
            Self(Vec::new())
        } else {
            let (stub, rest) = pattern.split_once('/').unwrap_or((pattern, ""));
            Self(vec![(stub, Pattern::new(rest))])
        }
    }
    pub fn count_leafs(&self) -> usize {
        if self.is_leaf() {
            1
        } else {
            self.0.iter().map(|x| x.1.count_leafs()).sum()
        }
    }
    pub fn is_leaf(&self) -> bool {
        self.0.is_empty()
    }
    pub fn iter<'s>(&'s self) -> std::slice::Iter<'s, (&str, Pattern<'a>)> {
        self.0.iter()
    }
    pub fn pattern_check(pattern: &str, target: &str) -> bool {
        let Some((starts_with, pattern)) = pattern.split_once('*') else {
            return pattern == target;
        };
        if !target.starts_with(starts_with) {
            return false;
        }
        let Some(last_index) = pattern.rfind('*') else {
            return target.ends_with(pattern);
        };
        let (mut pattern, ends_with) = pattern.split_at(last_index);
        if !target.ends_with(ends_with) {
            return false;
        }
        let mut target = target;
        while !pattern.is_empty() {
            let (section, rest) = pattern.split_once('*').unwrap_or((pattern, ""));
            let Some(index) = target.find(section) else {
                return false;
            };
            target = &target[index + section.len()..];
            pattern = rest;
        }
        true
    }
}

