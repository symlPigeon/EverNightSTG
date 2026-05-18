use super::shapes::Shape2D;
use crate::Vec2;

/// Collision detection result
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CollisionResult {
    pub is_colliding: bool,
    pub contact_point: Option<Vec2>,
    pub normal: Option<Vec2>,
    pub depth: f32,
}

impl CollisionResult {
    pub fn none() -> Self {
        CollisionResult {
            is_colliding: false,
            contact_point: None,
            normal: None,
            depth: 0.0,
        }
    }

    pub fn collision(contact_point: Vec2, normal: Vec2, depth: f32) -> Self {
        CollisionResult {
            is_colliding: true,
            contact_point: Some(contact_point),
            normal: Some(normal),
            depth,
        }
    }
}

/// Main collision detection dispatcher
pub fn detect(shape_a: &Shape2D, shape_b: &Shape2D) -> CollisionResult {
    use Shape2D::*;
    match (shape_a, shape_b) {
        // Circle combinations
        (Circle(a), Circle(b)) => circle_circle(a, b),
        (Circle(c), Rectangle(r)) | (Rectangle(r), Circle(c)) => circle_rectangle(c, r),
        (Circle(c), Capsule(cp)) | (Capsule(cp), Circle(c)) => circle_capsule_collision(c, cp),
        (Circle(c), Triangle(t)) | (Triangle(t), Circle(c)) => {
            circle_polygon(c, &triangle_vertices(t))
        }
        (Circle(c), Polygon(p)) | (Polygon(p), Circle(c)) => circle_polygon(c, &p.vertices),
        (Circle(c), Ellipse(e)) | (Ellipse(e), Circle(c)) => circle_ellipse(c, e),
        (Circle(c), Line(l)) | (Line(l), Circle(c)) => circle_line(c, l),
        (Circle(c), Ray(r)) | (Ray(r), Circle(c)) => circle_ray(c, r),

        // Rectangle combinations
        (Rectangle(a), Rectangle(b)) => rectangle_rectangle(a, b),
        (Rectangle(r), Capsule(cp)) | (Capsule(cp), Rectangle(r)) => {
            capsule_polygon(cp, &rectangle_vertices(r))
        }
        (Rectangle(r), Triangle(t)) | (Triangle(t), Rectangle(r)) => {
            polygon_polygon(&rectangle_vertices(r), &triangle_vertices(t))
        }
        (Rectangle(r), Polygon(p)) | (Polygon(p), Rectangle(r)) => {
            polygon_polygon(&rectangle_vertices(r), &p.vertices)
        }
        (Rectangle(r), Ellipse(e)) | (Ellipse(e), Rectangle(r)) => {
            ellipse_polygon(e, &rectangle_vertices(r))
        }
        (Rectangle(r), Line(l)) | (Line(l), Rectangle(r)) => {
            line_polygon(l, &rectangle_vertices(r))
        }
        (Rectangle(r), Ray(ray)) | (Ray(ray), Rectangle(r)) => {
            ray_polygon(ray, &rectangle_vertices(r))
        }

        // Capsule combinations
        (Capsule(a), Capsule(b)) => capsule_capsule(a, b),
        (Capsule(cp), Triangle(t)) | (Triangle(t), Capsule(cp)) => {
            capsule_polygon(cp, &triangle_vertices(t))
        }
        (Capsule(cp), Polygon(p)) | (Polygon(p), Capsule(cp)) => capsule_polygon(cp, &p.vertices),
        (Capsule(cp), Ellipse(e)) | (Ellipse(e), Capsule(cp)) => {
            circle_ellipse(&capsule_bounding_circle(cp), e)
        }
        (Capsule(cp), Line(l)) | (Line(l), Capsule(cp)) => capsule_line(cp, l),
        (Capsule(cp), Ray(r)) | (Ray(r), Capsule(cp)) => capsule_ray(cp, r),

        // Polygon (Triangle/Polygon) combinations
        (Triangle(a), Triangle(b)) => polygon_polygon(&triangle_vertices(a), &triangle_vertices(b)),
        (Triangle(t), Polygon(p)) | (Polygon(p), Triangle(t)) => {
            polygon_polygon(&triangle_vertices(t), &p.vertices)
        }
        (Polygon(a), Polygon(b)) => polygon_polygon(&a.vertices, &b.vertices),
        (Triangle(t), Ellipse(e)) | (Ellipse(e), Triangle(t)) => {
            ellipse_polygon(e, &triangle_vertices(t))
        }
        (Polygon(p), Ellipse(e)) | (Ellipse(e), Polygon(p)) => ellipse_polygon(e, &p.vertices),
        (Triangle(t), Line(l)) | (Line(l), Triangle(t)) => line_polygon(l, &triangle_vertices(t)),
        (Polygon(p), Line(l)) | (Line(l), Polygon(p)) => line_polygon(l, &p.vertices),
        (Triangle(t), Ray(r)) | (Ray(r), Triangle(t)) => ray_polygon(r, &triangle_vertices(t)),
        (Polygon(p), Ray(r)) | (Ray(r), Polygon(p)) => ray_polygon(r, &p.vertices),

        // Ellipse combinations
        (Ellipse(a), Ellipse(b)) => ellipse_ellipse(a, b),
        (Ellipse(e), Line(l)) | (Line(l), Ellipse(e)) => {
            circle_line(&ellipse_bounding_circle(e), l)
        }
        (Ellipse(e), Ray(r)) | (Ray(r), Ellipse(e)) => circle_ray(&ellipse_bounding_circle(e), r),

        // Line/Ray combinations
        (Line(a), Line(b)) => line_line(a, b),
        (Line(l), Ray(r)) | (Ray(r), Line(l)) => ray_line(r, l),
        (Ray(a), Ray(b)) => ray_ray(a, b),
    }
}

