extern crate minimax;

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::default::Default;
use std::fmt::{Display, Formatter, Result};

// TODO benchmarks: placement heavy starting from empty board; movement-heavy starting from full board

// Ideas for board representation:
// 1) Grid based: Keep a mostly empty grid with entries for what's in each cell.
//      The grid will need to expand and/or translate if the hive gets too long or moves.
// 2) Graph based: Each piece points to its neighbors.
//      Recalculating connectedness seems complex.
//      Even computing adjacent nodes may require walking all the way through the other pieces...
// 3) Location based: Hashmap or other association from coordinates to piece.
//      Don't need to resize the grid, doesn't take more space than necessary.
// 4) Graph based with grid backup.
//      Dynamically allocate used and empty adjacent hexes with indexes.
//      Compact adjacency list for each node. Generate new nodes when expanding.

// Location-based model.
// Hex coordinates. Grid connections plus one of the diagonals. First bug is at (0,0).
pub type Loc = (i8, i8);

fn adjacent(loc: Loc) -> [Loc; 6] {
    let (x, y) = loc;
    // In clockwise order
    [(x - 1, y - 1), (x, y - 1), (x + 1, y), (x + 1, y + 1), (x, y + 1), (x - 1, y)]
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Bug {
    Queen,
    Grasshopper,
    Spider,
    Ant,
    Beetle,
}

impl Bug {
    fn index(&self) -> usize {
        match *self {
            Bug::Queen => 0,
            Bug::Grasshopper => 1,
            Bug::Spider => 2,
            Bug::Ant => 3,
            Bug::Beetle => 4,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
enum Color {
    Black,
    White,
}

// A tile on the board.
struct Tile {
    bug: Bug,
    color: Color,
    underneath: Option<Box<Tile>>,
}

pub struct Board {
    // TODO: try some simpler association list.
    grid: HashMap<Loc, Tile>,
    remaining: [[u8; 5]; 2],
    move_num: u16,
}

impl Board {
    fn to_move(&self) -> Color {
        if self.move_num % 2 == 0 {
            Color::Black
        } else {
            Color::White
        }
    }

    fn get(&self, loc: Loc) -> Option<&Tile> {
        self.grid.get(&loc)
    }

    fn insert(&mut self, loc: Loc, bug: Bug, color: Color) {
        if let Some(prev) = self.grid.insert(loc, Tile { bug: bug, color: color, underneath: None })
        {
            self.grid.get_mut(&loc).unwrap().underneath = Some(Box::new(prev));
        }
    }

    // Asserts that there is something there.
    fn remove(&mut self, loc: Loc) -> Tile {
        let mut tile = self.grid.remove(&loc).unwrap();
        if let Some(stack) = tile.underneath.take() {
            self.grid.insert(loc, *stack);
        }
        tile
    }

    fn get_remaining(&self) -> &[u8; 5] {
        &self.remaining[self.move_num as usize & 1]
    }

    fn mut_remaining(&mut self) -> &mut [u8; 5] {
        &mut self.remaining[self.move_num as usize & 1]
    }

    fn get_available_bugs(&self) -> [(Bug, u8); 5] {
        let remaining = self.get_remaining();
        [
            (Bug::Queen, remaining[0]),
            (Bug::Grasshopper, remaining[1]),
            (Bug::Spider, remaining[2]),
            (Bug::Ant, remaining[3]),
            (Bug::Beetle, remaining[4]),
        ]
    }

    fn queen_required(&self) -> bool {
        self.move_num > 5 && self.get_remaining()[0] > 0
    }
}

#[test]
fn test_gen_placement() {
    let mut board = Board::default();
    for i in 1..5 {
        board.remaining[0][i] = 0;
        board.remaining[1][i] = 0;
    }
    board.insert((0, 0), Bug::Queen, Color::Black);
    board.insert((1, 0), Bug::Queen, Color::White);
    println!("{}", board);
    let mut moves = [None; 100];
    let mut n = 0;
    board.generate_placements(&mut moves, &mut n);
    assert_eq!(3, n);
    moves[..n].sort();
    assert_eq!(moves[0].unwrap().place().unwrap(), (-1, -1));
    assert_eq!(moves[1].unwrap().place().unwrap(), (-1, 0));
    assert_eq!(moves[2].unwrap().place().unwrap(), (0, 1));
}

impl Default for Board {
    fn default() -> Self {
        Board { grid: HashMap::new(), remaining: [[1, 3, 3, 2, 2], [1, 3, 3, 2, 2]], move_num: 0 }
    }
}

impl Display for Board {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(f, "{}", self.fancy_fmt())
    }
}

impl Board {
    fn bounding_box(&self) -> (i8, i8, i8, i8) {
        if self.grid.is_empty() {
            return (0, 1, 0, 1);
        }
        let mut minx = i8::MAX;
        let mut maxx = i8::MIN;
        let mut miny = i8::MAX;
        let mut maxy = i8::MIN;
        for &(x, y) in self.grid.keys() {
            minx = std::cmp::min(minx, x);
            maxx = std::cmp::max(maxx, x);
            miny = std::cmp::min(miny, y);
            maxy = std::cmp::max(maxy, y);
        }
        (minx, maxx - minx + 1, miny, maxy - miny + 1)
    }

    fn fancy_fmt(&self) -> String {
        let mut out = String::new();
        let (startx, dx, starty, dy) = self.bounding_box();
        for y in starty - 1..starty + dy + 1 {
            // Print prefix to get staggered hex rows
            let buflen = dy + starty - y;
            if buflen % 2 == 1 {
                out.push(' ');
            }
            for _ in 0..buflen / 2 {
                out.push('\u{ff0e}');
            }

            for x in startx - 1..startx + dx + 1 {
                if let Some(tile) = self.get((x, y)) {
                    if tile.color == Color::White {
                        // Invert terminal background color for white pieces.
                        out.push_str("\x1b[3m");
                    }
                    out.push(match tile.bug {
                        Bug::Queen => '\u{1f41d}',       // HONEYBEE
                        Bug::Grasshopper => '\u{1f997}', // CRICKET
                        Bug::Spider => '\u{1f577}',      // SPIDER
                        Bug::Ant => '\u{1f41c}',         // ANT
                        Bug::Beetle => '\u{1fab2}',      // BEETLE
                                                          //Bug::Ladybug => '\u{1f41e}'', // LADY BEETLE
                                                          //Bug::Mosquito => '\u{1f99f}', // MOSQUITO
                    });
                    if tile.color == Color::White {
                        // Reset coloring.
                        out.push_str("\x1b[m");
                    }
                } else {
                    // Empty cell. Full width period.
                    out.push('\u{ff0e}');
                }
            }
            out.push('\n');
        }
        out
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Move {
    Place(Loc, Bug),
    Movement(Loc, Loc),
}

// For reproducible tests.
impl Ord for Move {
    fn cmp(&self, other: &Self) -> Ordering {
        match *self {
            Move::Place(loc, bug) => {
                if let Move::Place(loc2, bug2) = other {
                    (loc, bug.index()).cmp(&(*loc2, bug2.index()))
                } else {
                    Ordering::Less
                }
            }
            Move::Movement(start, end) => {
                if let Move::Movement(start2, end2) = other {
                    (start, end).cmp(&(*start2, *end2))
                } else {
                    Ordering::Greater
                }
            }
        }
    }
}

impl PartialOrd for Move {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Move {
    #[cfg(test)]
    fn place(&self) -> Option<Loc> {
        if let Move::Place(loc, _) = self {
            Some(*loc)
        } else {
            None
        }
    }
}

impl minimax::Move for Move {
    type G = Game;
    fn apply(&self, board: &mut Board) {
        match *self {
            Move::Place(loc, bug) => {
                board.insert(loc, bug, board.to_move());
                board.mut_remaining()[bug.index()] -= 1;
            }
            Move::Movement(start, end) => {
                let tile = board.remove(start);
                board.insert(end, tile.bug, tile.color);
            }
        }
        board.move_num += 1;
    }
    fn undo(&self, board: &mut Board) {
        match *self {
            Move::Place(loc, bug) => {
                board.remove(loc);
                board.mut_remaining()[bug.index()] += 1;
            }
            Move::Movement(start, end) => {
                let tile = board.remove(end);
                board.insert(start, tile.bug, tile.color);
            }
        }
        board.move_num -= 1;
    }
}

impl Board {
    fn generate_placements(&self, board_moves: &mut [Option<Move>], n: &mut usize) {
        // First find empty spaces next to the correct color bugs.
        let mut available = HashSet::new();
        for (&loc, tile) in self.grid.iter() {
            if tile.color != self.to_move() {
                continue;
            }
            for &pos in adjacent(loc).iter() {
                if self.get(pos).is_none() {
                    available.insert(pos);
                }
            }
        }

        // Use empty spaces that have no opposite colored tiles adjacent.
        for &pos in available.iter() {
            let placeable = adjacent(pos)
                .iter()
                .all(|adj| self.get(*adj).map(|tile| tile.color == self.to_move()).unwrap_or(true));
            if placeable {
                for (bug, num_left) in self.get_available_bugs().iter() {
                    if self.queen_required() && *bug != Bug::Queen {
                        continue;
                    }
                    if *num_left > 0 {
                        board_moves[*n] = Some(Move::Place(pos, *bug));
                        *n += 1;
                    }
                }
            }
        }
    }

    // TODO: Linear algorithm to find all cut vertexes:
    // Algorithm explanation: https://web.archive.org/web/20180830110222/https://www.eecs.wsu.edu/~holder/courses/CptS223/spr08/slides/graphapps.pdf
    // Example code: https://cp-algorithms.com/graph/cutpoints.html
    //
    // TODO: cache movability for each tile, and somehow iteratively update it
    // Need to persist the DFS tree from an arbitrary root.
    // Adding a tile just adds a leaf to one of its neighbors
    // Removing a tile means recomputing a path to the root for any children of the removed node.
    // Hmm, maybe not. DFS iteration order is important.
    fn is_cut_vertex(&self, loc: Loc) -> bool {
        let mut visited = HashSet::new();
        visited.insert(loc);
        // Start searching from one arbitrary neighbor.
        // This should never be called on a disconnected node.
        let start: Loc =
            *adjacent(loc).iter().filter(|adj| self.get(**adj).is_some()).next().unwrap();
        let mut queue = vec![start];
        while let Some(node) = queue.pop() {
            if visited.contains(&node) {
                continue;
            }
            visited.insert(node);
            for &adj in adjacent(node).iter() {
                if self.get(adj).is_some() {
                    queue.push(adj);
                }
            }
        }
        visited.len() != self.grid.len()
    }

    // For a position on the outside (whether occupied or not), find all
    // adjacent locations still connected to the hive that are slideable.
    // A slideable position has 2 empty slots next to an occupied slot.
    // For all 2^6 possibilities, there can be 0, 2, or 4 slideable neighbors.
    fn slideable_adjacent(&self, loc: Loc) -> [Option<Loc>; 4] {
        let mut out = [None; 4];
        let mut n = 0;
        let neighbors = adjacent(loc);
        // Each bit is whether neighbor is occupied.
        let mut occupied = 0;
        for neighbor in neighbors.iter().rev() {
            occupied <<= 1;
            if self.get(*neighbor).is_some() {
                occupied |= 1;
            }
        }
        // Wrap around in each direction
        occupied |= occupied << 6;
        occupied = (occupied << 1) | (occupied >> 5) & 1;
        let mut slideable = !occupied & (occupied << 1 ^ occupied >> 1);

        for neighbor in &neighbors {
            slideable >>= 1;
            if slideable & 1 != 0 {
                out[n] = Some(*neighbor);
                n += 1;
            }
        }

        out
    }

    // From any bug on top of a stack. Walk or jump down in any direction.
    fn generate_stack_walking(&self, loc: Loc, board_moves: &mut [Option<Move>], n: &mut usize) {
        for &adj in adjacent(loc).iter() {
            if self.get(adj).is_none() {
                board_moves[*n] = Some(Move::Movement(loc, adj));
                *n += 1;
            }
        }
    }

    // Jumping over contiguous linear lines of tiles.
    fn generate_jumps(&self, loc: Loc, board_moves: &mut [Option<Move>], n: &mut usize) {
        for &dir in adjacent(loc).iter() {
            if self.get(dir).is_some() {
                let dx = dir.0 - loc.0;
                let dy = dir.1 - loc.1;
                let mut x = dir.0 + dx;
                let mut y = dir.1 + dy;
                while self.get((x, y)).is_some() {
                    x += dx;
                    y += dy;
                }
                board_moves[*n] = Some(Move::Movement(loc, (x, y)));
                *n += 1;
            }
        }
    }

    fn generate_walk1(&self, loc: Loc, board_moves: &mut [Option<Move>], n: &mut usize) {
        // TODO
    }
    fn generate_walk3(&self, loc: Loc, board_moves: &mut [Option<Move>], n: &mut usize) {
        // TODO
    }
    fn generate_walk_all(&self, loc: Loc, board_moves: &mut [Option<Move>], n: &mut usize) {
        // TODO
    }

    fn generate_movements(&self, board_moves: &mut [Option<Move>], n: &mut usize) {
        for (&loc, tile) in self.grid.iter() {
            if tile.underneath.is_some() {
                self.generate_stack_walking(loc, board_moves, n);
            } else if !self.is_cut_vertex(loc) {
                match tile.bug {
                    Bug::Queen => self.generate_walk1(loc, board_moves, n),
                    Bug::Grasshopper => self.generate_jumps(loc, board_moves, n),
                    Bug::Spider => self.generate_walk3(loc, board_moves, n),
                    Bug::Ant => self.generate_walk_all(loc, board_moves, n),
                    Bug::Beetle => self.generate_walk1(loc, board_moves, n),
                }
            }
        }
    }
}

#[test]
fn test_cut_vertex() {
    let mut board = Board::default();
    //．．🐝🐝🐝🐝
    // ．．．🐝．🐝🐝
    //．．．．🐝🐝
    for &node in &[(0, 0), (0, 1), (1, 0), (2, 1), (1, 2), (2, 2), (-1, 0), (-2, 0), (3, 1)] {
        board.insert(node, Bug::Queen, Color::Black);
    }
    println!("{}", board);
    // Line 1
    assert!(board.is_cut_vertex((-1, 0)));
    assert!(!board.is_cut_vertex((-2, 0)));
    assert!(!board.is_cut_vertex((0, 0)));
    assert!(!board.is_cut_vertex((1, 0)));
    // Line 2
    assert!(!board.is_cut_vertex((0, 1)));
    assert!(board.is_cut_vertex((2, 1)));
    assert!(!board.is_cut_vertex((3, 1)));
    // Line 3
    assert!(!board.is_cut_vertex((1, 2)));
    assert!(!board.is_cut_vertex((2, 2)));
}

#[test]
fn test_slideable() {
    let mut board = Board::default();
    // One neighbor.
    board.insert((0, 0), Bug::Queen, Color::Black);
    board.insert((1, 0), Bug::Queen, Color::Black);
    assert_eq!([Some((0, -1)), Some((1, 1)), None, None], board.slideable_adjacent((0, 0)));
    // Two adjacent neighbors.
    board.insert((1, 1), Bug::Queen, Color::Black);
    assert_eq!([Some((0, -1)), Some((0, 1)), None, None], board.slideable_adjacent((0, 0)));
    // Four adjacent neighbors.
    board.insert((0, 1), Bug::Queen, Color::Black);
    board.insert((-1, 0), Bug::Queen, Color::Black);
    assert_eq!([Some((-1, -1)), Some((0, -1)), None, None], board.slideable_adjacent((0, 0)));
    // Five adjacent neighbors.
    board.insert((-1, -1), Bug::Queen, Color::Black);
    assert_eq!([None, None, None, None], board.slideable_adjacent((0, 0)));
    // 2 separated groups of neighbors.
    board.remove((0, 1));
    assert_eq!([None, None, None, None], board.slideable_adjacent((0, 0)));
    // 2 opposite single neighbors
    board.remove((1, 1));
    board.remove((-1, -1));
    assert_eq!(
        [Some((-1, -1)), Some((0, -1)), Some((1, 1)), Some((0, 1))],
        board.slideable_adjacent((0, 0))
    );
}

#[test]
fn test_generate_jumps() {
    let mut board = Board::default();
    for &node in &[(0, 0), (0, 1), (0, 3), (1, 0), (2, 0)] {
        board.insert(node, Bug::Grasshopper, Color::Black);
    }
    let mut moves = [None; 6];
    let mut n = 0;
    board.generate_jumps((0, 0), &mut moves, &mut n);
    assert_eq!(n, 2);
    moves[..2].sort();
    assert_eq!(moves[0], Some(Move::Movement((0, 0), (0, 2))));
    assert_eq!(moves[1], Some(Move::Movement((0, 0), (3, 0))));
}

pub struct Game;

impl minimax::Game for Game {
    type S = Board;
    type M = Move;

    fn generate_moves(
        board: &Board, _: minimax::Player, board_moves: &mut [Option<Move>],
    ) -> usize {
        let mut n = 0;

        // Special case for the first 2 moves:
        if board.move_num < 2 {
            for (bug, _) in board.get_available_bugs().iter() {
                board_moves[n] = Some(Move::Place((board.move_num as i8, 0), *bug));
                n += 1;
            }
            board_moves[n] = None;
            return n;
        }

        // Find placeable positions.
        board.generate_placements(board_moves, &mut n);

        if board.queen_required() {
            // No movement allowed.
            board_moves[n] = None;
            return n;
        }

        // For moveable pieces, generate all legal moves.
        board.generate_movements(board_moves, &mut n);

        board_moves[n] = None;
        n
    }

    fn get_winner(_: &Board) -> Option<minimax::Winner> {
        // TODO
        None
    }
}
