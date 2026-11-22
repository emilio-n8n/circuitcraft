//! Pure symbol geometry shared by the on-screen painter and the exporters.
//!
//! A symbol is decomposed into simple [`Shape`] values in world coordinates.
//! The UI scales and translates them to screen space; the exporters write them
//! as SVG or PDF primitives. Geometry is rotated around the component centre
//! while text stays upright and readable.

use crate::document::{ComponentKind, rotate_offset};

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

fn rotate_point(point: (f64, f64), center: (f64, f64), turns: u8) -> (f64, f64) {
    let (dx, dy) = rotate_offset(point.0 - center.0, point.1 - center.1, turns);
    (center.0 + dx, center.1 + dy)
}

fn rotate_shapes(shapes: Vec<Shape>, x: f64, y: f64, turns: u8) -> Vec<Shape> {
    if turns.is_multiple_of(4) {
        return shapes;
    }
    let center = (x, y);
    shapes
        .into_iter()
        .map(|shape| match shape {
            Shape::Line { a, b, width } => Shape::Line {
                a: rotate_point(a, center, turns),
                b: rotate_point(b, center, turns),
                width,
            },
            Shape::Circle {
                center: point,
                radius,
                filled,
            } => Shape::Circle {
                center: rotate_point(point, center, turns),
                radius,
                filled,
            },
            Shape::Poly {
                points,
                closed,
                width,
            } => Shape::Poly {
                points: points
                    .into_iter()
                    .map(|point| rotate_point(point, center, turns))
                    .collect(),
                closed,
                width,
            },
            Shape::Text { pos, text, size } => Shape::Text {
                pos: rotate_point(pos, center, turns),
                text,
                size,
            },
        })
        .collect()
}

/// Build the shapes of a component symbol plus its label and value.
///
/// `rotation` is the number of clockwise quarter turns around `(x, y)`.
#[must_use]
pub fn symbol_shapes(
    kind: ComponentKind,
    x: f64,
    y: f64,
    rotation: u8,
    label: &str,
    value: &str,
) -> Vec<Shape> {
    let mut shapes = Vec::new();
    match kind {
        ComponentKind::Resistor => {
            shapes.push(line((x - 40.0, y), (x - 24.0, y)));
            let mut points = vec![(x - 24.0, y), (x - 18.0, y - 9.0)];
            for (index, offset) in [-10.0, -2.0, 6.0, 14.0].iter().enumerate() {
                let amplitude = if index % 2 == 0 { 9.0 } else { -9.0 };
                points.push((x + offset, y + amplitude));
            }
            points.extend([(x + 22.0, y - 9.0), (x + 24.0, y), (x + 40.0, y)]);
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
        ComponentKind::Transistor => {
            shapes.push(Shape::Circle {
                center: (x + 2.0, y),
                radius: 34.0,
                filled: false,
            });
            shapes.push(line((x - 40.0, y), (x - 12.0, y)));
            shapes.push(Shape::Line {
                a: (x - 12.0, y - 22.0),
                b: (x - 12.0, y + 22.0),
                width: 3.0,
            });
            shapes.push(line((x - 12.0, y - 8.0), (x + 20.0, y - 40.0)));
            shapes.push(line((x - 12.0, y + 8.0), (x + 20.0, y + 40.0)));
            shapes.push(poly(
                vec![
                    (x + 8.0, y + 31.0),
                    (x + 21.0, y + 40.0),
                    (x + 16.0, y + 26.0),
                ],
                true,
            ));
        }
        ComponentKind::OpAmp => {
            shapes.push(poly(
                vec![(x - 30.0, y - 32.0), (x - 30.0, y + 32.0), (x + 34.0, y)],
                true,
            ));
            shapes.push(line((x - 40.0, y - 20.0), (x - 30.0, y - 20.0)));
            shapes.push(line((x - 40.0, y + 20.0), (x - 30.0, y + 20.0)));
            shapes.push(line((x + 34.0, y), (x + 40.0, y)));
            shapes.push(Shape::Text {
                pos: (x - 14.0, y - 16.0),
                text: "-".into(),
                size: 12.0,
            });
            shapes.push(Shape::Text {
                pos: (x - 14.0, y + 16.0),
                text: "+".into(),
                size: 12.0,
            });
        }
        ComponentKind::Connector => {
            shapes.push(poly(
                vec![
                    (x - 24.0, y - 18.0),
                    (x + 24.0, y - 18.0),
                    (x + 24.0, y + 18.0),
                    (x - 24.0, y + 18.0),
                ],
                true,
            ));
            shapes.push(line((x - 40.0, y), (x - 24.0, y)));
            shapes.push(line((x + 24.0, y), (x + 40.0, y)));
            shapes.push(line((x, y - 18.0), (x, y + 18.0)));
        }
    }
    shapes.extend(pin_markers(kind, x, y));
    let (label_y, value_y) = match kind {
        ComponentKind::Ground | ComponentKind::VoltageSource => (y - 52.0, y + 34.0),
        ComponentKind::Transistor => (y - 70.0, y + 62.0),
        ComponentKind::OpAmp => (y - 48.0, y + 48.0),
        _ => (y - 30.0, y + 34.0),
    };
    shapes.push(Shape::Text {
        pos: (x, label_y),
        text: label.into(),
        size: 12.0,
    });
    if !value.is_empty() {
        shapes.push(Shape::Text {
            pos: (x, value_y),
            text: value.into(),
            size: 10.0,
        });
    }
    rotate_shapes(shapes, x, y, rotation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_produces_shapes() {
        for kind in ComponentKind::ALL {
            let shapes = symbol_shapes(kind, 0.0, 0.0, 0, "X", "V");
            assert!(!shapes.is_empty(), "no shapes for {}", kind.name());
            assert!(
                shapes
                    .iter()
                    .any(|shape| matches!(shape, Shape::Text { .. }))
            );
        }
    }

    #[test]
    fn rotation_moves_geometry_but_keeps_text_upright() {
        let shapes = symbol_shapes(ComponentKind::Resistor, 0.0, 0.0, 1, "R1", "10R");
        let texts: Vec<&Shape> = shapes
            .iter()
            .filter(|shape| matches!(shape, Shape::Text { .. }))
            .collect();
        assert_eq!(texts.len(), 2);
        let Shape::Text { pos, text, .. } = texts[0] else {
            unreachable!()
        };
        assert_eq!(text, "R1");
        // The label sits above the horizontal symbol; a quarter turn moves it right.
        assert!(pos.0 > 0.0, "rotated label must move right: {pos:?}");
        assert!(
            shapes.iter().any(|shape| matches!(
                shape,
                Shape::Line { a: (0.0, y1), b: (0.0, y2), .. } if y1 != y2
            )),
            "a vertical lead should exist after rotation"
        );
    }
}