/// Circle-Circle collision detection
fn circle_circle(a: &super::shapes::Circle, b: &super::shapes::Circle) -> CollisionResult {
    let delta = b.center - a.center;
    let distance = delta.length();
    let min_distance = a.radius + b.radius;

    if distance >= min_distance {
        return CollisionResult::none();
    }

    let depth = min_distance - distance;
    let normal = if distance > 0.001 {
        delta.normalized()
    } else {
        Vec2::new(1.0, 0.0)
    };
    let contact_point = a.center + normal * a.radius;

    CollisionResult::collision(contact_point, normal, depth)
}

/// Rectangle-Rectangle collision (OBB, supports rotation)
fn rectangle_rectangle(
    a: &super::shapes::Rectangle,
    b: &super::shapes::Rectangle,
) -> CollisionResult {
    // Get corners of both rectangles
    let corners_a = get_rectangle_corners(a);
    let corners_b = get_rectangle_corners(b);

    // Get edges as potential separating axes
    let axes = get_separating_axes(&corners_a, &corners_b);

    // Check each axis
    let mut min_overlap = f32::MAX;
    let mut min_axis = Vec2::new(1.0, 0.0);

    for axis in &axes {
        let proj_a = project_polygon(axis, &corners_a);
        let proj_b = project_polygon(axis, &corners_b);

        let overlap = overlap_amount(proj_a, proj_b);
        if overlap < 0.0 {
            return CollisionResult::none(); // Found separating axis
        }

        if overlap < min_overlap {
            min_overlap = overlap;
            min_axis = *axis;
        }
    }

    // No separating axis found, shapes collide
    // Calculate contact point as centroid between shape centers
    let contact_point = (a.position + b.position) * 0.5;

    // Ensure normal points from a to b
    let delta = b.position - a.position;
    let normal = if delta.dot(min_axis) < 0.0 {
        -min_axis
    } else {
        min_axis
    };

    CollisionResult::collision(contact_point, normal.normalized(), min_overlap)
}

/// Circle-Rectangle collision detection (supports rotation)
fn circle_rectangle(
    circle: &super::shapes::Circle,
    rect: &super::shapes::Rectangle,
) -> CollisionResult {
    let corners = get_rectangle_corners(rect);

    // Find closest point on rectangle edges to circle center
    let mut closest_point = corners[0];
    let mut min_dist = circle.center.distance_to(corners[0]);

    // Check all corners
    for corner in corners.iter().skip(1) {
        let dist = circle.center.distance_to(*corner);
        if dist < min_dist {
            min_dist = dist;
            closest_point = *corner;
        }
    }

    // Check all edges
    for i in 0..4 {
        let edge_start = corners[i];
        let edge_end = corners[(i + 1) % 4];
        let (closest_on_edge, _) = closest_point_on_segment(edge_start, edge_end, circle.center);
        let dist = circle.center.distance_to(closest_on_edge);
        if dist < min_dist {
            min_dist = dist;
            closest_point = closest_on_edge;
        }
    }

    // Check collision
    if min_dist >= circle.radius {
        return CollisionResult::none();
    }

    let delta = circle.center - closest_point;
    let depth = circle.radius - min_dist;
    let normal = if min_dist > 0.001 {
        delta.normalized()
    } else {
        Vec2::new(1.0, 0.0)
    };

    CollisionResult::collision(closest_point, normal, depth)
}

