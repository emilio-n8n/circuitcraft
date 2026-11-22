//! Pure domain algorithms: wire geometry, netlist, BOM and vector exports.
//!
//! Nothing in this module depends on egui or the filesystem, so every rule is
//! covered by ordinary unit tests. Exporters share the same wire routes and
//! symbol geometry, so SVG, PDF and the on-screen canvas stay consistent.

use std::collections::{BTreeMap, HashMap};

use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref, Str};

use crate::document::{Component, ComponentKind, Document, Pin, Wire, WireRoute};
use crate::symbols::{Shape, symbol_shapes};

type Point = (f64, f64);

/// A set of pins joined directly or transitively by wires and junctions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Net {
    /// Stable index of the net.
    pub id: usize,
    /// Readable name (`GND`, `V+ · V1` or `N<n>`).
    pub name: String,
    /// Pins belonging to the net, sorted by component id then pin name.
    pub pins: Vec<Pin>,
}

/// A point where at least two wires touch or cross.
#[derive(Debug, Clone, PartialEq)]
pub struct Junction {
    /// World-space position.
    pub point: Point,
    /// Ids of the wires meeting at this point.
    pub wires: Vec<u64>,
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

fn component_for<'a>(document: &'a Document, pin: &Pin) -> Option<&'a Component> {
    document
        .components
        .iter()
        .find(|component| component.id == pin.component)
}

/// Outward direction of a pin after component rotation.
fn pin_direction(document: &Document, pin: &Pin) -> Option<Point> {
    let component = component_for(document, pin)?;
    let offset = component.kind.pin_offset(&pin.name)?;
    let (dx, dy) = crate::document::rotate_offset(offset.0, offset.1, component.rotation);
    if component.kind == ComponentKind::Ground {
        // The ground pin has a zero-length offset; leave the symbol upwards so
        // the return wire does not run along the top rail.
        return Some((0.0, -1.0));
    }
    let length = (dx * dx + dy * dy).sqrt();
    if length < f64::EPSILON {
        None
    } else {
        Some((dx / length, dy / length))
    }
}

fn push_point(points: &mut Vec<Point>, point: Point) {
    if points
        .last()
        .is_none_or(|last| (last.0 - point.0).abs() > 1e-6 || (last.1 - point.1).abs() > 1e-6)
    {
        points.push(point);
    }
}

/// Waypoints of a wire in world coordinates.
///
/// Straight wires keep two points. Orthogonal wires leave each pin along its
/// outward direction and use at most two bends.
#[must_use]
pub fn wire_route(document: &Document, wire: &Wire) -> Vec<Point> {
    let (Some(a), Some(b)) = (
        component_for(document, &wire.a).and_then(|c| c.pin_position(&wire.a.name)),
        component_for(document, &wire.b).and_then(|c| c.pin_position(&wire.b.name)),
    ) else {
        return Vec::new();
    };
    if wire.route == WireRoute::Straight || a == b {
        return vec![a, b];
    }

    let a_horizontal = pin_direction(document, &wire.a)
        .is_none_or(|direction| direction.0.abs() >= direction.1.abs());
    let b_horizontal = pin_direction(document, &wire.b)
        .is_none_or(|direction| direction.0.abs() >= direction.1.abs());
    let mut points = Vec::new();
    push_point(&mut points, a);
    match (a_horizontal, b_horizontal) {
        (true, true) => {
            let middle_x = (a.0 + b.0) / 2.0;
            push_point(&mut points, (middle_x, a.1));
            push_point(&mut points, (middle_x, b.1));
        }
        (false, false) => {
            let middle_y = (a.1 + b.1) / 2.0;
            push_point(&mut points, (a.0, middle_y));
            push_point(&mut points, (b.0, middle_y));
        }
        (true, false) => push_point(&mut points, (b.0, a.1)),
        (false, true) => push_point(&mut points, (a.0, b.1)),
    }
    push_point(&mut points, b);
    points
}

