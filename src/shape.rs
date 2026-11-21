use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke};
use iced::{Color, Point, Rectangle, Renderer, Size, Theme, mouse};

/// Flat angular shapes of the HUD style, drawn behind or over widgets.
#[derive(Debug, Clone, Copy)]
pub enum Kind {
    /// Rectangle whose right edge is cut diagonally by `slant` pixels.
    Slant(f32),
    /// L-shaped marks on the top-left and bottom-right corners: arm length and line width.
    Brackets { arm: f32, width: f32 },
    /// Line of `thickness` along the bottom edge.
    Underline(f32),
}

impl Kind {
    /// The small corner marks of icon slots.
    pub const BRACKETS: Kind = Kind::Brackets { arm: 7.0, width: 1.0 };
}

pub struct Shape {
    pub kind: Kind,
    pub color: Color,
}

impl<Message> canvas::Program<Message> for Shape {
    type State = ();

    fn draw(&self, _: &(), renderer: &Renderer, _: &Theme, bounds: Rectangle, _: mouse::Cursor) -> Vec<Geometry> {
        let (w, h) = (bounds.width, bounds.height);
        let mut frame = Frame::new(renderer, bounds.size());
        match self.kind {
            Kind::Slant(slant) => {
                let points = [Point::ORIGIN, Point::new(w, 0.0), Point::new(w - slant, h), Point::new(0.0, h)];
                frame.fill(&polygon(&points), self.color);
            }
            Kind::Brackets { arm, width } => {
                // Keep the stroke inside the bounds.
                let inset = width / 2.0;
                let marks = Path::new(|builder| {
                    builder.move_to(Point::new(inset, arm));
                    builder.line_to(Point::new(inset, inset));
                    builder.line_to(Point::new(arm, inset));
                    builder.move_to(Point::new(w - arm, h - inset));
                    builder.line_to(Point::new(w - inset, h - inset));
                    builder.line_to(Point::new(w - inset, h - arm));
                });
                frame.stroke(&marks, Stroke::default().with_color(self.color).with_width(width));
            }
            Kind::Underline(thickness) => {
                frame.fill_rectangle(Point::new(0.0, h - thickness), Size::new(w, thickness), self.color);
            }
        }
        vec![frame.into_geometry()]
    }
}

fn polygon(points: &[Point]) -> Path {
    Path::new(|builder| {
        builder.move_to(points[0]);
        for point in &points[1..] {
            builder.line_to(*point);
        }
        builder.close();
    })
}