/// Circle-Capsule collision detection
fn circle_capsule_collision(
    circle: &super::shapes::Circle,
    capsule: &super::shapes::Capsule,
) -> CollisionResult {
    let (closest_on_capsule, _) =
        closest_point_on_segment(capsule.start, capsule.end, circle.center);
    let delta = circle.center - closest_on_capsule;
    let distance = delta.length();
    let min_distance = circle.radius + capsule.radius;

    if distance >= min_distance {
        return CollisionResult::none();
    }

    let depth = min_distance - distance;
    let normal = if distance > 0.001 {
        delta.normalized()
    } else {
        Vec2::new(1.0, 0.0)
    };
    let contact_point = closest_on_capsule + normal * capsule.radius;

    CollisionResult::collision(contact_point, normal, depth)
}

/// Capsule-Capsule collision detection
fn capsule_capsule(a: &super::shapes::Capsule, b: &super::shapes::Capsule) -> CollisionResult {
    let (closest_a, closest_b) = closest_point_on_segments(a.start, a.end, b.start, b.end);
    let delta = closest_b - closest_a;
    let distance = delta.length();
    let min_distance = a.radius + b.radius;

    if distance >= min_distance {
        return CollisionResult::none();
    }

    let depth = min_distance - distance;
    let normal = if distance > 0.001 {
        delta.normalized()
    } else {
        Vec2::new(1.0, 0.0)
    };
    let contact_point = closest_a + normal * a.radius;

    CollisionResult::collision(contact_point, normal, depth)
}

// ===== Helper functions =====

/// Get the four corners of a rectangle accounting for rotation
fn get_rectangle_corners(rect: &super::shapes::Rectangle) -> [Vec2; 4] {
    let half_width = rect.size.x * 0.5;
    let half_height = rect.size.y * 0.5;

    // Local corners before rotation
    let corners_local = [
        Vec2::new(-half_width, -half_height),
        Vec2::new(half_width, -half_height),
        Vec2::new(half_width, half_height),
        Vec2::new(-half_width, half_height),
    ];

    // Apply rotation and translation
    let cos_a = rect.rotation.cos();
    let sin_a = rect.rotation.sin();

    [
        rotate_point(corners_local[0], cos_a, sin_a) + rect.position,
        rotate_point(corners_local[1], cos_a, sin_a) + rect.position,
        rotate_point(corners_local[2], cos_a, sin_a) + rect.position,
        rotate_point(corners_local[3], cos_a, sin_a) + rect.position,
    ]
}

/// Rotate a point around origin by angle (cos, sin)
fn rotate_point(p: Vec2, cos_a: f32, sin_a: f32) -> Vec2 {
    Vec2::new(p.x * cos_a - p.y * sin_a, p.x * sin_a + p.y * cos_a)
}

/// Get separating axis candidates from two convex polygons
fn get_separating_axes(poly_a: &[Vec2; 4], poly_b: &[Vec2; 4]) -> [Vec2; 4] {
    let mut axes = [Vec2::new(1.0, 0.0); 4];

    // Get normals from edges of polygon A
    for i in 0..2 {
        let edge = poly_a[(i + 1) % 4] - poly_a[i];
        let normal = Vec2::new(-edge.y, edge.x).normalized();
        axes[i] = normal;
    }

    // Get normals from edges of polygon B
    for i in 0..2 {
        let edge = poly_b[(i + 1) % 4] - poly_b[i];
        let normal = Vec2::new(-edge.y, edge.x).normalized();
        axes[2 + i] = normal;
    }

    axes
}