fn cross(ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    ax * by - ay * bx
}

/// Intersection points of two segments, including overlaps and endpoints.
fn segment_intersections(a1: Point, a2: Point, b1: Point, b2: Point) -> Vec<Point> {
    let r = (a2.0 - a1.0, a2.1 - a1.1);
    let s = (b2.0 - b1.0, b2.1 - b1.1);
    let denominator = cross(r.0, r.1, s.0, s.1);
    let q_minus_p = (b1.0 - a1.0, b1.1 - a1.1);
    let epsilon = 1e-6;

    if denominator.abs() > epsilon {
        let t = cross(q_minus_p.0, q_minus_p.1, s.0, s.1) / denominator;
        let u = cross(q_minus_p.0, q_minus_p.1, r.0, r.1) / denominator;
        if (-epsilon..=1.0 + epsilon).contains(&t) && (-epsilon..=1.0 + epsilon).contains(&u) {
            return vec![(a1.0 + t * r.0, a1.1 + t * r.1)];
        }
        return Vec::new();
    }

    if cross(q_minus_p.0, q_minus_p.1, r.0, r.1).abs() > epsilon {
        return Vec::new(); // Parallel and distinct.
    }

    // Collinear: project both segments onto the longer one.
    let length_squared = r.0 * r.0 + r.1 * r.1;
    if length_squared < epsilon {
        return if (a1.0 - b1.0).abs() < epsilon && (a1.1 - b1.1).abs() < epsilon {
            vec![a1]
        } else {
            Vec::new()
        };
    }
    let t1 = ((b1.0 - a1.0) * r.0 + (b1.1 - a1.1) * r.1) / length_squared;
    let t2 = ((b2.0 - a1.0) * r.0 + (b2.1 - a1.1) * r.1) / length_squared;
    let (low, high) = if t1 <= t2 { (t1, t2) } else { (t2, t1) };
    if high < -epsilon || low > 1.0 + epsilon {
        return Vec::new();
    }
    let start = low.max(0.0);
    let end = high.min(1.0);
    if (end - start).abs() < epsilon {
        return vec![(a1.0 + start * r.0, a1.1 + start * r.1)];
    }
    vec![
        (a1.0 + start * r.0, a1.1 + start * r.1),
        (a1.0 + end * r.0, a1.1 + end * r.1),
    ]
}

/// Find every point where two different wires touch or cross.
#[must_use]
pub fn junctions(document: &Document) -> Vec<Junction> {
    let routes: Vec<(&Wire, Vec<Point>)> = document
        .wires
        .iter()
        .map(|wire| (wire, wire_route(document, wire)))
        .filter(|(_, points)| points.len() >= 2)
        .collect();
    let mut found: Vec<Junction> = Vec::new();
    for (index, (wire_a, route_a)) in routes.iter().enumerate() {
        for (wire_b, route_b) in routes.iter().skip(index + 1) {
            for segment_a in route_a.windows(2) {
                for segment_b in route_b.windows(2) {
                    for point in segment_intersections(
                        segment_a[0],
                        segment_a[1],
                        segment_b[0],
                        segment_b[1],
                    ) {
                        if let Some(junction) = found.iter_mut().find(|junction| {
                            (junction.point.0 - point.0).abs() < 0.01
                                && (junction.point.1 - point.1).abs() < 0.01
                        }) {
                            if !junction.wires.contains(&wire_a.id) {
                                junction.wires.push(wire_a.id);
                            }
                            if !junction.wires.contains(&wire_b.id) {
                                junction.wires.push(wire_b.id);
                            }
                        } else {
                            found.push(Junction {
                                point,
                                wires: vec![wire_a.id, wire_b.id],
                            });
                        }
                    }
                }
            }
        }
    }
    found.sort_by(|a, b| {
        a.point
            .0
            .total_cmp(&b.point.0)
            .then_with(|| a.point.1.total_cmp(&b.point.1))
    });
    found
}

