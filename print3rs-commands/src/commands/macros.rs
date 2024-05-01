use std::collections::HashMap;

#[derive(Debug)]
pub struct InfiniteRecursion;

type MacrosInner = HashMap<String, Vec<String>>;

/// Holder for G code macros.
/// Handles storage, lookup and expansion.
#[derive(Debug, Default)]
pub struct Macros(MacrosInner);

impl Macros {
    /// Empty holder
    pub fn new() -> Self {
        Self(MacrosInner::new())
    }

    /// Add a new macro, stores the expansion
    pub fn add<'a>(
        &mut self,
        name: &str,
        steps: impl IntoIterator<Item = &'a str>,
    ) -> Result<(), InfiniteRecursion> {
        let commands = self.expand_for_insertion(steps)?;
        self.0.insert(name.to_ascii_uppercase(), commands);
        Ok(())
    }

    /// Lookup a macro by name, return its expansion if defined
    pub fn get(&self, name: &str) -> Option<&Vec<String>> {
        self.0.get(&name.to_ascii_uppercase())
    }

    /// Remove a macro by name.
    /// If a macro with the same name existed, the previous expansion is returned.
    pub fn remove(&mut self, name: &str) -> Option<Vec<String>> {
        self.0.remove(&name.to_ascii_uppercase())
    }

    /// Iterate (name, expansions) stored
    pub fn iter(&self) -> std::collections::hash_map::Iter<'_, String, Vec<String>> {
        self.0.iter()
    }

    fn expand_recursive(
        &self,
        expanded: &mut Vec<String>,
        code: &str,
        already_expanded: Option<Vec<&str>>,
    ) -> Result<(), InfiniteRecursion> {
        // track expressions already expanded to prevent infinite recursion
        let mut already_expanded = already_expanded.unwrap_or_default();
        if already_expanded.contains(&code) {
            return Err(InfiniteRecursion);
        }
        match self.get(code) {
            Some(expansion) => {
                already_expanded.push(code);
                for extra in expansion {
                    self.expand_recursive(expanded, extra, Some(already_expanded.clone()))?
                }
            }
            None => expanded.push(code.to_ascii_uppercase()),
        };
        Ok(())
    }

    /// recursively expand all in input sequence before placing into internal map
    /// placing recursion here eliminates possibility of infinite recursion
    fn expand_for_insertion(
        &self,
        codes: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> Result<Vec<String>, InfiniteRecursion> {
        let mut expanded = vec![];

        for code in codes {
            self.expand_recursive(&mut expanded, code.as_ref(), None)?;
        }
        Ok(expanded)
    }

    /// Given a list of Gcodes and/or macros, replace any defined macros in the sequence with its expansion.
    pub fn expand<'a>(&self, codes: impl IntoIterator<Item = &'a str>) -> Vec<String> {
        let mut expanded = vec![];
        for code in codes {
            match self.get(code) {
                Some(expansion) => expanded.extend(expansion.iter().cloned()),
                None => expanded.push(code.to_ascii_uppercase()),
            }
        }
        expanded
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn macro_storage() {
        let mut macros = Macros::new();
        macros.add("codes", ["G0", "G1", "G2"]).unwrap();
        macros.add("codes2", ["codes", "G100"]).unwrap();
        assert_eq!(
            macros.get("codes2").unwrap(),
            &vec!["G0", "G1", "G2", "G100"]
        );
    }

    #[test]
    fn macro_expansion_empty() {
        let macros = Macros::new();
        let input = vec!["G0", "ONE", "G1"];
        let output = macros.expand(input.clone());
        assert_eq!(input, output)
    }

    #[test]
    fn macro_expansion() {
        let mut macros = Macros::new();
        macros.add("one", ["step1", "step2"]).unwrap();
        let output = macros.expand(["G0", "one", "G1"]);
        assert_eq!(output, vec!["G0", "STEP1", "STEP2", "G1"]);
    }

    #[test]
    fn iteration() {
        let mut macros = Macros::new();
        assert!(macros.iter().next().is_none());
        macros.add("test", ["G0", "G1"]).unwrap();
        let mut iter = macros.iter();
        let (name, steps) = iter.next().unwrap();
        assert_eq!(name, "TEST");
        assert_eq!(steps, &["G0", "G1"]);
        assert!(iter.next().is_none());
    }

    #[test]
    fn remove_macros() {
        let mut macros = Macros::new();
        macros.add("test", ["G0", "G1"]).unwrap();
        assert!(macros.get("test").is_some());
        let expansion = macros.remove("test").unwrap();
        assert_eq!(expansion, vec!["G0", "G1"]);
        assert!(macros.get("test").is_none());
    }

    #[test]
    fn detect_infinite_recursion() {
        let mut macros = Macros::new();
        macros.add("zero", ["one", "two", "zero"]).unwrap();
        macros.add("one", ["zero", "one", "two"]).unwrap_err();
    }

    #[test]
    fn mutual_ref_not_recursive() {
        let mut macros = Macros::new();
        macros.add("zero", ["one", "two", "three"]).unwrap();
        macros.add("one", ["zero", "one", "two"]).unwrap();
    }
}