/// Project a convex polygon onto an axis, return (min, max)
fn project_polygon(axis: &Vec2, polygon: &[Vec2; 4]) -> (f32, f32) {
    let mut min_proj = polygon[0].dot(*axis);
    let mut max_proj = min_proj;

    for vertex in polygon.iter().skip(1) {
        let proj = vertex.dot(*axis);
        if proj < min_proj {
            min_proj = proj;
        }
        if proj > max_proj {
            max_proj = proj;
        }
    }

    (min_proj, max_proj)
}

/// Calculate overlap amount between two 1D projections
/// Returns negative if separated, otherwise returns overlap distance
fn overlap_amount(proj_a: (f32, f32), proj_b: (f32, f32)) -> f32 {
    let overlap_left = proj_a.1 - proj_b.0;
    let overlap_right = proj_b.1 - proj_a.0;

    if overlap_left < 0.0 || overlap_right < 0.0 {
        return -1.0; // Separated
    }

    overlap_left.min(overlap_right)
}

/// Find closest point on a line segment to a point
fn closest_point_on_segment(p1: Vec2, p2: Vec2, point: Vec2) -> (Vec2, f32) {
    let d = p2 - p1;
    let len_sq = d.dot(d);
    if len_sq < 0.0001 {
        return (p1, 0.0);
    }
    let t = ((point - p1).dot(d) / len_sq).clamp(0.0, 1.0);
    let closest = p1 + d * t;
    (closest, t)
}

/// Find closest points between two line segments
fn closest_point_on_segments(p1: Vec2, p2: Vec2, p3: Vec2, p4: Vec2) -> (Vec2, Vec2) {
    let d1 = p2 - p1;
    let d2 = p4 - p3;
    let d3 = p3 - p1;

    let a = d1.dot(d1);
    let b = d1.dot(d2);
    let c = d2.dot(d2);
    let d = d1.dot(d3);
    let e = d2.dot(d3);

    let denom = a * c - b * b;
    let (s, t) = if denom.abs() < 0.0001 {
        // Segments are parallel; find best projection
        if a > 0.0001 {
            let t1 = (d / a).clamp(0.0, 1.0);
            (t1, 0.0)
        } else {
            (0.0, 0.0)
        }
    } else {
        let s_raw = (b * e - c * d) / denom;
        let t_raw = (a * e - b * d) / denom;
        let s = s_raw.clamp(0.0, 1.0);
        let t = t_raw.clamp(0.0, 1.0);
        (s, t)
    };

    let closest_a = p1 + d1 * s;
    let closest_b = p3 + d2 * t;

    (closest_a, closest_b)
}

// ===== Shape vertex converters =====

fn triangle_vertices(t: &super::shapes::Triangle) -> Vec<Vec2> {
    vec![t.a, t.b, t.c]
}

fn rectangle_vertices(r: &super::shapes::Rectangle) -> Vec<Vec2> {
    get_rectangle_corners(r).to_vec()
}

fn ellipse_bounding_circle(e: &super::shapes::Ellipse) -> super::shapes::Circle {
    super::shapes::Circle {
        center: e.center,
        radius: e.radii.0.max(e.radii.1),
    }
}

fn capsule_bounding_circle(cp: &super::shapes::Capsule) -> super::shapes::Circle {
    let mid = (cp.start + cp.end) * 0.5;
    let half_len = cp.start.distance_to(cp.end) * 0.5;
    super::shapes::Circle {
        center: mid,
        radius: half_len + cp.radius,
    }
}

// ===== General polygon utilities =====

/// Project a slice of vertices onto an axis, return (min, max)
fn project_vertices(axis: Vec2, vertices: &[Vec2]) -> (f32, f32) {
    let mut min_proj = vertices[0].dot(axis);
    let mut max_proj = min_proj;
    for v in vertices.iter().skip(1) {
        let proj = v.dot(axis);
        if proj < min_proj {
            min_proj = proj;
        }
        if proj > max_proj {
            max_proj = proj;
        }
    }
    (min_proj, max_proj)
}

/// Collect edge normals from a polygon as SAT axes
fn polygon_axes(vertices: &[Vec2]) -> Vec<Vec2> {
    let n = vertices.len();
    let mut axes = Vec::with_capacity(n);
    for i in 0..n {
        let edge = vertices[(i + 1) % n] - vertices[i];
        let normal = Vec2::new(-edge.y, edge.x);
        let len = normal.length();
        if len > 0.0001 {
            axes.push(normal * (1.0 / len));
        }
    }
    axes
}

