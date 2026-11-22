//! Circuit document model.
//!
//! The document is a self-contained schematic: components, the wires between
//! their pins, and an id counter. It has no UI or rendering dependency so it can
//! be serialized, diffed by the undo history and processed by pure domain
//! functions.

use serde::{Deserialize, Serialize};

/// The kind of an electronic component placed on the schematic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ComponentKind {
    /// Two-terminal resistor.
    Resistor,
    /// Two-terminal capacitor.
    Capacitor,
    /// Two-terminal inductor.
    Inductor,
    /// Diode (also used for LEDs).
    Diode,
    /// Single-terminal ground reference.
    Ground,
    /// Voltage source with a positive and a negative terminal.
    VoltageSource,
    /// Two-terminal switch.
    Switch,
}

impl ComponentKind {
    /// All kinds, in palette order.
    pub const ALL: [Self; 7] = [
        Self::Resistor,
        Self::Capacitor,
        Self::Inductor,
        Self::Diode,
        Self::Ground,
        Self::VoltageSource,
        Self::Switch,
    ];

    /// Short human-readable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Resistor => "Resistor",
            Self::Capacitor => "Capacitor",
            Self::Inductor => "Inductor",
            Self::Diode => "Diode",
            Self::Ground => "Ground",
            Self::VoltageSource => "Voltage source",
            Self::Switch => "Switch",
        }
    }

    /// Suggested initial value shown in the properties panel and the BOM.
    #[must_use]
    pub const fn default_value(self) -> &'static str {
        match self {
            Self::Resistor => "330R",
            Self::Capacitor => "100nF",
            Self::Inductor => "10mH",
            Self::Diode => "LED",
            Self::Ground => "",
            Self::VoltageSource => "5V",
            Self::Switch => "SW",
        }
    }

    /// Pin names exposed by this kind.
    #[must_use]
    pub const fn pins(self) -> &'static [&'static str] {
        match self {
            Self::Resistor | Self::Capacitor | Self::Inductor | Self::Switch => &["1", "2"],
            Self::Diode => &["A", "K"],
            Self::Ground => &["1"],
            Self::VoltageSource => &["+", "-"],
        }
    }

    /// Pin layout as `(name, dx, dy)` offsets from the component centre.
    #[must_use]
    pub const fn pin_layout(self) -> &'static [(&'static str, f64, f64)] {
        match self {
            Self::Resistor | Self::Capacitor | Self::Inductor | Self::Switch => {
                &[("1", -40.0, 0.0), ("2", 40.0, 0.0)]
            }
            Self::Diode => &[("A", -40.0, 0.0), ("K", 40.0, 0.0)],
            Self::Ground => &[("1", 0.0, 0.0)],
            Self::VoltageSource => &[("+", 0.0, -40.0), ("-", 0.0, 40.0)],
        }
    }

    /// Offset of a named pin, if the name exists on this kind.
    #[must_use]
    pub fn pin_offset(self, name: &str) -> Option<(f64, f64)> {
        self.pin_layout()
            .iter()
            .find(|(pin, _, _)| *pin == name)
            .map(|(_, dx, dy)| (*dx, *dy))
    }
}

/// A reference to one pin of one component.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Pin {
    /// Id of the owning component.
    pub component: u64,
    /// Pin name, for example `1`, `A` or `+`.
    pub name: String,
}

impl Pin {
    /// Build a pin reference.
    #[must_use]
    pub fn new(component: u64, name: impl Into<String>) -> Self {
        Self {
            component,
            name: name.into(),
        }
    }
}

/// A placed stencil instance on the schematic.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Component {
    /// Stable identifier.
    pub id: u64,
    /// Component kind, which determines the symbol and its pins.
    pub kind: ComponentKind,
    /// World-space x coordinate of the symbol centre.
    pub x: f64,
    /// World-space y coordinate of the symbol centre.
    pub y: f64,
    /// Editable reference designator, for example `R1`.
    pub label: String,
    /// Editable value, for example `330R`.
    pub value: String,
}

impl Component {
    /// World-space position of a named pin.
    #[must_use]
    pub fn pin_position(&self, name: &str) -> Option<(f64, f64)> {
        let (dx, dy) = self.kind.pin_offset(name)?;
        Some((self.x + dx, self.y + dy))
    }
}

