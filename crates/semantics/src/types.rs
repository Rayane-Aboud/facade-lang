use crate::dimension::Dimension;
use std::fmt;

pub const GEOMETRY: &[&str] = &["Curve", "Surface", "Solid"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ty {
    Quantity(Dimension),
    Kind(String),
    Geometry(String),
}

impl Ty {
    pub fn is_quantity(&self) -> bool {
        !matches!(self, Ty::Geometry(_))
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::Quantity(d) => write!(f, "{d}"),
            Ty::Kind(name) | Ty::Geometry(name) => write!(f, "{name}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_shows_dimension_or_name() {
        let volume = Ty::Quantity(Dimension::base("Length").pow(3));
        assert_eq!(volume.to_string(), "Length^3");
        assert_eq!(Ty::Kind("Thickness".to_string()).to_string(), "Thickness");
    }

    #[test]
    fn geometry_is_not_a_quantity() {
        assert!(Ty::Kind("Thickness".to_string()).is_quantity());
        assert!(!Ty::Geometry("Surface".to_string()).is_quantity());
    }
}