/// SAT test between two convex polygons.
/// Returns CollisionResult with the minimum separating axis info.
fn sat_polygons(verts_a: &[Vec2], verts_b: &[Vec2]) -> CollisionResult {
    let mut axes = polygon_axes(verts_a);
    axes.extend(polygon_axes(verts_b));

    let mut min_overlap = f32::MAX;
    let mut min_axis = Vec2::new(1.0, 0.0);

    for axis in &axes {
        let proj_a = project_vertices(*axis, verts_a);
        let proj_b = project_vertices(*axis, verts_b);
        let overlap = overlap_amount(proj_a, proj_b);
        if overlap < 0.0 {
            return CollisionResult::none();
        }
        if overlap < min_overlap {
            min_overlap = overlap;
            min_axis = *axis;
        }
    }

    // Compute centroids
    let centroid_a =
        verts_a.iter().fold(Vec2::zero(), |acc, &v| acc + v) * (1.0 / verts_a.len() as f32);
    let centroid_b =
        verts_b.iter().fold(Vec2::zero(), |acc, &v| acc + v) * (1.0 / verts_b.len() as f32);
    let contact_point = (centroid_a + centroid_b) * 0.5;

    let delta = centroid_b - centroid_a;
    let normal = if delta.dot(min_axis) < 0.0 {
        -min_axis
    } else {
        min_axis
    };

    CollisionResult::collision(contact_point, normal.normalized(), min_overlap)
}

/// Find the closest point on a convex polygon's boundary to a given point
fn closest_point_on_polygon(point: Vec2, vertices: &[Vec2]) -> Vec2 {
    let n = vertices.len();
    let mut closest = vertices[0];
    let mut min_dist = point.distance_to(vertices[0]);
    for i in 0..n {
        let (c, _) = closest_point_on_segment(vertices[i], vertices[(i + 1) % n], point);
        let d = point.distance_to(c);
        if d < min_dist {
            min_dist = d;
            closest = c;
        }
    }
    closest
}

/// Test if a point is inside a convex polygon (winding / cross product method)
fn point_in_polygon(point: Vec2, vertices: &[Vec2]) -> bool {
    let n = vertices.len();
    for i in 0..n {
        let edge = vertices[(i + 1) % n] - vertices[i];
        let to_point = point - vertices[i];
        if edge.cross(to_point) < 0.0 {
            return false;
        }
    }
    true
}

// ===== New collision functions =====

/// Circle vs convex polygon (SAT)
fn circle_polygon(circle: &super::shapes::Circle, vertices: &[Vec2]) -> CollisionResult {
    // Find closest point on polygon to circle center
    let closest = closest_point_on_polygon(circle.center, vertices);
    let delta = circle.center - closest;
    let dist = delta.length();

    if dist >= circle.radius && !point_in_polygon(circle.center, vertices) {
        return CollisionResult::none();
    }

    if dist < 0.0001 {
        // Circle center is inside polygon; use polygon axis SAT
        let mut axes = polygon_axes(vertices);
        // Also add axis from polygon centroid to circle center
        let centroid =
            vertices.iter().fold(Vec2::zero(), |acc, &v| acc + v) * (1.0 / vertices.len() as f32);
        let cx_axis = (circle.center - centroid).normalized();
        axes.push(cx_axis);

        let circle_verts = [circle.center]; // single point projection
        let mut min_overlap = f32::MAX;
        let mut min_axis = Vec2::new(1.0, 0.0);
        for axis in &axes {
            let proj_a = project_vertices(*axis, vertices);
            let circle_proj = circle.center.dot(*axis);
            let proj_b = (circle_proj - circle.radius, circle_proj + circle.radius);
            let overlap = overlap_amount(proj_a, proj_b);
            if overlap < 0.0 {
                return CollisionResult::none();
            }
            if overlap < min_overlap {
                min_overlap = overlap;
                min_axis = *axis;
            }
        }
        let _ = circle_verts; // suppress warning
        let centroid =
            vertices.iter().fold(Vec2::zero(), |acc, &v| acc + v) * (1.0 / vertices.len() as f32);
        let delta2 = circle.center - centroid;
        let normal = if delta2.dot(min_axis) < 0.0 {
            -min_axis
        } else {
            min_axis
        };
        return CollisionResult::collision(closest, normal.normalized(), min_overlap);
    }

    let depth = circle.radius - dist;
    let normal = delta.normalized();
    CollisionResult::collision(closest, normal, depth)
}

