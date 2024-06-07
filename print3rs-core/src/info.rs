use std::{collections::HashMap, ops::Deref};

/// Generic type for holding arbitrary device information
#[derive(Debug, Clone, PartialEq, PartialOrd, Default)]
pub enum Info {
    #[default]
    Key,
    Str(String),
    Int(isize),
    Float(f64),
    Bool(bool),
}

impl From<Info> for bool {
    fn from(value: Info) -> Self {
        value.is_true()
    }
}

impl Info {
    /// Asses truthiness for any possible contained type
    ///
    /// Key: always true
    /// Integral: true if >0 (positive only)
    /// float: true if not exactly 0.0
    /// bool: true if true
    /// string: true if non-empty
    pub fn is_true(&self) -> bool {
        match self {
            Info::Key => true,
            Info::Int(1..) => true,
            Info::Float(f) if *f > 0.0 => true,
            Info::Bool(true) => true,
            Info::Str(s) if !s.is_empty() => true,
            _ => false,
        }
    }
}

pub type InfoMapInner = HashMap<String, Info>;

/// Key:value store of various printer information
///
/// Use this type to store information about a printer,
/// and query various capabilities to determine extended functionality
#[derive(Debug, Default, Clone)]
pub struct InfoMap(InfoMapInner);

impl Deref for InfoMap {
    type Target = InfoMapInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<InfoMapInner> for InfoMap {
    fn from(value: InfoMapInner) -> Self {
        Self(value)
    }
}

impl From<InfoMap> for InfoMapInner {
    fn from(value: InfoMap) -> Self {
        value.0
    }
}

/// Known named capabilities of devices
#[non_exhaustive]
pub enum Capability {
    AutoreportTemp,
    AutoreportPos,
    EmergencyParser,
    AutoreportSdStatus,
    Arcs,
    HostActionCommands,
    BuildPercent,
    Progress,
    AdvancedOk,
}

impl AsRef<str> for Capability {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Capability {
    fn as_str(&self) -> &'static str {
        match self {
            Capability::AutoreportTemp => "AUTOREPORT_TEMP",
            Capability::AutoreportPos => "AUTOREPORT_POS",
            Capability::EmergencyParser => "EMERGENCY_PARSER",
            Capability::AutoreportSdStatus => "AUTOREPORT_SD_STATUS",
            Capability::Arcs => "ARCS",
            Capability::HostActionCommands => "HOST_ACTION_COMMANDS",
            Capability::BuildPercent => "BUILD_PERCENT",
            Capability::Progress => "PROGRESS",
            Capability::AdvancedOk => "ADVANCED_OK",
        }
    }
}

impl InfoMap {
    /// Check if a named known capability is supported on this device.
    pub fn has_capability(&self, capability: Capability) -> bool {
        self.0.get(capability.as_str()).is_some_and(Info::is_true)
    }
    /// Add a named known capability into this map, semantically meaning that this feature is supported.
    pub fn add_capability(&mut self, capability: Capability) {
        self.0.insert(capability.as_str().to_string(), Info::Key);
    }
    /// Remove a named known capability, semantically asserting that this feature is NOT supported.
    pub fn remove_capability(&mut self, capability: Capability) {
        self.0.remove(capability.as_str());
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn add_has_remove() {
        let mut info = InfoMap::default();
        assert!(!info.has_capability(Capability::Arcs));
        info.add_capability(Capability::Arcs);
        assert!(info.has_capability(Capability::Arcs));
        info.remove_capability(Capability::Arcs);
        assert!(!info.has_capability(Capability::Arcs));
    }

    #[test]
    fn info_truth() {
        assert!(Info::default().is_true());
        assert!(Info::Bool(true).is_true());
        assert!(!Info::Int(-1).is_true());
        assert!(!Info::Str(Default::default()).is_true());
        assert!(bool::from(Info::Float(6.9)));
    }

    #[test]
    fn map_conversion() {
        let mut map = InfoMapInner::new();
        map.insert("test".to_string(), Info::Key);
        let og = map.clone();
        let converted: InfoMap = map.into();
        assert_eq!(converted.deref(), &og);
        assert_eq!(og, converted.into())
    }

    #[test]
    fn info_conversion() {
        let cap = Capability::AutoreportPos;
        assert_eq!(cap.as_ref(), "AUTOREPORT_POS");
    }
}
