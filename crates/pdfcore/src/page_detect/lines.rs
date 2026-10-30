//! Candidate quadrilaterals: Hough lines from the Canny map, plus the largest
//! bright region as a fallback for low-contrast page borders.

use super::Quad;
use image::{GrayImage, Luma};
use imageproc::contours::find_contours;
use imageproc::geometry::{approximate_polygon_dp, convex_hull, min_area_rect};
use imageproc::point::Point as GeoPoint;
use std::f32::consts::{FRAC_PI_2, PI};

/// Angle resolution of the Hough accumulator (one degree per bin).
const HOUGH_THETAS: usize = 180;

/// Lines kept from the accumulator, strongest first.
const MAX_LINES: usize = 24;

/// Quadrilateral candidates scored per photo.
const MAX_CANDIDATES: usize = 24;

/// A Hough line: `x cos(theta) + y sin(theta) = rho`.
struct Line {
    theta: f32,
    rho: f32,
    votes: u32,
}

/// Builds quadrilaterals from the Hough lines: two roughly parallel lines and
/// two more that cross them at about a right angle form the page outline.
pub(super) fn quad_candidates_from_lines(edges: &GrayImage) -> Vec<Quad> {
    let lines = hough_lines(edges);
    if lines.len() < 4 {
        return Vec::new();
    }
    let directions: Vec<f32> = lines.iter().map(|line| direction(line.theta)).collect();
    let mut pairs: Vec<(usize, usize, u32)> = Vec::new();
    for first in 0..lines.len() {
        for second in (first + 1)..lines.len() {
            if angle_between(directions[first], directions[second]) < 25.0f32.to_radians() {
                pairs.push((first, second, lines[first].votes + lines[second].votes));
            }
        }
    }
    pairs.sort_by_key(|pair| std::cmp::Reverse(pair.2));
    pairs.truncate(MAX_CANDIDATES);
    let mut quads: Vec<Quad> = Vec::new();
    for (index, (first_a, second_a, _)) in pairs.iter().enumerate() {
        for (first_b, second_b, _) in pairs.iter().skip(index + 1) {
            let crossing = angle_between(directions[*first_a], directions[*first_b]);
            if (crossing - FRAC_PI_2).abs() > 30.0f32.to_radians() {
                continue;
            }
            let shared = [first_a, second_a].iter().any(|line| *line == first_b || *line == second_b);
            if shared {
                continue;
            }
            let Some(corner_a) = intersect(&lines[*first_a], &lines[*first_b]) else {
                continue;
            };
            let Some(corner_b) = intersect(&lines[*second_a], &lines[*first_b]) else {
                continue;
            };
            let Some(corner_c) = intersect(&lines[*second_a], &lines[*second_b]) else {
                continue;
            };
            let Some(corner_d) = intersect(&lines[*first_a], &lines[*second_b]) else {
                continue;
            };
            let quad = crate::scan::order_corners([corner_a, corner_b, corner_c, corner_d]);
            if quads.iter().any(|known| same_quad(known, &quad)) {
                continue;
            }
            quads.push(quad);
            if quads.len() >= MAX_CANDIDATES {
                return quads;
            }
        }
    }
    quads
}