/// Convex polygon vs convex polygon (SAT)
fn polygon_polygon(verts_a: &[Vec2], verts_b: &[Vec2]) -> CollisionResult {
    sat_polygons(verts_a, verts_b)
}

/// Circle vs Ellipse (approximate: scale space to circle)
fn circle_ellipse(
    circle: &super::shapes::Circle,
    ellipse: &super::shapes::Ellipse,
) -> CollisionResult {
    let (rx, ry) = ellipse.radii;
    if rx < 0.0001 || ry < 0.0001 {
        return CollisionResult::none();
    }
    // Transform circle center into ellipse's local space, then scale to unit circle
    let cos_a = ellipse.major_axis_angle.cos();
    let sin_a = ellipse.major_axis_angle.sin();
    let local = circle.center - ellipse.center;
    let lx = local.x * cos_a + local.y * sin_a;
    let ly = -local.x * sin_a + local.y * cos_a;

    // Scaled space: ellipse becomes unit circle, circle becomes ellipse-like blob
    // Approximate: use scaled distance to center
    let scaled_x = lx / rx;
    let scaled_y = ly / ry;
    let scaled_dist = (scaled_x * scaled_x + scaled_y * scaled_y).sqrt();
    // Effective radius of circle in scaled space (conservative: use max scale factor)
    let scale = rx.min(ry);
    let scaled_circle_radius = circle.radius / scale;

    if scaled_dist >= 1.0 + scaled_circle_radius {
        return CollisionResult::none();
    }

    // Compute real-space normal: gradient of ellipse equation
    let grad_x = 2.0 * lx / (rx * rx);
    let grad_y = 2.0 * ly / (ry * ry);
    let grad_world_x = grad_x * cos_a - grad_y * sin_a;
    let grad_world_y = grad_x * sin_a + grad_y * cos_a;
    let grad = Vec2::new(grad_world_x, grad_world_y);
    let normal = if grad.length() > 0.0001 {
        grad.normalized()
    } else {
        Vec2::new(1.0, 0.0)
    };

    // Approximate contact point on ellipse surface
    let contact = ellipse.center
        + Vec2::new(
            rx * cos_a * scaled_x / scaled_dist.max(0.001),
            ry * sin_a * scaled_y / scaled_dist.max(0.001),
        );
    let depth = (1.0 + scaled_circle_radius - scaled_dist) * scale;

    CollisionResult::collision(contact, normal, depth.max(0.0))
}

/// Ellipse vs Ellipse (approximate: bounding circles)
fn ellipse_ellipse(a: &super::shapes::Ellipse, b: &super::shapes::Ellipse) -> CollisionResult {
    circle_circle(&ellipse_bounding_circle(a), &ellipse_bounding_circle(b))
}

/// Ellipse vs convex polygon (approximate: bounding circle)
fn ellipse_polygon(e: &super::shapes::Ellipse, vertices: &[Vec2]) -> CollisionResult {
    circle_polygon(&ellipse_bounding_circle(e), vertices)
}

/// Capsule vs convex polygon
fn capsule_polygon(capsule: &super::shapes::Capsule, vertices: &[Vec2]) -> CollisionResult {
    let n = vertices.len();
    let mut closest_capsule = Vec2::zero();
    let mut closest_poly = Vec2::zero();
    let mut min_dist = f32::MAX;

    for i in 0..n {
        let (ca, cb) = closest_point_on_segments(
            capsule.start,
            capsule.end,
            vertices[i],
            vertices[(i + 1) % n],
        );
        let d = ca.distance_to(cb);
        if d < min_dist {
            min_dist = d;
            closest_capsule = ca;
            closest_poly = cb;
        }
    }

    // Also test if capsule segment endpoints are inside polygon
    let start_inside = point_in_polygon(capsule.start, vertices);
    let end_inside = point_in_polygon(capsule.end, vertices);

    if min_dist > capsule.radius && !start_inside && !end_inside {
        return CollisionResult::none();
    }

    let depth = capsule.radius - min_dist;
    let delta = closest_capsule - closest_poly;
    let normal = if delta.length() > 0.0001 {
        delta.normalized()
    } else {
        Vec2::new(1.0, 0.0)
    };

    CollisionResult::collision(closest_poly, normal, depth.max(0.0))
}

