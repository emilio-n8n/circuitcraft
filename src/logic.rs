//! Pure domain algorithms: netlist, BOM, CSV and SVG export.
//!
//! Nothing in this module depends on egui or the filesystem, so every rule is
//! covered by ordinary unit tests.

use std::collections::{BTreeMap, HashMap};

use crate::document::{Document, Pin};
use crate::symbols::{Shape, symbol_shapes};

/// A set of pins joined directly or transitively by wires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Net {
    /// Stable index of the net.
    pub id: usize,
    /// Pins belonging to the net, sorted by component id then pin name.
    pub pins: Vec<Pin>,
}

fn find(parent: &mut [usize], mut node: usize) -> usize {
    while parent[node] != node {
        let grandparent = parent[parent[node]];
        parent[node] = grandparent;
        node = grandparent;
    }
    node
}

fn union(parent: &mut [usize], a: usize, b: usize) {
    let root_a = find(parent, a);
    let root_b = find(parent, b);
    if root_a != root_b {
        parent[root_b] = root_a;
    }
}

/// Group every pin of every component into electrical nets.
///
/// Two pins share a net when a chain of wires connects them. Unconnected pins
/// appear as singleton nets, and pins referenced by a wire but missing from the
/// component list are still represented defensively.
#[must_use]
pub fn netlist(document: &Document) -> Vec<Net> {
    let mut pins: Vec<Pin> = Vec::new();
    for component in &document.components {
        for name in component.kind.pins() {
            pins.push(Pin::new(component.id, *name));
        }
    }
    for wire in &document.wires {
        if !pins.contains(&wire.a) {
            pins.push(wire.a.clone());
        }
        if !pins.contains(&wire.b) {
            pins.push(wire.b.clone());
        }
    }

    let index: HashMap<&Pin, usize> = pins.iter().enumerate().map(|(i, pin)| (pin, i)).collect();
    let mut parent: Vec<usize> = (0..pins.len()).collect();
    for wire in &document.wires {
        if let (Some(&a), Some(&b)) = (index.get(&wire.a), index.get(&wire.b)) {
            union(&mut parent, a, b);
        }
    }

    let mut groups: HashMap<usize, Vec<Pin>> = HashMap::new();
    for (position, pin) in pins.into_iter().enumerate() {
        let root = find(&mut parent, position);
        groups.entry(root).or_default().push(pin);
    }

    let mut grouped: Vec<Vec<Pin>> = groups.into_values().collect();
    for group in &mut grouped {
        group.sort();
    }
    grouped.sort_by(|a, b| a.first().cmp(&b.first()));
    grouped
        .into_iter()
        .enumerate()
        .map(|(id, pins)| Net { id, pins })
        .collect()
}

/// Human-readable description of a net using component labels.
#[must_use]
pub fn net_label(document: &Document, net: &Net) -> String {
    net.pins
        .iter()
        .map(|pin| {
            document
                .components
                .iter()
                .find(|component| component.id == pin.component)
                .map_or_else(
                    || format!("?{}:{}", pin.component, pin.name),
                    |component| format!("{}.{}", component.label, pin.name),
                )
        })
        .collect::<Vec<_>>()
        .join(" — ")
}

/// One line of the bill of materials.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BomLine {
    /// Component kind.
    pub kind: crate::document::ComponentKind,
    /// Component value.
    pub value: String,
    /// Number of placed components with this kind and value.
    pub quantity: usize,
}

/// Aggregate placed components by `(kind, value)`.
#[must_use]
pub fn bom(document: &Document) -> Vec<BomLine> {
    let mut counts: BTreeMap<(crate::document::ComponentKind, String), usize> = BTreeMap::new();
    for component in &document.components {
        *counts
            .entry((component.kind, component.value.clone()))
            .or_insert(0) += 1;
    }
    counts
        .into_iter()
        .map(|((kind, value), quantity)| BomLine {
            kind,
            value,
            quantity,
        })
        .collect()
}

/// Escape a field for RFC 4180 CSV.
#[must_use]
pub fn csv_escape(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.into()
    }
}