fn net_name(document: &Document, pins: &[Pin], id: usize) -> String {
    if pins
        .iter()
        .any(|pin| component_for(document, pin).is_some_and(|c| c.kind == ComponentKind::Ground))
    {
        return "GND".into();
    }
    if pins.iter().any(|pin| {
        pin.name == "+"
            && component_for(document, pin).is_some_and(|c| c.kind == ComponentKind::VoltageSource)
    }) {
        return "V+".into();
    }
    format!("N{}", id + 1)
}

/// Group every pin of every component into named electrical nets.
///
/// Pins are joined by shared wires and by wire junctions. Unconnected pins
/// appear as singleton nets; pins referenced by a wire but missing from the
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
    let mut wire_representative: HashMap<u64, usize> = HashMap::new();
    for wire in &document.wires {
        if let (Some(&a), Some(&b)) = (index.get(&wire.a), index.get(&wire.b)) {
            union(&mut parent, a, b);
            wire_representative.insert(wire.id, a);
        }
    }
    for junction in junctions(document) {
        let representatives: Vec<usize> = junction
            .wires
            .iter()
            .filter_map(|id| wire_representative.get(id).copied())
            .collect();
        for pair in representatives.windows(2) {
            union(&mut parent, pair[0], pair[1]);
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
        .map(|(id, pins)| Net {
            name: net_name(document, &pins, id),
            id,
            pins,
        })
        .collect()
}

/// Human-readable description of a net using component labels.
#[must_use]
pub fn net_label(document: &Document, net: &Net) -> String {
    net.pins
        .iter()
        .map(|pin| {
            component_for(document, pin).map_or_else(
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
    pub kind: ComponentKind,
    /// Component value.
    pub value: String,
    /// Number of placed components with this kind and value.
    pub quantity: usize,
}

/// Aggregate placed components by `(kind, value)`.
#[must_use]
pub fn bom(document: &Document) -> Vec<BomLine> {
    let mut counts: BTreeMap<(ComponentKind, String), usize> = BTreeMap::new();
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

fn schematic_bounds(document: &Document) -> (f64, f64, f64, f64) {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for component in &document.components {
        for name in component.kind.pins() {
            if let Some((x, y)) = component.pin_position(name) {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
        min_x = min_x.min(component.x - 60.0);
        min_y = min_y.min(component.y - 70.0);
        max_x = max_x.max(component.x + 60.0);
        max_y = max_y.max(component.y + 70.0);
    }
    if min_x.is_infinite() {
        (0.0, 0.0, 400.0, 200.0)
    } else {
        (min_x - 20.0, min_y - 20.0, max_x + 20.0, max_y + 20.0)
    }
}

/// Render the whole schematic as a standalone SVG document.
#[must_use]
pub fn document_svg(document: &Document) -> String {
    let (min_x, min_y, max_x, max_y) = schematic_bounds(document);
    let width = max_x - min_x;
    let height = max_y - min_y;
    let mut output = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{} {} {} {}\" width=\"{}\" height=\"{}\">\n",
        number(min_x),
        number(min_y),
        number(width),
        number(height),
        number(width),
        number(height)
    );
    output.push_str(&format!(
        "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#ffffff\"/>\n",
        number(min_x),
        number(min_y),
        number(width),
        number(height)
    ));

    for wire in &document.wires {
        let route = wire_route(document, wire);
        if route.len() < 2 {
            continue;
        }
        let points = route
            .iter()
            .map(|(x, y)| format!("{},{}", number(*x), number(*y)))
            .collect::<Vec<_>>()
            .join(" ");
        output.push_str(&format!(
            "<polyline points=\"{points}\" fill=\"none\" stroke=\"#2563eb\" stroke-width=\"2\" stroke-linejoin=\"round\"/>\n"
        ));
    }
    for junction in junctions(document) {
        output.push_str(&format!(
            "<circle cx=\"{}\" cy=\"{}\" r=\"4\" fill=\"#2563eb\"/>\n",
            number(junction.point.0),
            number(junction.point.1)
        ));
    }
    for component in &document.components {
        for shape in symbol_shapes(
            component.kind,
            component.x,
            component.y,
            component.rotation,
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

/// Escape a string for a PDF literal string.
fn pdf_escape(value: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\\' | '(' | ')' => {
                bytes.push(b'\\');
                bytes.push(character as u8);
            }
            c if c.is_ascii_graphic() || c == ' ' => bytes.push(c as u8),
            _ => bytes.push(b'?'),
        }
    }
    bytes
}

fn pdf_circle(content: &mut Content, center: (f32, f32), radius: f32) {
    const SEGMENTS: usize = 12;
    for step in 0..=SEGMENTS {
        let angle = std::f32::consts::TAU * step as f32 / SEGMENTS as f32;
        let x = center.0 + radius * angle.cos();
        let y = center.1 + radius * angle.sin();
        if step == 0 {
            content.move_to(x, y);
        } else {
            content.line_to(x, y);
        }
    }
    content.close_path().stroke();
}

/// Render the whole schematic as a single-page PDF.
///
/// Uses the standard Helvetica font (no embedding) and one point per world
/// unit. Non-ASCII characters are replaced by `?` because the base font uses
/// StandardEncoding.
#[must_use]
pub fn document_pdf(document: &Document) -> Vec<u8> {
    let (min_x, min_y, max_x, max_y) = schematic_bounds(document);
    let padding = 24.0_f64;
    let width = (max_x - min_x + padding * 2.0) as f32;
    let height = (max_y - min_y + padding * 2.0) as f32;
    let transform = |point: Point| {
        (
            (point.0 - min_x + padding) as f32,
            (max_y - point.1 + padding) as f32,
        )
    };

    let catalog_id = Ref::new(1);
    let page_tree_id = Ref::new(2);
    let page_id = Ref::new(3);
    let font_id = Ref::new(4);
    let content_id = Ref::new(5);
    let font_name = Name(b"F1");

    let mut content = Content::new();
    content.set_line_width(2.0);
    content.set_stroke_rgb(0.10, 0.12, 0.18);
    content.set_fill_rgb(0.10, 0.12, 0.18);

    for wire in &document.wires {
        for pair in wire_route(document, wire).windows(2) {
            let (x1, y1) = transform(pair[0]);
            let (x2, y2) = transform(pair[1]);
            content.move_to(x1, y1);
            content.line_to(x2, y2);
            content.stroke();
        }
    }
    for junction in junctions(document) {
        let (x, y) = transform(junction.point);
        content.rect(x - 3.0, y - 3.0, 6.0, 6.0);
        content.fill_nonzero();
    }
    for component in &document.components {
        for shape in symbol_shapes(
            component.kind,
            component.x,
            component.y,
            component.rotation,
            &component.label,
            &component.value,
        ) {
            match shape {
                Shape::Line { a, b, width } => {
                    let (x1, y1) = transform(a);
                    let (x2, y2) = transform(b);
                    content.set_line_width(width as f32);
                    content.move_to(x1, y1);
                    content.line_to(x2, y2);
                    content.stroke();
                }
                Shape::Circle {
                    center,
                    radius,
                    filled,
                } => {
                    let (center, radius) = (transform(center), radius as f32);
                    if filled {
                        content.set_fill_rgb(0.91, 0.34, 0.24);
                    }
                    pdf_circle(&mut content, center, radius);
                    if filled {
                        content.set_fill_rgb(0.10, 0.12, 0.18);
                    }
                }
                Shape::Poly {
                    points,
                    closed,
                    width,
                } => {
                    content.set_line_width(width as f32);
                    for (index, point) in points.iter().enumerate() {
                        let (x, y) = transform(*point);
                        if index == 0 {
                            content.move_to(x, y);
                        } else {
                            content.line_to(x, y);
                        }
                    }
                    if closed {
                        content.close_path();
                    }
                    content.stroke();
                }
                Shape::Text { pos, text, size } => {
                    let (x, y) = transform(pos);
                    let encoded = pdf_escape(&text);
                    content.begin_text();
                    content.set_font(font_name, size as f32);
                    content.set_text_matrix([1.0, 0.0, 0.0, 1.0, x, y]);
                    content.show(Str(&encoded));
                    content.end_text();
                }
            }
        }
    }

    let mut pdf = Pdf::new();
    pdf.catalog(catalog_id).pages(page_tree_id);
    pdf.pages(page_tree_id).kids([page_id]).count(1);
    let mut page = pdf.page(page_id);
    page.media_box(Rect::new(0.0, 0.0, width, height));
    page.parent(page_tree_id);
    page.contents(content_id);
    page.resources().fonts().pair(font_name, font_id);
    page.finish();
    pdf.type1_font(font_id).base_font(Name(b"Helvetica"));
    pdf.stream(content_id, &content.finish());
    pdf.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::WireRoute;
    use craftkit_core::History;

    #[test]
    fn netlist_groups_connected_pins_with_names() {
        let nets = netlist(&Document::sample());
        assert_eq!(nets.len(), 3);
        let ground = nets
            .iter()
            .find(|net| net.pins.contains(&Pin::new(4, "1")))
            .expect("ground net");
        assert_eq!(ground.name, "GND");
        assert_eq!(ground.pins.len(), 3);
        assert!(ground.pins.contains(&Pin::new(3, "K")));
        assert!(ground.pins.contains(&Pin::new(1, "-")));

        let source_plus = nets
            .iter()
            .find(|net| net.pins.contains(&Pin::new(1, "+")))
            .expect("source plus net");
        assert_eq!(source_plus.name, "V+");
        assert_eq!(source_plus.pins, vec![Pin::new(1, "+"), Pin::new(2, "1")]);

        let remaining = nets
            .iter()
            .find(|net| net.name.starts_with('N'))
            .expect("numbered net");
        assert_eq!(remaining.pins, vec![Pin::new(2, "2"), Pin::new(3, "A")]);
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
    fn orthogonal_wires_are_axis_aligned() {
        let document = Document::sample();
        for wire in &document.wires {
            let route = wire_route(&document, wire);
            assert!(route.len() >= 2);
            for pair in route.windows(2) {
                assert!(
                    (pair[0].0 - pair[1].0).abs() < 1e-6 || (pair[0].1 - pair[1].1).abs() < 1e-6,
                    "segment must be horizontal or vertical: {pair:?}"
                );
            }
        }
    }

    #[test]
    fn junctions_detect_shared_pins_and_crossings() {
        let document = Document::sample();
        let found = junctions(&document);
        assert!(
            found.iter().any(
                |junction| (junction.point.0 - 480.0).abs() < 1e-6 && junction.wires.len() == 2
            ),
            "ground pin should join two wires"
        );

        // Crossing wire segments are detected even without a shared pin.
        let crossing =
            segment_intersections((100.0, 0.0), (100.0, 200.0), (0.0, 100.0), (200.0, 100.0));
        assert_eq!(crossing, vec![(100.0, 100.0)]);
    }

    #[test]
    fn junction_merges_two_wires_into_one_net() {
        let mut document = Document::default();
        let source = document.add_component(ComponentKind::VoltageSource, 0.0, 0.0);
        let left = document.add_component(ComponentKind::Resistor, 120.0, -40.0);
        let right = document.add_component(ComponentKind::Resistor, 120.0, 80.0);
        let ground = document.add_component(ComponentKind::Ground, 240.0, 20.0);
        document.add_wire(Pin::new(source, "+"), Pin::new(left, "1"));
        document.add_wire(Pin::new(source, "-"), Pin::new(right, "1"));
        document.add_wire(Pin::new(left, "2"), Pin::new(ground, "1"));
        document.add_wire(Pin::new(right, "2"), Pin::new(ground, "1"));
        let nets = netlist(&document);
        let ground_net = nets
            .iter()
            .find(|net| net.pins.contains(&Pin::new(ground, "1")))
            .expect("ground net");
        assert!(ground_net.pins.contains(&Pin::new(left, "2")));
        assert!(ground_net.pins.contains(&Pin::new(right, "2")));
        assert_eq!(ground_net.name, "GND");
        assert_eq!(junctions(&document).len(), 1);
    }

    #[test]
    fn segment_intersection_handles_cross_and_overlap() {
        let cross = segment_intersections((0.0, 5.0), (10.0, 5.0), (5.0, 0.0), (5.0, 10.0));
        assert_eq!(cross, vec![(5.0, 5.0)]);
        let overlap = segment_intersections((0.0, 0.0), (10.0, 0.0), (5.0, 0.0), (15.0, 0.0));
        assert_eq!(overlap.len(), 2);
        assert!(segment_intersections((0.0, 0.0), (1.0, 0.0), (0.0, 2.0), (1.0, 2.0)).is_empty());
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
            rotation: 0,
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
            rotation: 0,
        });
        let csv = bom_csv(&document);
        assert!(csv.starts_with("kind,value,quantity\n"));
        assert!(csv.contains("\"a,b\"\"c\",1\n"));
    }

    #[test]
    fn svg_is_non_empty_escaped_and_draws_junctions() {
        let mut document = Document::sample();
        document.components[0].label = "<V&\"'>".into();
        let svg = document_svg(&document);
        assert!(svg.starts_with("<svg"));
        assert!(svg.ends_with("</svg>\n"));
        assert!(svg.contains("&lt;V&amp;&quot;&apos;&gt;"));
        assert!(!svg.contains("<V&\"'>"));
        assert!(svg.contains("<polyline"));
        assert!(svg.contains("<circle"));
    }

    #[test]
    fn pdf_is_valid_header_and_escapes_text() {
        let mut document = Document {
            title: "PDF test".into(),
            ..Document::default()
        };
        document.components.push(Component {
            id: 1,
            kind: ComponentKind::Resistor,
            x: 0.0,
            y: 0.0,
            label: "R(1)".into(),
            value: "10R".into(),
            rotation: 1,
        });
        let bytes = document_pdf(&document);
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.starts_with("%PDF-"), "PDF header missing");
        assert!(text.contains("%%EOF"), "PDF trailer missing");
        assert!(text.contains("/Helvetica"), "font resource missing");
        assert!(text.contains("BT"), "text object missing");
        assert_eq!(pdf_escape("R(1)"), b"R\\(1\\)", "literal string escaping");
        assert_eq!(pdf_escape("é"), b"?", "non-ASCII replacement");
    }

    #[test]
    fn json_round_trip_preserves_new_fields() {
        let mut document = Document::sample();
        document.rotate_component(2, 1);
        document.wires[0].route = WireRoute::Straight;
        let bytes = serde_json::to_vec(&document).expect("serialize");
        let restored: Document = serde_json::from_slice(&bytes).expect("deserialize");
        assert_eq!(restored, document);
    }

    #[test]
    fn undo_redo_of_a_move_and_rotation() {
        let mut history = History::new(Document::sample());
        let original = history.current().components[1].x;
        assert!(history.apply(|document| document.components[1].x += 25.0));
        assert_eq!(history.current().components[1].x, original + 25.0);
        assert!(history.undo());
        assert_eq!(history.current().components[1].x, original);
        assert!(history.redo());
        assert_eq!(history.current().components[1].x, original + 25.0);

        assert!(history.apply(|document| {
            document.rotate_component(2, 1);
        }));
        assert_eq!(history.current().components[1].rotation, 1);
        assert!(history.undo());
        assert_eq!(history.current().components[1].rotation, 0);
    }
}