/// Circle vs Line segment
fn circle_line(circle: &super::shapes::Circle, line: &super::shapes::Line) -> CollisionResult {
    let (closest, _) = closest_point_on_segment(line.start, line.end, circle.center);
    let delta = circle.center - closest;
    let dist = delta.length();

    if dist >= circle.radius {
        return CollisionResult::none();
    }

    let depth = circle.radius - dist;
    let normal = if dist > 0.0001 {
        delta.normalized()
    } else {
        Vec2::new(1.0, 0.0)
    };
    CollisionResult::collision(closest, normal, depth)
}

/// Circle vs Ray
fn circle_ray(circle: &super::shapes::Circle, ray: &super::shapes::Ray) -> CollisionResult {
    let dir = ray.direction.normalized();
    let to_center = circle.center - ray.origin;
    let t = to_center.dot(dir).max(0.0);
    let closest = ray.origin + dir * t;
    let delta = circle.center - closest;
    let dist = delta.length();

    if dist >= circle.radius {
        return CollisionResult::none();
    }

    let depth = circle.radius - dist;
    let normal = if dist > 0.0001 {
        delta.normalized()
    } else {
        Vec2::new(1.0, 0.0)
    };
    CollisionResult::collision(closest, normal, depth)
}

/// Capsule vs Line segment
fn capsule_line(capsule: &super::shapes::Capsule, line: &super::shapes::Line) -> CollisionResult {
    let (ca, cl) = closest_point_on_segments(capsule.start, capsule.end, line.start, line.end);
    let delta = ca - cl;
    let dist = delta.length();

    if dist >= capsule.radius {
        return CollisionResult::none();
    }

    let depth = capsule.radius - dist;
    let normal = if dist > 0.0001 {
        delta.normalized()
    } else {
        Vec2::new(1.0, 0.0)
    };
    CollisionResult::collision(cl, normal, depth)
}

/// Capsule vs Ray
fn capsule_ray(capsule: &super::shapes::Capsule, ray: &super::shapes::Ray) -> CollisionResult {
    let dir = ray.direction.normalized();
    // Treat ray as a half-infinite segment; clamp t >= 0
    let d1 = capsule.end - capsule.start;
    let d2 = dir;
    let d3 = ray.origin - capsule.start;
    let a = d1.dot(d1);
    let b = d1.dot(d2);
    let c = d2.dot(d2);
    let dd = d1.dot(d3);
    let e = d2.dot(d3);
    let denom = a * c - b * b;

    let (s, t) = if denom.abs() < 0.0001 {
        let s = if a > 0.0001 {
            (dd / a).clamp(0.0, 1.0)
        } else {
            0.0
        };
        (s, 0.0_f32)
    } else {
        let s = ((b * e - c * dd) / denom).clamp(0.0, 1.0);
        let t = ((a * e - b * dd) / denom).max(0.0);
        (s, t)
    };

    let closest_capsule = capsule.start + d1 * s;
    let closest_ray = ray.origin + dir * t;
    let delta = closest_capsule - closest_ray;
    let dist = delta.length();

    if dist >= capsule.radius {
        return CollisionResult::none();
    }

    let depth = capsule.radius - dist;
    let normal = if dist > 0.0001 {
        delta.normalized()
    } else {
        Vec2::new(1.0, 0.0)
    };
    CollisionResult::collision(closest_ray, normal, depth)
}

