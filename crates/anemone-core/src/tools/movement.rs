//! Room movement — locations, collision map, idle wander.
//! 1:1 port of Brain movement logic.

use rand::Rng;
use std::collections::HashSet;
use std::sync::LazyLock;

use crate::types::Position;

/// Room is 12x12 tiles
pub const ROOM_COLS: i32 = 12;
pub const ROOM_ROWS: i32 = 12;

/// Collision map extracted from the Smallville tilemap.
/// 'X' = blocked, '.' = walkable.
const COLLISION_ROWS: &[&str] = &[
    "XXXX..XXXXXX", // row 0
    "..XX...XX...", // row 1
    ".......XXXX.", // row 2
    "..XX...XX...", // row 3
    "..XX...XX...", // row 4
    "........XX..", // row 5
    "............", // row 6
    "..XXXXXX..XX", // row 7
    "..XX...X..X.", // row 8
    "....XXX...X.", // row 9
    "XX...X.....X", // row 10
    "X....X......", // row 11
];

/// Pre-computed set of blocked tile coordinates.
static BLOCKED: LazyLock<HashSet<(i32, i32)>> = LazyLock::new(|| {
    let mut blocked = HashSet::new();
    for (y, row) in COLLISION_ROWS.iter().enumerate() {
        for (x, ch) in row.chars().enumerate() {
            if ch == 'X' {
                blocked.insert((x as i32, y as i32));
            }
        }
    }
    blocked
});

/// Check if a tile is blocked.
pub fn is_blocked(x: i32, y: i32) -> bool {
    BLOCKED.contains(&(x, y))
}

/// Check if a position is valid (in bounds and not blocked).
pub fn is_valid_position(x: i32, y: i32) -> bool {
    x >= 0 && x < ROOM_COLS && y >= 0 && y < ROOM_ROWS && !is_blocked(x, y)
}

/// Handle the move tool — move to a named location.
pub fn handle_move(position: &mut Position, location: &str) -> String {
    if let Some(target) = crate::types::room_location(location) {
        position.x = target.x;
        position.y = target.y;
        format!("Moved to {}.", location)
    } else {
        format!("Unknown location: {}", location)
    }
}

/// Random ±1 step between thoughts (idle wander).
pub fn idle_wander(position: &mut Position) {
    let mut rng = rand::thread_rng();
    let dx: i32 = rng.gen_range(-1..=1);
    let dy: i32 = rng.gen_range(-1..=1);
    let nx = position.x + dx;
    let ny = position.y + dy;
    if is_valid_position(nx, ny) {
        position.x = nx;
        position.y = ny;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blocked_tiles() {
        // Row 0: "XXXX..XXXXXX"
        assert!(is_blocked(0, 0));
        assert!(is_blocked(3, 0));
        assert!(!is_blocked(4, 0));
        assert!(!is_blocked(5, 0));
        assert!(is_blocked(6, 0));

        // Row 6: "............" (all clear)
        for x in 0..12 {
            assert!(!is_blocked(x, 6));
        }
    }

    #[test]
    fn test_valid_position() {
        assert!(is_valid_position(5, 5)); // rug/center
        assert!(!is_valid_position(-1, 0)); // out of bounds
        assert!(!is_valid_position(12, 0)); // out of bounds
        assert!(!is_valid_position(0, 0)); // blocked
    }

    #[test]
    fn test_handle_move() {
        let mut pos = Position { x: 5, y: 5 };
        let result = handle_move(&mut pos, "desk");
        assert_eq!(pos.x, 10);
        assert_eq!(pos.y, 1);
        assert!(result.contains("desk"));
    }

    #[test]
    fn test_handle_move_unknown() {
        let mut pos = Position { x: 5, y: 5 };
        let result = handle_move(&mut pos, "bathroom");
        assert!(result.contains("Unknown"));
        // Position should not change
        assert_eq!(pos.x, 5);
        assert_eq!(pos.y, 5);
    }

    #[test]
    fn test_idle_wander_stays_valid() {
        let mut pos = Position { x: 5, y: 5 };
        for _ in 0..100 {
            idle_wander(&mut pos);
            assert!(is_valid_position(pos.x, pos.y));
        }
    }
}
