use iced::widget::canvas::{self, Frame, Geometry, Path};
use iced::{Color, Point, Rectangle, Renderer, Theme, mouse};

pub const RADIUS: u32 = 23;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

/// Concave corner hanging below the bar, so the desktop looks like a rounded screen.
pub struct Corner {
    pub side: Side,
    pub color: Color,
}

impl<Message> canvas::Program<Message> for Corner {
    type State = ();

    fn draw(&self, _: &(), renderer: &Renderer, _: &Theme, bounds: Rectangle, _: mouse::Cursor) -> Vec<Geometry> {
        let r = bounds.width;
        // Fill between the outer corner and a quarter circle bulging towards it.
        let (corner, edge, end) = match self.side {
            Side::Left => (Point::ORIGIN, Point::new(r, 0.0), Point::new(0.0, r)),
            Side::Right => (Point::new(r, 0.0), Point::ORIGIN, Point::new(r, r)),
        };
        let path = Path::new(|builder| {
            builder.move_to(corner);
            builder.line_to(edge);
            builder.arc_to(corner, end, r);
            builder.close();
        });

        let mut frame = Frame::new(renderer, bounds.size());
        frame.fill(&path, self.color);
        vec![frame.into_geometry()]
    }
}