/// Render the BOM as CSV with a header row.
#[must_use]
pub fn bom_csv(document: &Document) -> String {
    let mut output = String::from("kind,value,quantity\n");
    for line in bom(document) {
        output.push_str(&format!(
            "{},{},{}\n",
            csv_escape(line.kind.name()),
            csv_escape(&line.value),
            line.quantity
        ));
    }
    output
}

/// Escape text for inclusion in an XML/SVG element.
#[must_use]
pub fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn number(value: f64) -> String {
    format!("{value:.2}")
}

fn shape_svg(shape: &Shape) -> String {
    match shape {
        Shape::Line { a, b, width } => format!(
            "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"#1f2937\" stroke-width=\"{}\" stroke-linecap=\"round\"/>",
            number(a.0),
            number(a.1),
            number(b.0),
            number(b.1),
            number(*width)
        ),
        Shape::Circle {
            center,
            radius,
            filled,
        } => {
            let fill = if *filled { "#1f2937" } else { "none" };
            format!(
                "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{}\" stroke=\"#1f2937\" stroke-width=\"2\"/>",
                number(center.0),
                number(center.1),
                number(*radius),
                fill
            )
        }
        Shape::Poly {
            points,
            closed,
            width,
        } => {
            let coordinates = points
                .iter()
                .map(|(x, y)| format!("{},{}", number(*x), number(*y)))
                .collect::<Vec<_>>()
                .join(" ");
            let tag = if *closed { "polygon" } else { "polyline" };
            let fill = if *closed { "#dbeafe" } else { "none" };
            format!(
                "<{tag} points=\"{coordinates}\" fill=\"{fill}\" stroke=\"#1f2937\" stroke-width=\"{}\" stroke-linejoin=\"round\" stroke-linecap=\"round\"/>",
                number(*width)
            )
        }
        Shape::Text { pos, text, size } => format!(
            "<text x=\"{}\" y=\"{}\" text-anchor=\"middle\" font-family=\"sans-serif\" font-size=\"{}\" fill=\"#111827\">{}</text>",
            number(pos.0),
            number(pos.1 + size * 0.35),
            number(*size),
            escape_xml(text)
        ),
    }
}

