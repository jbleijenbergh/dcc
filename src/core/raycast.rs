use glam::{Mat4, Vec2, Vec3, Vec4, Vec4Swizzles};

pub struct Ray {
    pub origin: Vec3,
    pub direction: Vec3,
}

impl Ray {
    pub fn from_screen(mouse_pos: Vec2, screen_size: Vec2, view: Mat4, proj: Mat4) -> Self {
        // Convert screen coordinates to Normalized Device Coordinates (NDC)
        // NDC X range: [-1.0, 1.0], Y range: [-1.0, 1.0] (Y is positive up)
        let ndc_x = (2.0 * mouse_pos.x) / screen_size.x - 1.0;
        let ndc_y = 1.0 - (2.0 * mouse_pos.y) / screen_size.y;

        // Inverse View-Projection matrix to go back to world space
        let inv_vp = (proj * view).inverse();

        // Project near and far points
        let ndc_near = Vec4::new(ndc_x, ndc_y, -1.0, 1.0);
        let ndc_far = Vec4::new(ndc_x, ndc_y, 1.0, 1.0);

        let near_world = inv_vp * ndc_near;
        let far_world = inv_vp * ndc_far;

        // Perform perspective divide
        let near_world_3 = near_world.xyz() / near_world.w;
        let far_world_3 = far_world.xyz() / far_world.w;

        Self {
            origin: near_world_3,
            direction: (far_world_3 - near_world_3).normalize(),
        }
    }
}

pub struct RaycastHit {
    pub distance: f32,
    pub uv: Vec2,
    pub point: Vec3,
}

pub fn intersect_document(ray: &Ray, document: &crate::graphics::mesh::Document) -> Option<RaycastHit> {
    let mut closest_hit: Option<RaycastHit> = None;

    let nodes = document.get_active_nodes();
    for (node, world_matrix) in &nodes {
        if let Some(ref mesh) = node.mesh {
            for primitive in &mesh.primitives {
                if let Some(hit) = intersect_primitive(ray, primitive, *world_matrix) {
                    if let Some(ref current) = closest_hit {
                        if hit.distance < current.distance {
                            closest_hit = Some(hit);
                        }
                    } else {
                        closest_hit = Some(hit);
                    }
                }
            }
        }
    }

    closest_hit
}

pub fn intersect_aabb(ray: &Ray, min: Vec3, max: Vec3) -> bool {
    let mut tmin = f32::MIN;
    let mut tmax = f32::MAX;

    // X slab
    if ray.direction.x.abs() > 1e-6 {
        let t1 = (min.x - ray.origin.x) / ray.direction.x;
        let t2 = (max.x - ray.origin.x) / ray.direction.x;
        tmin = tmin.max(t1.min(t2));
        tmax = tmax.min(t1.max(t2));
    } else if ray.origin.x < min.x || ray.origin.x > max.x {
        return false;
    }

    // Y slab
    if ray.direction.y.abs() > 1e-6 {
        let t1 = (min.y - ray.origin.y) / ray.direction.y;
        let t2 = (max.y - ray.origin.y) / ray.direction.y;
        tmin = tmin.max(t1.min(t2));
        tmax = tmax.min(t1.max(t2));
    } else if ray.origin.y < min.y || ray.origin.y > max.y {
        return false;
    }

    // Z slab
    if ray.direction.z.abs() > 1e-6 {
        let t1 = (min.z - ray.origin.z) / ray.direction.z;
        let t2 = (max.z - ray.origin.z) / ray.direction.z;
        tmin = tmin.max(t1.min(t2));
        tmax = tmax.min(t1.max(t2));
    } else if ray.origin.z < min.z || ray.origin.z > max.z {
        return false;
    }

    tmax >= tmin && tmax > 0.0
}

pub fn intersect_primitive(
    ray: &Ray,
    primitive: &crate::graphics::mesh::Primitive,
    world_matrix: Mat4,
) -> Option<RaycastHit> {
    // 1. Transform ray to primitive's local space
    let inv_matrix = world_matrix.inverse();
    let local_ray_dir = inv_matrix.transform_vector3(ray.direction);
    let len = local_ray_dir.length();
    if len < 1e-6 {
        return None;
    }
    let local_ray = Ray {
        origin: inv_matrix.transform_point3(ray.origin),
        direction: local_ray_dir / len,
    };

    // 2. Perform fast local AABB check
    if !intersect_aabb(&local_ray, primitive.bounds_min, primitive.bounds_max) {
        return None;
    }

    let mut closest_hit: Option<RaycastHit> = None;

    // 3. Linearly test triangles in local space
    for chunk in primitive.indices.chunks_exact(3) {
        let i0 = chunk[0] as usize;
        let i1 = chunk[1] as usize;
        let i2 = chunk[2] as usize;

        let v0 = &primitive.vertices[i0];
        let v1 = &primitive.vertices[i1];
        let v2 = &primitive.vertices[i2];

        let p0 = Vec3::from(v0.position);
        let p1 = Vec3::from(v1.position);
        let p2 = Vec3::from(v2.position);

        if let Some((t_local, u, v)) = intersect_triangle(&local_ray, p0, p1, p2) {
            // Transform hit point back to world space
            let hit_local_point = local_ray.origin + local_ray.direction * t_local;
            let hit_world_point = world_matrix.transform_point3(hit_local_point);
            let t_world = (hit_world_point - ray.origin).length();

            let w = 1.0 - u - v;
            let uv0 = Vec2::from(v0.tex_coords);
            let uv1 = Vec2::from(v1.tex_coords);
            let uv2 = Vec2::from(v2.tex_coords);
            let hit_uv = uv0 * w + uv1 * u + uv2 * v;

            let hit = RaycastHit {
                distance: t_world,
                uv: hit_uv,
                point: hit_world_point,
            };

            if let Some(ref current) = closest_hit {
                if hit.distance < current.distance {
                    closest_hit = Some(hit);
                }
            } else {
                closest_hit = Some(hit);
            }
        }
    }

    closest_hit
}

