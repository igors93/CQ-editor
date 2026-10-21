//! Spatial acceleration structures shared by scene queries.

use glam::Vec3;

const LEAF_TRIANGLES: usize = 8;

#[derive(Clone, Copy, Debug)]
struct Bounds {
    min: Vec3,
    max: Vec3,
}

impl Bounds {
    const EMPTY: Self = Self {
        min: Vec3::splat(f32::INFINITY),
        max: Vec3::splat(f32::NEG_INFINITY),
    };

    fn include(mut self, point: Vec3) -> Self {
        self.min = self.min.min(point);
        self.max = self.max.max(point);
        self
    }

    fn union(self, other: Self) -> Self {
        Self {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }
}

#[derive(Debug)]
enum NodeKind {
    Leaf { start: usize, count: usize },
    Branch { left: usize, right: usize },
}

#[derive(Debug)]
struct Node {
    bounds: Bounds,
    kind: NodeKind,
}

#[derive(Debug)]
struct Triangle {
    index: usize,
    bounds: Bounds,
    center: Vec3,
}

/// Bounding-volume hierarchy over a mesh's triangle list.
#[derive(Debug, Default)]
pub(crate) struct TriangleBvh {
    nodes: Vec<Node>,
    triangles: Vec<usize>,
}

impl TriangleBvh {
    pub(crate) fn build(positions: &[[f32; 3]], indices: &[u32]) -> Self {
        let vertex = |index: u32| positions.get(index as usize).copied().map(Vec3::from_array);
        let mut triangles: Vec<Triangle> = indices
            .as_chunks::<3>()
            .0
            .iter()
            .enumerate()
            .filter_map(|(index, triangle)| {
                let (Some(a), Some(b), Some(c)) = (
                    vertex(triangle[0]),
                    vertex(triangle[1]),
                    vertex(triangle[2]),
                ) else {
                    return None;
                };
                let bounds = Bounds::EMPTY.include(a).include(b).include(c);
                Some(Triangle {
                    index,
                    bounds,
                    center: (a + b + c) / 3.0,
                })
            })
            .collect();
        if triangles.is_empty() {
            return Self::default();
        }

        let mut result = Self::default();
        build_node(&mut result.nodes, &mut result.triangles, &mut triangles);
        result
    }

    pub(crate) fn nearest(
        &self,
        origin: Vec3,
        direction: Vec3,
        positions: &[[f32; 3]],
        indices: &[u32],
    ) -> Option<(f32, usize)> {
        self.nearest_with_count(origin, direction, positions, indices)
            .0
    }

    pub(crate) fn nearest_with_count(
        &self,
        origin: Vec3,
        direction: Vec3,
        positions: &[[f32; 3]],
        indices: &[u32],
    ) -> (Option<(f32, usize)>, usize) {
        if self.nodes.is_empty() {
            return (None, 0);
        }
        let mut stack = vec![0usize];
        let mut nearest = None;
        let mut tested = 0;
        while let Some(node_index) = stack.pop() {
            let node = &self.nodes[node_index];
            let limit = nearest.map_or(f32::INFINITY, |(distance, _)| distance);
            if ray_box(origin, direction, node.bounds).is_none_or(|near| near > limit) {
                continue;
            }
            match node.kind {
                NodeKind::Leaf { start, count } => {
                    for &triangle_index in &self.triangles[start..start + count] {
                        tested += 1;
                        let Some(triangle) = indices
                            .get(triangle_index * 3..triangle_index * 3 + 3)
                            .and_then(|slice| <&[u32; 3]>::try_from(slice).ok())
                        else {
                            continue;
                        };
                        let vertex = |index: u32| {
                            positions.get(index as usize).copied().map(Vec3::from_array)
                        };
                        let (Some(a), Some(b), Some(c)) = (
                            vertex(triangle[0]),
                            vertex(triangle[1]),
                            vertex(triangle[2]),
                        ) else {
                            continue;
                        };
                        if let Some(distance) = ray_triangle(origin, direction, a, b, c)
                            && distance < nearest.map_or(f32::INFINITY, |hit| hit.0)
                        {
                            nearest = Some((distance, triangle_index));
                        }
                    }
                }
                NodeKind::Branch { left, right } => {
                    let left_distance = ray_box(origin, direction, self.nodes[left].bounds);
                    let right_distance = ray_box(origin, direction, self.nodes[right].bounds);
                    match (left_distance, right_distance) {
                        (Some(a), Some(b)) if a <= b => {
                            stack.push(right);
                            stack.push(left);
                        }
                        (Some(_), Some(_)) => {
                            stack.push(left);
                            stack.push(right);
                        }
                        (Some(_), None) => stack.push(left),
                        (None, Some(_)) => stack.push(right),
                        (None, None) => {}
                    }
                }
            }
        }
        (nearest, tested)
    }
}

fn build_node(
    nodes: &mut Vec<Node>,
    ordered: &mut Vec<usize>,
    triangles: &mut [Triangle],
) -> usize {
    let bounds = triangles.iter().fold(Bounds::EMPTY, |bounds, triangle| {
        bounds.union(triangle.bounds)
    });
    let index = nodes.len();
    nodes.push(Node {
        bounds,
        kind: NodeKind::Leaf { start: 0, count: 0 },
    });

    if triangles.len() <= LEAF_TRIANGLES {
        let start = ordered.len();
        ordered.extend(triangles.iter().map(|triangle| triangle.index));
        nodes[index].kind = NodeKind::Leaf {
            start,
            count: triangles.len(),
        };
        return index;
    }

    let centers = triangles.iter().fold(Bounds::EMPTY, |bounds, triangle| {
        bounds.include(triangle.center)
    });
    let size = centers.max - centers.min;
    let axis = if size.x >= size.y && size.x >= size.z {
        0
    } else if size.y >= size.z {
        1
    } else {
        2
    };
    triangles.sort_unstable_by(|a, b| a.center[axis].total_cmp(&b.center[axis]));
    let middle = triangles.len() / 2;
    let (left_triangles, right_triangles) = triangles.split_at_mut(middle);
    let left = build_node(nodes, ordered, left_triangles);
    let right = build_node(nodes, ordered, right_triangles);
    nodes[index].kind = NodeKind::Branch { left, right };
    index
}

/// Slab intersection returning the first non-negative distance.
fn ray_box(origin: Vec3, direction: Vec3, bounds: Bounds) -> Option<f32> {
    let mut near: f32 = 0.0;
    let mut far = f32::INFINITY;
    for axis in 0..3 {
        if direction[axis].abs() < 1e-12 {
            if origin[axis] < bounds.min[axis] || origin[axis] > bounds.max[axis] {
                return None;
            }
            continue;
        }
        let inverse = 1.0 / direction[axis];
        let mut a = (bounds.min[axis] - origin[axis]) * inverse;
        let mut b = (bounds.max[axis] - origin[axis]) * inverse;
        if a > b {
            std::mem::swap(&mut a, &mut b);
        }
        near = near.max(a);
        far = far.min(b);
        if near > far {
            return None;
        }
    }
    (far >= 0.0).then_some(near)
}

/// Moller-Trumbore intersection, accepting both triangle sides.
fn ray_triangle(origin: Vec3, direction: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let (ab, ac) = (b - a, c - a);
    let p = direction.cross(ac);
    let determinant = ab.dot(p);
    if determinant.abs() < 1e-12 {
        return None;
    }
    let inverse = 1.0 / determinant;
    let s = origin - a;
    let u = s.dot(p) * inverse;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(ab);
    let v = direction.dot(q) * inverse;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let distance = ac.dot(q) * inverse;
    (distance > 0.0).then_some(distance)
}
