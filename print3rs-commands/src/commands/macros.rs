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
    pub fn get(&self, name: impl AsRef<str>) -> Option<&Vec<String>> {
        self.0.get(&name.as_ref().to_ascii_uppercase())
    }
    pub fn remove(&mut self, name: impl AsRef<str>) -> Option<Vec<String>> {
        self.0.remove(&name.as_ref().to_ascii_uppercase())
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
            match self.get(&code) {
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
    fn no_infinite_recurse() {
        let mut macros = Macros::new();
        macros.add("zero", vec!["one", "two", "zero"]).unwrap();
        macros.add("one", vec!["zero", "one", "two"]).unwrap_err();
    }
}