pub fn intersect_triangle(ray: &Ray, p0: Vec3, p1: Vec3, p2: Vec3) -> Option<(f32, f32, f32)> {
    // Möller-Trumbore intersection algorithm
    let e1 = p1 - p0;
    let e2 = p2 - p0;
    let h = ray.direction.cross(e2);
    let a = e1.dot(h);

    // Parallel or back-facing skip
    if a.abs() < 1e-6 {
        return None;
    }

    let f = 1.0 / a;
    let s = ray.origin - p0;
    let u = f * s.dot(h);

    if u < 0.0 || u > 1.0 {
        return None;
    }

    let q = s.cross(e1);
    let v = f * ray.direction.dot(q);

    if v < 0.0 || u + v > 1.0 {
        return None;
    }

    let t = f * e2.dot(q);
    if t > 0.0001 {
        Some((t, u, v))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod ray_tests {
        use super::*;

        #[test]
        fn test_ray_from_screen_center() {
            let mouse_pos = Vec2::new(100.0, 100.0);
            let screen_size = Vec2::new(200.0, 200.0);
            let view = Mat4::IDENTITY;
            let proj = Mat4::IDENTITY;

            let ray = Ray::from_screen(mouse_pos, screen_size, view, proj);

            assert_eq!(ray.origin, Vec3::new(0.0, 0.0, -1.0));
            assert_eq!(ray.direction, Vec3::new(0.0, 0.0, 1.0));
        }
    }

    mod intersection_tests {
        use super::*;

        #[test]
        fn test_intersect_triangle_hit() {
            let ray = Ray {
                origin: Vec3::new(0.0, 0.0, -2.0),
                direction: Vec3::new(0.0, 0.0, 1.0),
            };
            let p0 = Vec3::new(-1.0, -1.0, 0.0);
            let p1 = Vec3::new(1.0, -1.0, 0.0);
            let p2 = Vec3::new(0.0, 1.0, 0.0);

            let result = intersect_triangle(&ray, p0, p1, p2);
            assert!(result.is_some());
            let (t, u, v) = result.unwrap();
            assert!((t - 2.0).abs() < 1e-4);
            assert!(u >= 0.0 && u <= 1.0);
            assert!(v >= 0.0 && v <= 1.0);
        }

        #[test]
        fn test_intersect_triangle_miss() {
            let ray = Ray {
                origin: Vec3::new(2.0, 2.0, -2.0),
                direction: Vec3::new(0.0, 0.0, 1.0),
            };
            let p0 = Vec3::new(-1.0, -1.0, 0.0);
            let p1 = Vec3::new(1.0, -1.0, 0.0);
            let p2 = Vec3::new(0.0, 1.0, 0.0);

            let result = intersect_triangle(&ray, p0, p1, p2);
            assert!(result.is_none());
        }

        #[test]
        fn test_intersect_triangle_parallel() {
            let ray = Ray {
                origin: Vec3::new(0.0, 0.0, -2.0),
                direction: Vec3::new(1.0, 0.0, 0.0), // parallel to XY plane
            };
            let p0 = Vec3::new(-1.0, -1.0, 0.0);
            let p1 = Vec3::new(1.0, -1.0, 0.0);
            let p2 = Vec3::new(0.0, 1.0, 0.0);

            let result = intersect_triangle(&ray, p0, p1, p2);
            assert!(result.is_none());
        }

        #[test]
        fn test_intersect_triangle_opposite() {
            let ray = Ray {
                origin: Vec3::new(0.0, 0.0, -2.0),
                direction: Vec3::new(0.0, 0.0, -1.0), // pointing away
            };
            let p0 = Vec3::new(-1.0, -1.0, 0.0);
            let p1 = Vec3::new(1.0, -1.0, 0.0);
            let p2 = Vec3::new(0.0, 1.0, 0.0);

            let result = intersect_triangle(&ray, p0, p1, p2);
            assert!(result.is_none());
        }
    }
}
