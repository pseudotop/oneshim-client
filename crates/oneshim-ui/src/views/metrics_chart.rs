use iced::widget::canvas::{self, Cache, Geometry, Path, Stroke};
use iced::{mouse, Color, Element, Length, Point, Rectangle, Renderer, Size, Theme};

#[derive(Debug, Clone, Copy)]
pub struct ChartColors {
    pub line: Color,
    pub fill: Color,
    pub grid: Color,
}

impl ChartColors {
    pub fn cpu() -> Self {
        Self {
            line: Color::from_rgb(0.2, 0.6, 1.0),
            fill: Color::from_rgba(0.2, 0.6, 1.0, 0.3),
            grid: Color::from_rgba(1.0, 1.0, 1.0, 0.15),
        }
    }

    pub fn memory() -> Self {
        Self {
            line: Color::from_rgb(0.2, 0.8, 0.4),
            fill: Color::from_rgba(0.2, 0.8, 0.4, 0.3),
            grid: Color::from_rgba(1.0, 1.0, 1.0, 0.15),
        }
    }
}

pub struct MetricsChartCanvas {
    data: Vec<f64>,
    colors: ChartColors,
    min_value: Option<f64>,
    max_value: Option<f64>,
    cache: Cache,
}

impl MetricsChartCanvas {
    pub fn new(data: Vec<f64>, colors: ChartColors) -> Self {
        Self {
            data,
            colors,
            min_value: None,
            max_value: None,
            cache: Cache::default(),
        }
    }

    pub fn with_fixed_scale(mut self, min: f64, max: f64) -> Self {
        self.min_value = Some(min);
        self.max_value = Some(max);
        self
    }

    fn data_range(&self) -> (f64, f64) {
        if self.data.is_empty() {
            return (0.0, 100.0);
        }

        if let (Some(min), Some(max)) = (self.min_value, self.max_value) {
            return (min, max);
        }

        let min = self
            .min_value
            .unwrap_or_else(|| self.data.iter().cloned().fold(f64::INFINITY, f64::min));
        let max = self
            .max_value
            .unwrap_or_else(|| self.data.iter().cloned().fold(f64::NEG_INFINITY, f64::max));

        if (max - min).abs() < 0.001 {
            (min.max(0.0) - 1.0, max + 1.0)
        } else {
            let padding = (max - min) * 0.1;
            ((min - padding).max(0.0), max + padding)
        }
    }
}

impl<Message> canvas::Program<Message> for MetricsChartCanvas {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let geometry = self.cache.draw(renderer, bounds.size(), |frame| {
            let width = frame.width();
            let height = frame.height();

            frame.fill_rectangle(
                Point::ORIGIN,
                Size::new(width, height),
                Color::from_rgba(0.0, 0.0, 0.0, 0.2),
            );

            for i in 1..4 {
                let y = height * (i as f32) / 4.0;
                let grid_path = Path::line(Point::new(0.0, y), Point::new(width, y));
                frame.stroke(
                    &grid_path,
                    Stroke::default()
                        .with_color(self.colors.grid)
                        .with_width(1.0),
                );
            }

            if self.data.is_empty() {
                return;
            }

            let (min_val, max_val) = self.data_range();
            let range = max_val - min_val;

            let points: Vec<Point> = self
                .data
                .iter()
                .enumerate()
                .map(|(i, &value)| {
                    let x = if self.data.len() > 1 {
                        (i as f32) / (self.data.len() - 1) as f32 * width
                    } else {
                        width / 2.0
                    };
                    let normalized = if range > 0.0 {
                        ((value - min_val) / range) as f32
                    } else {
                        0.5
                    };
                    let y = height - (normalized.clamp(0.0, 1.0) * height);
                    Point::new(x, y)
                })
                .collect();

            if points.len() >= 2 {
                let mut fill_builder = canvas::path::Builder::new();
                fill_builder.move_to(Point::new(points[0].x, height));

                for point in &points {
                    fill_builder.line_to(*point);
                }

                fill_builder.line_to(Point::new(points.last().unwrap().x, height));
                fill_builder.close();

                let fill_path = fill_builder.build();
                frame.fill(&fill_path, self.colors.fill);
            }

            if points.len() >= 2 {
                let mut line_builder = canvas::path::Builder::new();
                line_builder.move_to(points[0]);

                for point in points.iter().skip(1) {
                    line_builder.line_to(*point);
                }

                let line_path = line_builder.build();
                frame.stroke(
                    &line_path,
                    Stroke::default()
                        .with_color(self.colors.line)
                        .with_width(2.0),
                );
            }
        });

        vec![geometry]
    }
}

pub fn cpu_chart<'a, Message: 'a>(data: Vec<f32>) -> Element<'a, Message> {
    let data_f64: Vec<f64> = data.into_iter().map(|v| v as f64).collect();
    let chart = MetricsChartCanvas::new(data_f64, ChartColors::cpu()).with_fixed_scale(0.0, 100.0);
    canvas::Canvas::new(chart)
        .width(Length::Fill)
        .height(Length::Fixed(60.0))
        .into()
}

pub fn memory_chart<'a, Message: 'a>(data: Vec<f64>) -> Element<'a, Message> {
    let chart = MetricsChartCanvas::new(data, ChartColors::memory());
    canvas::Canvas::new(chart)
        .width(Length::Fill)
        .height(Length::Fixed(60.0))
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chart_colors() {
        let cpu = ChartColors::cpu();
        assert!(cpu.line.b > cpu.line.r); // check
        let mem = ChartColors::memory();
        assert!(mem.line.g > mem.line.r); // green tint check
    }

    #[test]
    fn chart_creation() {
        let data = vec![10.0, 20.0, 30.0, 40.0, 50.0];
        let chart = MetricsChartCanvas::new(data.clone(), ChartColors::cpu());
        assert_eq!(chart.data.len(), 5);
    }

    #[test]
    fn data_range_auto() {
        let data = vec![10.0, 20.0, 30.0, 40.0, 50.0];
        let chart = MetricsChartCanvas::new(data, ChartColors::memory());
        let (min, max) = chart.data_range();
        assert!(min < 10.0); // includes lower padding
        assert!(max > 50.0); // includes upper padding
    }

    #[test]
    fn data_range_fixed() {
        let data = vec![10.0, 20.0, 30.0];
        let chart = MetricsChartCanvas::new(data, ChartColors::cpu()).with_fixed_scale(0.0, 100.0);
        let (min, max) = chart.data_range();
        assert_eq!(min, 0.0);
        assert_eq!(max, 100.0);
    }
}
