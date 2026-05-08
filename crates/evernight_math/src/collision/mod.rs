pub mod detection;
pub mod shapes;

// Public API
pub use detection::{CollisionResult, detect};
pub use shapes::{Capsule, Circle, Ellipse, Line, Polygon, Ray, Rectangle, Shape2D, Triangle};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Angle, Vec2};

    #[test]
    fn test_circle_circle_collision() {
        let c1 = shapes::Circle {
            center: Vec2::new(0.0, 0.0),
            radius: 1.0,
        };
        let c2 = shapes::Circle {
            center: Vec2::new(1.5, 0.0),
            radius: 1.0,
        };
        let shape_a = shapes::Shape2D::Circle(c1);
        let shape_b = shapes::Shape2D::Circle(c2);

        let result = detection::detect(&shape_a, &shape_b);
        assert!(result.is_colliding);
        assert!(result.depth > 0.4 && result.depth < 0.6);
    }

    #[test]
    fn test_circle_circle_no_collision() {
        let c1 = shapes::Circle {
            center: Vec2::new(0.0, 0.0),
            radius: 1.0,
        };
        let c2 = shapes::Circle {
            center: Vec2::new(3.0, 0.0),
            radius: 1.0,
        };
        let shape_a = shapes::Shape2D::Circle(c1);
        let shape_b = shapes::Shape2D::Circle(c2);

        let result = detection::detect(&shape_a, &shape_b);
        assert!(!result.is_colliding);
    }

    #[test]
    fn test_circle_aabb_collision() {
        let c = shapes::Circle {
            center: Vec2::new(1.0, 0.0),
            radius: 1.0,
        };
        let r = shapes::Rectangle {
            position: Vec2::new(0.0, 0.0),
            size: Vec2::new(2.0, 2.0),
            rotation: Angle(0.0),
        };
        let shape_a = shapes::Shape2D::Circle(c);
        let shape_b = shapes::Shape2D::Rectangle(r);

        let result = detection::detect(&shape_a, &shape_b);
        assert!(result.is_colliding);
    }

    #[test]
    fn test_capsule_capsule_collision() {
        // Two capsules that clearly overlap (distance < radii sum)
        let cp1 = shapes::Capsule {
            start: Vec2::new(0.0, 0.0),
            end: Vec2::new(1.0, 0.0),
            radius: 0.6,
        };
        let cp2 = shapes::Capsule {
            start: Vec2::new(1.2, 0.0),
            end: Vec2::new(2.5, 0.0),
            radius: 0.6,
        };
        let shape_a = shapes::Shape2D::Capsule(cp1);
        let shape_b = shapes::Shape2D::Capsule(cp2);

        let result = detection::detect(&shape_a, &shape_b);
        assert!(
            result.is_colliding,
            "Capsules should collide: result={:?}",
            result
        );
    }

    #[test]
    fn test_rectangle_rectangle_rotated_collision() {
        // Two rectangles that collide when one is rotated 45 degrees
        let r1 = shapes::Rectangle {
            position: Vec2::new(0.0, 0.0),
            size: Vec2::new(2.0, 2.0),
            rotation: Angle(0.0),
        };
        let r2 = shapes::Rectangle {
            position: Vec2::new(1.5, 0.0),
            size: Vec2::new(2.0, 2.0),
            rotation: Angle(std::f32::consts::PI / 4.0), // 45 degrees
        };
        let shape_a = shapes::Shape2D::Rectangle(r1);
        let shape_b = shapes::Shape2D::Rectangle(r2);

        let result = detection::detect(&shape_a, &shape_b);
        assert!(
            result.is_colliding,
            "Rotated rectangles should collide: result={:?}",
            result
        );
        assert!(result.depth > 0.0, "Collision depth should be positive");
    }

    #[test]
    fn test_circle_rotated_rectangle_collision() {
        let c = shapes::Circle {
            center: Vec2::new(1.5, 0.0),
            radius: 0.8,
        };
        let r = shapes::Rectangle {
            position: Vec2::new(0.0, 0.0),
            size: Vec2::new(2.0, 2.0),
            rotation: Angle(std::f32::consts::PI / 6.0),
        };
        let result = detection::detect(&shapes::Shape2D::Circle(c), &shapes::Shape2D::Rectangle(r));
        assert!(
            result.is_colliding,
            "Circle should collide with rotated rectangle: {:?}",
            result
        );
    }

    #[test]
    fn test_circle_triangle_collision() {
        let c = shapes::Circle {
            center: Vec2::new(0.5, 0.3),
            radius: 0.5,
        };
        let t = shapes::Triangle {
            a: Vec2::new(0.0, 0.0),
            b: Vec2::new(2.0, 0.0),
            c: Vec2::new(1.0, 2.0),
        };
        let result = detection::detect(&shapes::Shape2D::Circle(c), &shapes::Shape2D::Triangle(t));
        assert!(
            result.is_colliding,
            "Circle should collide with triangle: {:?}",
            result
        );
    }

    #[test]
    fn test_circle_polygon_no_collision() {
        let c = shapes::Circle {
            center: Vec2::new(5.0, 5.0),
            radius: 0.5,
        };
        let p = shapes::Polygon {
            vertices: vec![
                Vec2::new(0.0, 0.0),
                Vec2::new(1.0, 0.0),
                Vec2::new(1.0, 1.0),
                Vec2::new(0.0, 1.0),
            ],
        };
        let result = detection::detect(&shapes::Shape2D::Circle(c), &shapes::Shape2D::Polygon(p));
        assert!(
            !result.is_colliding,
            "Circle should not collide with distant polygon: {:?}",
            result
        );
    }

    #[test]
    fn test_triangle_triangle_collision() {
        let t1 = shapes::Triangle {
            a: Vec2::new(0.0, 0.0),
            b: Vec2::new(2.0, 0.0),
            c: Vec2::new(1.0, 2.0),
        };
        let t2 = shapes::Triangle {
            a: Vec2::new(1.0, 0.5),
            b: Vec2::new(3.0, 0.5),
            c: Vec2::new(2.0, 2.5),
        };
        let result = detection::detect(
            &shapes::Shape2D::Triangle(t1),
            &shapes::Shape2D::Triangle(t2),
        );
        assert!(
            result.is_colliding,
            "Overlapping triangles should collide: {:?}",
            result
        );
    }

    #[test]
    fn test_line_line_intersection() {
        let l1 = shapes::Line {
            start: Vec2::new(0.0, 0.0),
            end: Vec2::new(2.0, 2.0),
        };
        let l2 = shapes::Line {
            start: Vec2::new(0.0, 2.0),
            end: Vec2::new(2.0, 0.0),
        };
        let result = detection::detect(&shapes::Shape2D::Line(l1), &shapes::Shape2D::Line(l2));
        assert!(
            result.is_colliding,
            "Crossing lines should intersect: {:?}",
            result
        );
    }

    #[test]
    fn test_line_line_no_intersection() {
        let l1 = shapes::Line {
            start: Vec2::new(0.0, 0.0),
            end: Vec2::new(1.0, 0.0),
        };
        let l2 = shapes::Line {
            start: Vec2::new(0.0, 2.0),
            end: Vec2::new(1.0, 2.0),
        };
        let result = detection::detect(&shapes::Shape2D::Line(l1), &shapes::Shape2D::Line(l2));
        assert!(
            !result.is_colliding,
            "Parallel lines should not intersect: {:?}",
            result
        );
    }

    #[test]
    fn test_ray_polygon_hit() {
        let r = shapes::Ray {
            origin: Vec2::new(-2.0, 0.5),
            direction: Vec2::new(1.0, 0.0),
        };
        let p = shapes::Polygon {
            vertices: vec![
                Vec2::new(0.0, 0.0),
                Vec2::new(2.0, 0.0),
                Vec2::new(2.0, 2.0),
                Vec2::new(0.0, 2.0),
            ],
        };
        let result = detection::detect(&shapes::Shape2D::Ray(r), &shapes::Shape2D::Polygon(p));
        assert!(result.is_colliding, "Ray should hit polygon: {:?}", result);
    }

    #[test]
    fn test_capsule_polygon_collision() {
        let cp = shapes::Capsule {
            start: Vec2::new(-1.0, 0.5),
            end: Vec2::new(1.0, 0.5),
            radius: 0.3,
        };
        let p = shapes::Polygon {
            vertices: vec![
                Vec2::new(0.0, 0.0),
                Vec2::new(2.0, 0.0),
                Vec2::new(2.0, 2.0),
                Vec2::new(0.0, 2.0),
            ],
        };
        let result = detection::detect(&shapes::Shape2D::Capsule(cp), &shapes::Shape2D::Polygon(p));
        assert!(
            result.is_colliding,
            "Capsule should collide with polygon: {:?}",
            result
        );
    }
}