fn hough_lines(edges: &GrayImage) -> Vec<Line> {
    let (width, height) = edges.dimensions();
    let diagonal = ((width * width + height * height) as f32).sqrt().ceil() as i32;
    let rho_bins = (2 * diagonal + 1) as usize;
    let mut accumulator = vec![0u32; rho_bins * HOUGH_THETAS];
    let cosines: Vec<f32> = (0..HOUGH_THETAS).map(|bin| (bin as f32 * PI / 180.0).cos()).collect();
    let sines: Vec<f32> = (0..HOUGH_THETAS).map(|bin| (bin as f32 * PI / 180.0).sin()).collect();
    let mut edge_pixels = 0usize;
    for (x, y, pixel) in edges.enumerate_pixels() {
        if pixel[0] == 0 {
            continue;
        }
        edge_pixels += 1;
        for (bin, (cosine, sine)) in cosines.iter().zip(sines.iter()).enumerate() {
            let rho = x as f32 * cosine + y as f32 * sine;
            let bucket = rho.round() as i32 + diagonal;
            if bucket >= 0 && bucket < 2 * diagonal + 1 {
                accumulator[bucket as usize * HOUGH_THETAS + bin] += 1;
            }
        }
    }
    if edge_pixels == 0 {
        return Vec::new();
    }
    let highest = accumulator.iter().copied().max().unwrap_or(0) as f32;
    let threshold = (highest * 0.35).max(12.0) as u32;
    let mut peaks: Vec<(u32, usize, usize)> = accumulator
        .iter()
        .copied()
        .enumerate()
        .filter(|(_, votes)| *votes >= threshold)
        .map(|(index, votes)| (votes, index / HOUGH_THETAS, index % HOUGH_THETAS))
        .collect();
    peaks.sort_by_key(|peak| std::cmp::Reverse(peak.0));
    let mut lines: Vec<Line> = Vec::new();
    for (votes, bucket, theta_bin) in peaks {
        let rho = bucket as f32 - diagonal as f32;
        let theta = theta_bin as f32 * PI / 180.0;
        let duplicate = lines.iter().any(|line: &Line| {
            angle_between(line.theta, theta) <= 4.0f32.to_radians() && (line.rho - rho).abs() <= 6.0
        });
        if duplicate {
            continue;
        }
        lines.push(Line { theta, rho, votes });
        if lines.len() >= MAX_LINES {
            break;
        }
    }
    lines
}

/// The direction of a line, modulo half a turn (0..PI).
fn direction(theta: f32) -> f32 {
    (theta + FRAC_PI_2).rem_euclid(PI)
}

/// Smallest angle between two directions given modulo half a turn.
fn angle_between(left: f32, right: f32) -> f32 {
    let difference = (left - right).abs().rem_euclid(PI);
    difference.min(PI - difference)
}

fn intersect(left: &Line, right: &Line) -> Option<[f32; 2]> {
    let determinant = left.theta.cos() * right.theta.sin() - left.theta.sin() * right.theta.cos();
    if determinant.abs() < 1e-3 {
        return None;
    }
    let x = (left.rho * right.theta.sin() - left.theta.sin() * right.rho) / determinant;
    let y = (left.theta.cos() * right.rho - left.rho * right.theta.cos()) / determinant;
    Some([x, y])
}

fn same_quad(left: &Quad, right: &Quad) -> bool {
    left.iter().zip(right.iter()).all(|(a, b)| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt() < 3.0)
}

/// The largest bright region (paper usually is bright): its convex hull,
/// simplified down to four corners.
pub(super) fn quad_from_bright_regions(luma: &GrayImage) -> Option<Quad> {
    let threshold = imageproc::contrast::otsu_level(luma);
    let binary = GrayImage::from_fn(luma.width(), luma.height(), |x, y| {
        Luma([if luma.get_pixel(x, y)[0] > threshold { 255u8 } else { 0u8 }])
    });
    let contours = find_contours::<i32>(&binary);
    let mut best_points: Option<Vec<GeoPoint<i32>>> = None;
    let mut best_area = 0.0f32;
    for contour in contours {
        if contour.points.len() < 8 {
            continue;
        }
        let area = polygon_area_i32(&contour.points);
        if area > best_area {
            best_area = area;
            best_points = Some(contour.points);
        }
    }
    let points = best_points?;
    let hull = convex_hull(points);
    if hull.len() < 4 {
        return None;
    }
    let hull_f32: Vec<GeoPoint<f32>> = hull.iter().map(|point| GeoPoint::new(point.x as f32, point.y as f32)).collect();
    let longest = luma.width().max(luma.height()) as f64;
    for step in [0.01, 0.02, 0.03, 0.05, 0.08, 0.12] {
        let simplified = approximate_polygon_dp(&hull_f32, longest * step, true);
        if simplified.len() == 4 {
            return Some([
                [simplified[0].x, simplified[0].y],
                [simplified[1].x, simplified[1].y],
                [simplified[2].x, simplified[2].y],
                [simplified[3].x, simplified[3].y],
            ]);
        }
    }
    let rect = min_area_rect(&hull_f32);
    Some(rect.map(|point| [point.x, point.y]))
}

fn polygon_area_i32(points: &[GeoPoint<i32>]) -> f32 {
    let mut twice = 0i64;
    for index in 0..points.len() {
        let current = points[index];
        let next = points[(index + 1) % points.len()];
        twice += current.x as i64 * next.y as i64 - next.x as i64 * current.y as i64;
    }
    (twice as f32 * 0.5).abs()
}
