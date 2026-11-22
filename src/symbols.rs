//! Pure symbol geometry shared by the on-screen painter and the SVG exporter.
//!
//! A symbol is decomposed into simple [`Shape`] values in world coordinates.
//! The UI scales and translates them to screen space; the exporter writes them
//! as SVG primitives. Keeping this module UI-free means a symbol always looks
//! the same in both renderers.

use crate::document::ComponentKind;

/// A single drawable primitive of a component symbol.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    /// A straight stroke.
    Line {
        /// Start point.
        a: (f64, f64),
        /// End point.
        b: (f64, f64),
        /// Stroke width in world units.
        width: f64,
    },
    /// A circle, filled or stroked.
    Circle {
        /// Centre point.
        center: (f64, f64),
        /// Radius in world units.
        radius: f64,
        /// Whether the interior is filled.
        filled: bool,
    },
    /// An open or closed polyline.
    Poly {
        /// Vertices.
        points: Vec<(f64, f64)>,
        /// Whether to close the path.
        closed: bool,
        /// Stroke width in world units.
        width: f64,
    },
    /// Centred label text.
    Text {
        /// Centre of the text.
        pos: (f64, f64),
        /// Text content.
        text: String,
        /// Font size in world units.
        size: f64,
    },
}

fn line(a: (f64, f64), b: (f64, f64)) -> Shape {
    Shape::Line { a, b, width: 2.0 }
}

fn poly(points: Vec<(f64, f64)>, closed: bool) -> Shape {
    Shape::Poly {
        points,
        closed,
        width: 2.0,
    }
}

fn pin_markers(kind: ComponentKind, x: f64, y: f64) -> Vec<Shape> {
    kind.pin_layout()
        .iter()
        .map(|(_, dx, dy)| Shape::Circle {
            center: (x + dx, y + dy),
            radius: 3.5,
            filled: true,
        })
        .collect()
}

/// Build the shapes of a component symbol plus its label and value.
#[must_use]
pub fn symbol_shapes(kind: ComponentKind, x: f64, y: f64, label: &str, value: &str) -> Vec<Shape> {
    let mut shapes = Vec::new();
    match kind {
        ComponentKind::Resistor => {
            let (left, right) = ((x - 40.0, y), (x + 40.0, y));
            shapes.push(line(left, (x - 24.0, y)));
            let mut points = vec![(x - 24.0, y), (x - 18.0, y - 9.0)];
            for (index, offset) in [-10.0, -2.0, 6.0, 14.0].iter().enumerate() {
                let amplitude = if index % 2 == 0 { 9.0 } else { -9.0 };
                points.push((x + offset, y + amplitude));
            }
            points.extend([(x + 22.0, y - 9.0), (x + 24.0, y), right]);
            shapes.push(poly(points, false));
        }
        ComponentKind::Capacitor => {
            shapes.push(line((x - 40.0, y), (x - 9.0, y)));
            shapes.push(Shape::Line {
                a: (x - 9.0, y - 17.0),
                b: (x - 9.0, y + 17.0),
                width: 3.0,
            });
            shapes.push(Shape::Line {
                a: (x + 9.0, y - 17.0),
                b: (x + 9.0, y + 17.0),
                width: 3.0,
            });
            shapes.push(line((x + 9.0, y), (x + 40.0, y)));
        }
        ComponentKind::Inductor => {
            shapes.push(line((x - 40.0, y), (x - 24.0, y)));
            let mut points = vec![(x - 24.0, y)];
            for bump in 0..4 {
                let center_x = x - 18.0 + bump as f64 * 12.0;
                for step in 0..=8 {
                    let theta = std::f64::consts::PI * (1.0 - step as f64 / 8.0);
                    points.push((center_x + 6.0 * theta.cos(), y - 6.0 * theta.sin()));
                }
            }
            points.push((x + 24.0, y));
            shapes.push(poly(points, false));
            shapes.push(line((x + 24.0, y), (x + 40.0, y)));
        }
        ComponentKind::Diode => {
            shapes.push(line((x - 40.0, y), (x - 16.0, y)));
            shapes.push(poly(
                vec![(x - 16.0, y - 15.0), (x - 16.0, y + 15.0), (x + 12.0, y)],
                true,
            ));
            shapes.push(Shape::Line {
                a: (x + 12.0, y - 15.0),
                b: (x + 12.0, y + 15.0),
                width: 3.0,
            });
            shapes.push(line((x + 12.0, y), (x + 40.0, y)));
        }
        ComponentKind::Ground => {
            shapes.push(line((x, y), (x, y + 14.0)));
            for (offset, half_width) in [(14.0, 18.0), (21.0, 11.0), (28.0, 4.0)] {
                shapes.push(Shape::Line {
                    a: (x - half_width, y + offset),
                    b: (x + half_width, y + offset),
                    width: 2.5,
                });
            }
        }
        ComponentKind::VoltageSource => {
            shapes.push(line((x, y - 40.0), (x, y - 20.0)));
            shapes.push(Shape::Circle {
                center: (x, y),
                radius: 20.0,
                filled: false,
            });
            shapes.push(line((x - 6.0, y - 9.0), (x + 6.0, y - 9.0)));
            shapes.push(line((x, y - 15.0), (x, y - 3.0)));
            shapes.push(line((x - 6.0, y + 9.0), (x + 6.0, y + 9.0)));
            shapes.push(line((x, y + 20.0), (x, y + 40.0)));
        }
        ComponentKind::Switch => {
            shapes.push(line((x - 40.0, y), (x - 18.0, y)));
            shapes.push(line((x + 18.0, y), (x + 40.0, y)));
            shapes.push(Shape::Circle {
                center: (x - 18.0, y),
                radius: 3.0,
                filled: true,
            });
            shapes.push(Shape::Circle {
                center: (x + 18.0, y),
                radius: 3.0,
                filled: true,
            });
            shapes.push(line((x - 16.0, y - 2.0), (x + 14.0, y - 14.0)));
        }
    }
    shapes.extend(pin_markers(kind, x, y));
    let label_y = if kind == ComponentKind::Ground || kind == ComponentKind::VoltageSource {
        y - 52.0
    } else {
        y - 30.0
    };
    shapes.push(Shape::Text {
        pos: (x, label_y),
        text: label.into(),
        size: 12.0,
    });
    if !value.is_empty() {
        shapes.push(Shape::Text {
            pos: (x, y + 34.0),
            text: value.into(),
            size: 10.0,
        });
    }
    shapes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_produces_shapes() {
        for kind in ComponentKind::ALL {
            let shapes = symbol_shapes(kind, 0.0, 0.0, "X", "V");
            assert!(!shapes.is_empty(), "no shapes for {}", kind.name());
            assert!(
                shapes
                    .iter()
                    .any(|shape| matches!(shape, Shape::Text { .. }))
            );
        }
    }
}