/// Render the whole schematic as a standalone SVG document.
#[must_use]
pub fn document_svg(document: &Document) -> String {
    let mut pins: Vec<(Pin, (f64, f64))> = Vec::new();
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for component in &document.components {
        min_x = min_x.min(component.x - 60.0);
        min_y = min_y.min(component.y - 60.0);
        max_x = max_x.max(component.x + 60.0);
        max_y = max_y.max(component.y + 60.0);
        for name in component.kind.pins() {
            if let Some(position) = component.pin_position(name) {
                pins.push((Pin::new(component.id, *name), position));
            }
        }
    }
    if min_x.is_infinite() {
        min_x = 0.0;
        min_y = 0.0;
        max_x = 400.0;
        max_y = 200.0;
    }
    let padding = 20.0;
    min_x -= padding;
    min_y -= padding;
    max_x += padding;
    max_y += padding;

    let mut output = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{} {} {} {}\" width=\"{}\" height=\"{}\">\n",
        number(min_x),
        number(min_y),
        number(max_x - min_x),
        number(max_y - min_y),
        number(max_x - min_x),
        number(max_y - min_y)
    );
    output.push_str(&format!(
        "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#ffffff\"/>\n",
        number(min_x),
        number(min_y),
        number(max_x - min_x),
        number(max_y - min_y)
    ));

    for wire in &document.wires {
        let start = pins
            .iter()
            .find(|(pin, _)| *pin == wire.a)
            .map(|(_, position)| *position);
        let end = pins
            .iter()
            .find(|(pin, _)| *pin == wire.b)
            .map(|(_, position)| *position);
        if let (Some(a), Some(b)) = (start, end) {
            output.push_str(&format!(
                "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"#2563eb\" stroke-width=\"2\"/>\n",
                number(a.0),
                number(a.1),
                number(b.0),
                number(b.1)
            ));
        }
    }

    for component in &document.components {
        for shape in symbol_shapes(
            component.kind,
            component.x,
            component.y,
            &component.label,
            &component.value,
        ) {
            output.push_str(&shape_svg(&shape));
            output.push('\n');
        }
    }
    output.push_str("</svg>\n");
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Component, ComponentKind, Pin};
    use craftkit_core::History;

    #[test]
    fn netlist_groups_connected_pins() {
        let nets = netlist(&Document::sample());
        assert_eq!(nets.len(), 3);
        let ground_net = nets
            .iter()
            .find(|net| net.pins.contains(&Pin::new(4, "1")))
            .expect("ground net");
        assert_eq!(ground_net.pins.len(), 3);
        assert!(ground_net.pins.contains(&Pin::new(3, "K")));
        assert!(ground_net.pins.contains(&Pin::new(1, "-")));

        let source_plus = nets
            .iter()
            .find(|net| net.pins.contains(&Pin::new(1, "+")))
            .expect("source plus net");
        assert_eq!(source_plus.pins, vec![Pin::new(1, "+"), Pin::new(2, "1")]);
    }

    #[test]
    fn netlist_keeps_isolated_pins_as_singletons() {
        let mut document = Document::default();
        document.add_component(ComponentKind::Resistor, 0.0, 0.0);
        let nets = netlist(&document);
        assert_eq!(nets.len(), 2);
        assert!(nets.iter().all(|net| net.pins.len() == 1));
    }

    #[test]
    fn bom_groups_by_kind_and_value() {
        let mut document = Document::sample();
        let duplicate = Component {
            id: document.next_id,
            kind: ComponentKind::Resistor,
            x: 0.0,
            y: 0.0,
            label: "R2".into(),
            value: "330R".into(),
        };
        document.next_id += 1;
        document.components.push(duplicate);

        let lines = bom(&document);
        let resistor = lines
            .iter()
            .find(|line| line.kind == ComponentKind::Resistor && line.value == "330R")
            .expect("resistor line");
        assert_eq!(resistor.quantity, 2);
        assert_eq!(lines.len(), 4);
    }

    #[test]
    fn csv_escapes_commas_and_quotes() {
        assert_eq!(csv_escape("plain"), "plain");
        assert_eq!(csv_escape("a,b"), "\"a,b\"");
        assert_eq!(csv_escape("a\"b"), "\"a\"\"b\"");
        let mut document = Document::default();
        document.components.push(Component {
            id: 1,
            kind: ComponentKind::Resistor,
            x: 0.0,
            y: 0.0,
            label: "R1".into(),
            value: "a,b\"c".into(),
        });
        let csv = bom_csv(&document);
        assert!(csv.starts_with("kind,value,quantity\n"));
        assert!(csv.contains("\"a,b\"\"c\",1\n"));
    }

    #[test]
    fn svg_is_non_empty_and_escapes_labels() {
        let mut document = Document::sample();
        document.components[0].label = "<V&\"'>".into();
        let svg = document_svg(&document);
        assert!(svg.starts_with("<svg"));
        assert!(svg.ends_with("</svg>\n"));
        assert!(svg.contains("&lt;V&amp;&quot;&apos;&gt;"));
        assert!(!svg.contains("<V&\"'>"));
        assert!(svg.contains("#2563eb"));
    }

    #[test]
    fn json_round_trip_preserves_document() {
        let document = Document::sample();
        let bytes = serde_json::to_vec(&document).expect("serialize");
        let restored: Document = serde_json::from_slice(&bytes).expect("deserialize");
        assert_eq!(restored, document);
    }

    #[test]
    fn undo_redo_of_a_move() {
        let mut history = History::new(Document::sample());
        let original = history.current().components[1].x;
        assert!(history.apply(|document| document.components[1].x += 25.0));
        assert_eq!(history.current().components[1].x, original + 25.0);
        assert!(history.undo());
        assert_eq!(history.current().components[1].x, original);
        assert!(history.redo());
        assert_eq!(history.current().components[1].x, original + 25.0);
    }
}