/// A wire joining two pins.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Wire {
    /// Stable identifier.
    pub id: u64,
    /// First endpoint.
    pub a: Pin,
    /// Second endpoint.
    pub b: Pin,
}

/// A self-contained circuit schematic.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    /// Document title.
    pub title: String,
    /// Placed components.
    pub components: Vec<Component>,
    /// Wires between component pins.
    pub wires: Vec<Wire>,
    /// Next id handed out for components and wires.
    pub next_id: u64,
}

impl Default for Document {
    fn default() -> Self {
        Self {
            title: "Untitled".into(),
            components: Vec::new(),
            wires: Vec::new(),
            next_id: 1,
        }
    }
}

impl Document {
    /// Built-in demonstration circuit: source → resistor → LED → ground.
    #[must_use]
    pub fn sample() -> Self {
        let component = |id, kind, x, y, label: &str, value: &str| Component {
            id,
            kind,
            x,
            y,
            label: label.into(),
            value: value.into(),
        };
        let wire = |id, a: Pin, b: Pin| Wire { id, a, b };
        Self {
            title: "CircuitCraft sample".into(),
            components: vec![
                component(1, ComponentKind::VoltageSource, 0.0, 0.0, "V1", "5V"),
                component(2, ComponentKind::Resistor, 160.0, -40.0, "R1", "330R"),
                component(3, ComponentKind::Diode, 320.0, -40.0, "D1", "LED"),
                component(4, ComponentKind::Ground, 480.0, -40.0, "GND", ""),
            ],
            wires: vec![
                wire(5, Pin::new(1, "+"), Pin::new(2, "1")),
                wire(6, Pin::new(2, "2"), Pin::new(3, "A")),
                wire(7, Pin::new(3, "K"), Pin::new(4, "1")),
                wire(8, Pin::new(4, "1"), Pin::new(1, "-")),
            ],
            next_id: 9,
        }
    }

    /// Allocate an id and place a new component.
    pub fn add_component(&mut self, kind: ComponentKind, x: f64, y: f64) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let label = format!(
            "{}{}",
            kind.name().split_whitespace().next().unwrap_or("C"),
            id
        );
        self.components.push(Component {
            id,
            kind,
            x,
            y,
            label,
            value: kind.default_value().into(),
        });
        id
    }

    /// Allocate an id and add a wire between two pins.
    ///
    /// Returns `None` when both endpoints are the same pin.
    pub fn add_wire(&mut self, a: Pin, b: Pin) -> Option<u64> {
        if a == b {
            return None;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.wires.push(Wire { id, a, b });
        Some(id)
    }

    /// Remove a component and every wire attached to it.
    ///
    /// Returns `true` when a component was actually removed.
    pub fn remove_component(&mut self, id: u64) -> bool {
        let before = self.components.len();
        self.components.retain(|component| component.id != id);
        if self.components.len() == before {
            return false;
        }
        self.wires
            .retain(|wire| wire.a.component != id && wire.b.component != id);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_has_expected_shape() {
        let document = Document::sample();
        assert_eq!(document.components.len(), 4);
        assert_eq!(document.wires.len(), 4);
        assert_eq!(document.next_id, 9);
        assert_eq!(document.components[0].kind, ComponentKind::VoltageSource);
        assert_eq!(document.components[1].kind, ComponentKind::Resistor);
        assert_eq!(document.components[2].kind, ComponentKind::Diode);
        assert_eq!(document.components[3].kind, ComponentKind::Ground);
    }

    #[test]
    fn removing_a_component_also_removes_its_wires() {
        let mut document = Document::sample();
        assert!(document.remove_component(2));
        assert_eq!(document.components.len(), 3);
        assert_eq!(document.wires.len(), 2);
        assert!(
            document
                .wires
                .iter()
                .all(|wire| { wire.a.component != 2 && wire.b.component != 2 })
        );
        assert!(!document.remove_component(2));
    }

    #[test]
    fn pin_position_respects_kind() {
        let document = Document::sample();
        let resistor = &document.components[1];
        assert_eq!(resistor.pin_position("1"), Some((120.0, -40.0)));
        assert_eq!(resistor.pin_position("2"), Some((200.0, -40.0)));
        assert_eq!(resistor.pin_position("nope"), None);
    }
}