/// Line vs convex polygon
fn line_polygon(line: &super::shapes::Line, vertices: &[Vec2]) -> CollisionResult {
    let n = vertices.len();
    let mut min_dist = f32::MAX;
    let mut best_ca = Vec2::zero();
    let mut best_cb = Vec2::zero();

    for i in 0..n {
        let (ca, cb) =
            closest_point_on_segments(line.start, line.end, vertices[i], vertices[(i + 1) % n]);
        let d = ca.distance_to(cb);
        if d < min_dist {
            min_dist = d;
            best_ca = ca;
            best_cb = cb;
        }
    }

    if min_dist > 0.0001 {
        // Check if line endpoints are inside polygon
        if !point_in_polygon(line.start, vertices) && !point_in_polygon(line.end, vertices) {
            return CollisionResult::none();
        }
    }

    let delta = best_ca - best_cb;
    let normal = if delta.length() > 0.0001 {
        delta.normalized()
    } else {
        Vec2::new(1.0, 0.0)
    };
    CollisionResult::collision(best_cb, normal, 0.0)
}

/// Ray vs convex polygon
fn ray_polygon(ray: &super::shapes::Ray, vertices: &[Vec2]) -> CollisionResult {
    let n = vertices.len();
    let dir = ray.direction.normalized();
    let mut min_t = f32::MAX;
    let mut contact = Vec2::zero();
    let mut hit_normal = Vec2::new(1.0, 0.0);

    for i in 0..n {
        let edge_start = vertices[i];
        let edge_end = vertices[(i + 1) % n];
        let edge = edge_end - edge_start;
        let denom = dir.cross(edge);
        if denom.abs() < 0.0001 {
            continue;
        }
        let to_edge = edge_start - ray.origin;
        let t = to_edge.cross(edge) / denom;
        let u = to_edge.cross(dir) / denom;
        if t >= 0.0 && (0.0..=1.0).contains(&u) && t < min_t {
            min_t = t;
            contact = ray.origin + dir * t;
            let edge_normal = Vec2::new(-edge.y, edge.x).normalized();
            hit_normal = if dir.dot(edge_normal) > 0.0 {
                -edge_normal
            } else {
                edge_normal
            };
        }
    }

    if min_t == f32::MAX {
        return CollisionResult::none();
    }

    CollisionResult::collision(contact, hit_normal, 0.0)
}

/// Line vs Line
fn line_line(a: &super::shapes::Line, b: &super::shapes::Line) -> CollisionResult {
    let d1 = a.end - a.start;
    let d2 = b.end - b.start;
    let denom = d1.cross(d2);

    if denom.abs() < 0.0001 {
        return CollisionResult::none(); // Parallel
    }

    let to_b = b.start - a.start;
    let t = to_b.cross(d2) / denom;
    let u = to_b.cross(d1) / denom;

    if !(0.0..=1.0).contains(&t) || !(0.0..=1.0).contains(&u) {
        return CollisionResult::none();
    }

    let contact = a.start + d1 * t;
    let normal = Vec2::new(-d1.y, d1.x).normalized();
    CollisionResult::collision(contact, normal, 0.0)
}

/// Ray vs Line segment
fn ray_line(ray: &super::shapes::Ray, line: &super::shapes::Line) -> CollisionResult {
    let dir = ray.direction.normalized();
    let edge = line.end - line.start;
    let denom = dir.cross(edge);

    if denom.abs() < 0.0001 {
        return CollisionResult::none();
    }

    let to_line = line.start - ray.origin;
    let t = to_line.cross(edge) / denom;
    let u = to_line.cross(dir) / denom;

    if t < 0.0 || !(0.0..=1.0).contains(&u) {
        return CollisionResult::none();
    }

    let contact = ray.origin + dir * t;
    let normal = Vec2::new(-edge.y, edge.x).normalized();
    let normal = if dir.dot(normal) > 0.0 {
        -normal
    } else {
        normal
    };
    CollisionResult::collision(contact, normal, 0.0)
}

/// Ray vs Ray
fn ray_ray(a: &super::shapes::Ray, b: &super::shapes::Ray) -> CollisionResult {
    let da = a.direction.normalized();
    let db = b.direction.normalized();
    let denom = da.cross(db);

    if denom.abs() < 0.0001 {
        return CollisionResult::none(); // Parallel
    }

    let to_b = b.origin - a.origin;
    let t = to_b.cross(db) / denom;
    let u = to_b.cross(da) / denom;

    if t < 0.0 || u < 0.0 {
        return CollisionResult::none();
    }

    let contact = a.origin + da * t;
    let normal = Vec2::new(-da.y, da.x).normalized();
    CollisionResult::collision(contact, normal, 0.0)
}
