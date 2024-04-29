use std::collections::HashMap;

#[derive(Debug)]
pub struct InfiniteRecursion;
type MacrosInner = HashMap<String, Vec<String>>;
#[derive(Debug, Default)]
pub struct Macros(MacrosInner);
impl Macros {
    pub fn new() -> Self {
        Self(MacrosInner::new())
    }
    pub fn add<'a>(
        &mut self,
        name: &str,
        steps: impl IntoIterator<Item = &'a str>,
    ) -> Result<(), InfiniteRecursion> {
        let commands = self.expand_for_insertion(steps)?;
        self.0.insert(name.to_ascii_uppercase(), commands);
        Ok(())
    }
    pub fn get(&self, name: &str) -> Option<&Vec<String>> {
        self.0.get(&name.to_ascii_uppercase())
    }
    pub fn remove(&mut self, name: &str) -> Option<Vec<String>> {
        self.0.remove(&name.to_ascii_uppercase())
    }
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
    fn macro_expansion() {
        let mut macros = Macros::new();
        macros.add("codes", ["G0", "G1", "G2"]).unwrap();
        macros.add("codes2", ["codes", "G100"]).unwrap();
        assert_eq!(
            macros.get("codes2").unwrap(),
            &vec!["G0", "G1", "G2", "G100"]
        );
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
