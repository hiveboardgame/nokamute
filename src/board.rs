extern crate minimax;

use std::cmp::min;
use std::collections::HashMap;
use std::convert::TryInto;
use std::default::Default;
use std::fmt::{Display, Formatter, Result};

use crate::zobrist::ZOBRIST_TABLE;

// TODO AI shootout: https://jonthysell.com/2016/07/13/creating-an-ai-to-play-hive-with-mzinga-part-i/

// TODO benchmarks: placement heavy starting from empty board; movement-heavy starting from full board

// TODO minimax evaluator extensions:
// * Transposition table (hashmap of zobrist hash to evaluation? alpha&beta values?)
// * Iterative search (search to depth n, resort moves, search to depth n+1, until timeout)
// * Parallel search (makes the previous 2 harder)

// Board representation: Adjacency-list graph with grid backup.
//      Dynamically allocate used and empty adjacent hexes with indexes.
//      Compact adjacency list for each node. Generate new nodes when expanding.

// Hex coordinates. Grid connections plus one of the diagonals. First bug is at (0,0).
pub type Loc = (i8, i8);

// Persistent id of a location.
pub type Id = u8;

// Special value for nodes not adjacent to occupied tiles that haven't been
// allocated their own node yet.
const UNASSIGNED: Id = 0;

fn adjacent(loc: Loc) -> [Loc; 6] {
    let (x, y) = loc;
    // In clockwise order
    [(x - 1, y - 1), (x, y - 1), (x + 1, y), (x + 1, y + 1), (x, y + 1), (x - 1, y)]
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd)]
pub enum Bug {
    Queen = 0,
    Grasshopper = 1,
    Spider = 2,
    Ant = 3,
    Beetle = 4,
}

#[derive(Copy, Clone, Debug, PartialEq)]
enum Color {
    Black = 0,
    White = 1,
}

// A tile on the board.
#[derive(Clone)]
struct Tile {
    bug: Bug,
    color: Color,
    underneath: Option<Box<Tile>>,
}

impl Tile {
    fn height(&self) -> u32 {
        if let Some(next) = &self.underneath {
            1 + next.height()
        } else {
            0
        }
    }
}

#[derive(Clone)]
struct Node {
    // Adjacency list.
    adj: [Id; 6],
    tile: Option<Tile>,
}

#[derive(Clone)]
pub struct Board {
    // Indexed by Id.
    nodes: Vec<Node>,
    id_to_loc: Vec<Loc>,
    loc_to_id: HashMap<Loc, Id>,
    remaining: [[u8; 5]; 2],
    queens: [Id; 2],
    move_num: u16,
    zobrist_hash: u64,
    zobrist_history: Vec<u64>,
}

fn zobrist(id: Id, bug: Bug, color: Color, height: u32) -> u64 {
    // Put the id in the high bits, to keep cache locality for the likely unused high ids.
    let hash = ZOBRIST_TABLE[(id as usize) << 4 | (bug as usize) << 1 | (color as usize)];
    // I don't really want to multiply the table by another factor of 7, so
    // just realign the existing random bits.
    hash.rotate_left(height)
}

impl Board {
    fn to_move(&self) -> Color {
        if self.move_num % 2 == 0 {
            Color::Black
        } else {
            Color::White
        }
    }

    pub fn loc(&self, id: Id) -> Loc {
        self.id_to_loc[id as usize]
    }

    pub fn id(&mut self, loc: Loc) -> Id {
        self.alloc(loc)
    }

    // Allocate a new node, and link it to its neighbors.
    fn alloc(&mut self, loc: Loc) -> Id {
        if let Some(id) = self.loc_to_id.get(&loc) {
            return *id;
        }
        let new_id: Id = self.nodes.len().try_into().unwrap();
        self.loc_to_id.insert(loc, new_id);
        self.id_to_loc.push(loc);
        let mut node = Node { tile: None, adj: [UNASSIGNED; 6] };
        // Link existing adjacent nodes in both directions.
        for (i, adj) in (0..6).zip(adjacent(loc).iter()) {
            if let Some(id) = self.loc_to_id.get(adj) {
                node.adj[i] = *id;
                debug_assert_eq!(self.nodes[*id as usize].adj[(i + 3) % 6], UNASSIGNED);
                self.nodes[*id as usize].adj[(i + 3) % 6] = new_id;
            }
        }
        self.nodes.push(node);
        new_id
    }

    // For tiles getting placed, ensure all tiles around them are allocated.
    // This ensures empty tiles know all tiles that surround them, even if
    // they don't touch each other for placement.
    fn alloc_surrounding(&mut self, id: Id) {
        for (i, &loc) in (0..6).zip(adjacent(self.loc(id)).iter()) {
            if self.adjacent(id)[i] == UNASSIGNED {
                self.alloc(loc);
            }
        }
    }

    fn get(&self, id: Id) -> Option<&Tile> {
        self.nodes[id as usize].tile.as_ref()
    }

    fn insert(&mut self, id: Id, bug: Bug, color: Color) {
        let underneath = if let Some(prev) = self.nodes[id as usize].tile.take() {
            Some(Box::new(prev))
        } else {
            // Potentially newly occupied node. Ensure all surrounding nodes get allocated.
            self.alloc_surrounding(id);
            None
        };
        let tile = Tile { bug: bug, color: color, underneath: underneath };
        self.zobrist_hash ^= zobrist(id, bug, color, tile.height());
        self.nodes[id as usize].tile = Some(tile);

        if bug == Bug::Queen {
            self.queens[self.move_num as usize & 1] = id;
        }
    }

    // Asserts that there is something there.
    fn remove(&mut self, id: Id) -> Tile {
        let mut tile = self.nodes[id as usize].tile.take().unwrap();
        self.zobrist_hash ^= zobrist(id, tile.bug, tile.color, tile.height());
        if let Some(stack) = tile.underneath.take() {
            self.nodes[id as usize].tile = Some(*stack);
        }
        tile
    }

    fn adjacent(&self, id: Id) -> &[Id; 6] {
        &self.nodes[id as usize].adj
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

    fn queens_surrounded(&self) -> [usize; 2] {
        let mut out = [0; 2];
        for i in 0..2 {
            out[i] = self
                .adjacent(self.queens[i])
                .iter()
                .filter(|adj| self.get(**adj).is_some())
                .count();
        }
        out
    }
}

impl Default for Board {
    fn default() -> Self {
        // Pre-allocate dummy unassigned Id to unused location.
        let fake_loc = (i8::MAX, i8::MAX);
        let mut loc_to_id = HashMap::new();
        loc_to_id.insert(fake_loc, 0);
        let mut board = Board {
            nodes: vec![Node { adj: [UNASSIGNED; 6], tile: None }],
            id_to_loc: vec![fake_loc],
            loc_to_id: loc_to_id,
            remaining: [[1, 3, 2, 3, 2], [1, 3, 2, 3, 2]],
            queens: [UNASSIGNED; 2],
            move_num: 0,
            zobrist_hash: 0,
            zobrist_history: Vec::new(),
        };
        // Pre-allocate starting moves.
        board.alloc((0, 0));
        board.alloc((1, 0));
        board
    }
}

impl Display for Board {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(f, "{}", self.fancy_fmt())
    }
}

impl Board {
    fn bounding_box(&self) -> (i8, i8, i8, i8) {
        if self.nodes.len() == 3 {
            return (0, 1, 0, 1);
        }
        let mut minx = i8::MAX;
        let mut maxx = i8::MIN;
        let mut miny = i8::MAX;
        let mut maxy = i8::MIN;
        for (id, loc) in (0..).zip(self.id_to_loc.iter()) {
            if self.get(id).is_some() {
                minx = std::cmp::min(minx, loc.0);
                maxx = std::cmp::max(maxx, loc.0);
                miny = std::cmp::min(miny, loc.1);
                maxy = std::cmp::max(maxy, loc.1);
            }
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
                if let Some(tile) = self.get(*self.loc_to_id.get(&(x, y)).unwrap_or(&UNASSIGNED)) {
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
    Place(Id, Bug),
    Movement(Id, Id),
    Pass,
}

impl minimax::Move for Move {
    type G = Game;
    fn apply(&self, board: &mut Board) {
        match *self {
            Move::Place(id, bug) => {
                board.insert(id, bug, board.to_move());
                board.mut_remaining()[bug as usize] -= 1;
            }
            Move::Movement(start, end) => {
                let tile = board.remove(start);
                board.insert(end, tile.bug, tile.color);
            }
            Move::Pass => {}
        }
        board.move_num += 1;
        board.zobrist_history.push(board.zobrist_hash);
    }
    fn undo(&self, board: &mut Board) {
        board.move_num -= 1;
        board.zobrist_history.pop();
        match *self {
            Move::Place(id, bug) => {
                board.remove(id);
                board.mut_remaining()[bug as usize] += 1;
            }
            Move::Movement(start, end) => {
                let tile = board.remove(end);
                board.insert(start, tile.bug, tile.color);
            }
            Move::Pass => {}
        }
    }
}

// Useful utility.
struct NodeSet {
    table: [bool; 256],
}

impl NodeSet {
    fn new() -> NodeSet {
        NodeSet { table: [false; 256] }
    }

    fn set(&mut self, id: Id) {
        self.table[id as usize] = true;
    }

    fn get(&self, id: Id) -> bool {
        self.table[id as usize]
    }
}

impl Board {
    fn generate_placements(&self, moves: &mut [Option<Move>], n: &mut usize) {
        // Use empty spaces that have no opposite colored tiles adjacent.
        for (id, node) in (0..).zip(self.nodes.iter()).skip(1) {
            if node.tile.is_some() {
                continue;
            }
            let mut num_buddies = 0;
            let mut num_enemies = 0;
            for adj in node.adj.iter() {
                if let Some(tile) = self.get(*adj) {
                    if tile.color == self.to_move() {
                        num_buddies += 1;
                    } else {
                        num_enemies += 1;
                    }
                }
            }
            if num_buddies > 0 && num_enemies == 0 {
                for (bug, num_left) in self.get_available_bugs().iter() {
                    if self.queen_required() && *bug != Bug::Queen {
                        continue;
                    }
                    if *num_left > 0 {
                        moves[*n] = Some(Move::Place(id, *bug));
                        *n += 1;
                    }
                }
            }
        }
    }

    // Linear algorithm to find all cut vertexes.
    // Algorithm explanation: https://web.archive.org/web/20180830110222/https://www.eecs.wsu.edu/~holder/courses/CptS223/spr08/slides/graphapps.pdf
    // Example code: https://cp-algorithms.com/graph/cutpoints.html
    //
    // TODO: cache movability for each tile, and somehow iteratively update it
    // Need to persist the DFS tree from an arbitrary root.
    // Adding a tile just adds a leaf to one of its neighbors
    // Removing a tile means recomputing a path to the root for any children of the removed node.
    // Hmm, maybe not. DFS iteration order is important.
    fn find_cut_vertexes(&self) -> NodeSet {
        struct State<'a> {
            board: &'a Board,
            visited: NodeSet,
            immovable: NodeSet,
            // Visitation number in DFS traversal.
            num: [u8; 256],
            // Lowest-numbered node reachable using DFS edges and then at most
            // one back edge.
            low: [u8; 256],
            visit_num: u8,
        }
        let mut state = State {
            board: self,
            visited: NodeSet::new(),
            immovable: NodeSet::new(),
            num: [0; 256],
            low: [0; 256],
            visit_num: 1,
        };
        fn dfs(state: &mut State, id: Id, parent: Id) {
            state.visited.set(id);
            state.num[id as usize] = state.visit_num;
            state.low[id as usize] = state.visit_num;
            state.visit_num += 1;
            let mut children = 0;
            for &adj in state.board.adjacent(id) {
                if state.board.get(adj).is_none() {
                    continue;
                }
                if adj == parent {
                    continue;
                }
                if state.visited.get(adj) {
                    state.low[id as usize] = min(state.low[id as usize], state.num[adj as usize]);
                } else {
                    dfs(state, adj, id);
                    state.low[id as usize] = min(state.low[id as usize], state.low[adj as usize]);
                    if state.low[adj as usize] >= state.num[id as usize] && parent != UNASSIGNED {
                        state.immovable.set(id);
                    }
                    children += 1;
                }
            }
            if parent == UNASSIGNED && children > 1 {
                state.immovable.set(id);
            }
        }

        let start: Id =
            (0..).zip(self.nodes.iter()).filter(|(_, x)| x.tile.is_some()).next().unwrap().0;
        dfs(&mut state, start, UNASSIGNED);
        state.immovable
    }

    // For a position on the outside (whether occupied or not), find all
    // adjacent locations still connected to the hive that are slideable.
    // A slideable position has 2 empty slots next to an occupied slot.
    // For all 2^6 possibilities, there can be 0, 2, or 4 slideable neighbors.
    fn slideable_adjacent(&self, origin: Id, id: Id) -> [Option<Id>; 4] {
        let mut out = [None; 4];
        let mut n = 0;
        let neighbors = self.adjacent(id);
        // Each bit is whether neighbor is occupied.
        let mut occupied = 0;
        for neighbor in neighbors.iter().rev() {
            occupied <<= 1;
            // Since the origin bug is moving, we can't crawl around it.
            if self.get(*neighbor).is_some() && *neighbor != origin {
                occupied |= 1;
            }
        }
        // Wrap around in each direction
        occupied |= occupied << 6;
        occupied = (occupied << 1) | (occupied >> 5) & 1;
        let mut slideable = !occupied & (occupied << 1 ^ occupied >> 1);

        for neighbor in neighbors.iter() {
            slideable >>= 1;
            if slideable & 1 != 0 {
                out[n] = Some(*neighbor);
                n += 1;
            }
        }

        out
    }

    // From any bug on top of a stack. Walk or jump down in any direction.
    fn generate_stack_walking(&self, id: Id, moves: &mut [Option<Move>], n: &mut usize) {
        for &adj in self.adjacent(id).iter() {
            moves[*n] = Some(Move::Movement(id, adj));
            *n += 1;
        }
    }

    // Jumping over contiguous linear lines of tiles.
    fn generate_jumps(&self, id: Id, moves: &mut [Option<Move>], n: &mut usize) {
        for dir in 0..6 {
            let mut jump = id;
            let mut dist = 0;
            while self.get(jump).is_some() {
                jump = self.adjacent(jump)[dir];
                dist += 1;
            }
            if dist > 1 {
                moves[*n] = Some(Move::Movement(id, jump));
                *n += 1;
            }
        }
    }

    fn generate_walk_up(&self, id: Id, moves: &mut [Option<Move>], n: &mut usize) {
        for &adj in self.adjacent(id).iter() {
            if self.get(adj).is_some() {
                moves[*n] = Some(Move::Movement(id, adj));
                *n += 1;
            }
        }
    }

    fn generate_walk1(&self, id: Id, moves: &mut [Option<Move>], n: &mut usize) {
        for adj in self.slideable_adjacent(id, id).iter() {
            if let &Some(node) = adj {
                moves[*n] = Some(Move::Movement(id, node));
                *n += 1;
            }
        }
    }

    fn generate_walk3(&self, orig: Id, moves: &mut [Option<Move>], n: &mut usize) {
        fn dfs(
            id: Id, orig: Id, board: &Board, path: &mut Vec<Id>, moves: &mut [Option<Move>],
            n: &mut usize,
        ) {
            if path.contains(&id) {
                return;
            }
            if path.len() == 3 {
                moves[*n] = Some(Move::Movement(orig, id));
                *n += 1;
                return;
            }
            path.push(id);
            for adj in board.slideable_adjacent(orig, id).iter() {
                if let Some(node) = *adj {
                    dfs(node, orig, board, path, moves, n);
                }
            }
            path.pop();
        }
        let mut path = Vec::with_capacity(3);
        dfs(orig, orig, self, &mut path, moves, n);
    }

    fn generate_walk_all(&self, orig: Id, moves: &mut [Option<Move>], n: &mut usize) {
        let mut visited = NodeSet::new();
        let mut queue = vec![orig];
        while let Some(node) = queue.pop() {
            if visited.get(node) {
                continue;
            }
            visited.set(node);
            if node != orig {
                moves[*n] = Some(Move::Movement(orig, node));
                *n += 1;
            }
            for adj in self.slideable_adjacent(orig, node).iter() {
                if let Some(next) = adj {
                    queue.push(*next);
                }
            }
        }
    }

    fn generate_movements(&self, moves: &mut [Option<Move>], n: &mut usize) {
        let immovable = self.find_cut_vertexes();
        for (id, node) in (0..).zip(self.nodes.iter()).skip(1) {
            if let Some(tile) = &node.tile {
                if tile.color != self.to_move() {
                    continue;
                }
                if tile.underneath.is_some() {
                    self.generate_stack_walking(id, moves, n);
                } else if !immovable.get(id) {
                    match tile.bug {
                        Bug::Queen => self.generate_walk1(id, moves, n),
                        Bug::Grasshopper => self.generate_jumps(id, moves, n),
                        Bug::Spider => self.generate_walk3(id, moves, n),
                        Bug::Ant => self.generate_walk_all(id, moves, n),
                        Bug::Beetle => {
                            self.generate_walk1(id, moves, n);
                            self.generate_walk_up(id, moves, n);
                        }
                    }
                }
            }
        }
    }
}

pub struct Game;

impl minimax::Game for Game {
    type S = Board;
    type M = Move;

    fn generate_moves(board: &Board, _: minimax::Player, moves: &mut [Option<Move>]) -> usize {
        let mut n = 0;

        if board.move_num < 2 {
            // Special case for the first 2 moves:
            for (bug, _) in board.get_available_bugs().iter() {
                moves[n] = Some(Move::Place((board.move_num + 1) as Id, *bug));
                n += 1;
            }
        } else {
            // Find placeable positions.
            board.generate_placements(moves, &mut n);

            if !board.queen_required() {
                // For movable pieces, generate all legal moves.
                board.generate_movements(moves, &mut n);
            }
        }

        if n == 0 {
            moves[n] = Some(Move::Pass);
            n += 1;
        }

        moves[n] = None;
        n
    }

    fn get_winner(board: &Board) -> Option<minimax::Winner> {
        let queens_surrounded = board.queens_surrounded();
        let n = board.zobrist_history.len();
        if n > 5 && board.zobrist_history[n - 5] == board.zobrist_hash {
            // Draw by stalemate.
            Some(minimax::Winner::Draw)
        } else if queens_surrounded == [6, 6] {
            // Draw by simultaneous queen surrounding.
            Some(minimax::Winner::Draw)
        } else if queens_surrounded[board.move_num as usize & 1] == 6 {
            Some(minimax::Winner::Competitor(minimax::Player::Computer))
        } else if queens_surrounded[(board.move_num + 1) as usize & 1] == 6 {
            Some(minimax::Winner::Competitor(minimax::Player::Opponent))
        } else {
            None
        }
    }
}

// An evaluator that knows nothing but the rules, and maximally explores the tree.
pub struct DumbEvaluator;

impl minimax::Evaluator for DumbEvaluator {
    type G = Game;
    fn evaluate(_: &Board, mw: Option<minimax::Winner>) -> minimax::Evaluation {
        match mw {
            Some(minimax::Winner::Competitor(wp)) => match wp {
                minimax::Player::Computer => minimax::Evaluation::Best,
                minimax::Player::Opponent => minimax::Evaluation::Worst,
            },
            _ => minimax::Evaluation::Score(0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Ordering;

    const ORIGIN: Id = 1;

    impl Board {
        fn insert_loc(&mut self, loc: Loc, bug: Bug, color: Color) {
            let id = self.alloc(loc);
            self.insert(id, bug, color);
        }

        fn remove_loc(&mut self, loc: Loc) -> Tile {
            let id = self.alloc(loc);
            self.remove(id)
        }

        fn fill_board(&mut self, locs: &[Loc], bug: Bug) {
            for &loc in locs {
                let id = self.alloc(loc);
                self.insert(id, bug, Color::Black);
            }
        }

        fn assert_placements(&self, moves: &[Option<Move>], expected: &[(Loc, Bug)]) {
            let mut actual_pairs = Vec::new();
            for m in moves.iter() {
                if let Some(Move::Place(actual_id, actual_bug)) = m {
                    actual_pairs.push((self.loc(*actual_id), *actual_bug));
                }
            }
            actual_pairs.sort();
            let mut expected_pairs = Vec::new();
            expected_pairs.extend(expected);
            expected_pairs.sort();
            assert_eq!(actual_pairs, expected_pairs);
        }

        fn assert_movements(&self, moves: &[Option<Move>], start: Loc, ends: &[Loc]) {
            let mut actual_ends = Vec::new();
            for m in moves.iter() {
                if let Some(Move::Movement(actual_start, actual_end)) = m {
                    assert_eq!(self.loc(*actual_start), start);
                    actual_ends.push(self.loc(*actual_end));
                }
            }
            actual_ends.sort();
            let mut expected_ends = Vec::new();
            expected_ends.extend(ends);
            expected_ends.sort();
            assert_eq!(actual_ends, expected_ends);
        }
    }

    impl Ord for Bug {
        fn cmp(&self, other: &Self) -> Ordering {
            (*self as u8).cmp(&(*other as u8))
        }
    }

    impl Ord for Move {
        fn cmp(&self, other: &Self) -> Ordering {
            match *self {
                Move::Place(loc, bug) => {
                    if let Move::Place(loc2, bug2) = other {
                        (loc, bug as u8).cmp(&(*loc2, *bug2 as u8))
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
                Move::Pass => Ordering::Less,
            }
        }
    }

    impl PartialOrd for Move {
        fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
            Some(self.cmp(other))
        }
    }

    #[test]
    fn test_gen_placement() {
        let mut board = Board::default();
        for i in 1..5 {
            board.remaining[0][i] = 0;
            board.remaining[1][i] = 0;
        }
        board.insert(1, Bug::Queen, Color::Black);
        board.insert(2, Bug::Queen, Color::White);
        println!("{}", board);
        let mut moves = [None; 100];
        let mut n = 0;
        board.generate_placements(&mut moves, &mut n);
        board.assert_placements(
            &moves[..n],
            &[((-1, -1), Bug::Queen), ((-1, 0), Bug::Queen), ((0, 1), Bug::Queen)],
        );
    }

    #[test]
    fn test_cut_vertex() {
        let mut board = Board::default();
        //．．🐝🐝🐝🐝
        // ．．．🐝．🐝🐝
        //．．．．🐝🐝
        board.fill_board(
            &[(0, 0), (0, 1), (1, 0), (2, 1), (1, 2), (2, 2), (-1, 0), (-2, 0), (3, 1)],
            Bug::Queen,
        );
        println!("{}", board);
        let cuts = board.find_cut_vertexes();
        let mut is_cut_loc = |loc: Loc| {
            let id = board.id(loc);
            cuts.get(id)
        };
        // Line 1
        assert!(is_cut_loc((-1, 0)));
        assert!(!is_cut_loc((-2, 0)));
        assert!(!is_cut_loc((0, 0)));
        assert!(!is_cut_loc((1, 0)));
        // Line 2
        assert!(!is_cut_loc((0, 1)));
        assert!(is_cut_loc((2, 1)));
        assert!(!is_cut_loc((3, 1)));
        // Line 3
        assert!(!is_cut_loc((1, 2)));
        assert!(!is_cut_loc((2, 2)));
    }

    #[test]
    fn test_slideable() {
        let mut board = Board::default();
        let x = board.alloc((0, 0));
        // One neighbor.
        board.insert_loc((0, 0), Bug::Queen, Color::Black);
        board.insert_loc((1, 0), Bug::Queen, Color::Black);
        assert_eq!(
            [Some(board.alloc((0, -1))), Some(board.alloc((1, 1))), None, None],
            board.slideable_adjacent(x, x)
        );
        // Two adjacent neighbors.
        board.insert_loc((1, 1), Bug::Queen, Color::Black);
        assert_eq!(
            [Some(board.alloc((0, -1))), Some(board.alloc((0, 1))), None, None],
            board.slideable_adjacent(x, x)
        );
        // Four adjacent neighbors.
        board.insert_loc((0, 1), Bug::Queen, Color::Black);
        board.insert_loc((-1, 0), Bug::Queen, Color::Black);
        assert_eq!(
            [Some(board.alloc((-1, -1))), Some(board.alloc((0, -1))), None, None],
            board.slideable_adjacent(x, x)
        );
        // Five adjacent neighbors.
        board.insert_loc((-1, -1), Bug::Queen, Color::Black);
        assert_eq!([None, None, None, None], board.slideable_adjacent(x, x));
        // 2 separated groups of neighbors.
        board.remove_loc((0, 1));
        assert_eq!([None, None, None, None], board.slideable_adjacent(x, x));
        // 2 opposite single neighbors
        board.remove_loc((1, 1));
        board.remove_loc((-1, -1));
        assert_eq!(
            [
                Some(board.alloc((-1, -1))),
                Some(board.alloc((0, -1))),
                Some(board.alloc((1, 1))),
                Some(board.alloc((0, 1)))
            ],
            board.slideable_adjacent(x, x)
        );
    }

    #[test]
    fn test_generate_jumps() {
        let mut board = Board::default();
        //．．．🦗🦗🦗．
        // ．．🦗．．．
        //．．．．．．
        // ．🦗．．
        board.fill_board(&[(0, 0), (0, 1), (0, 3), (1, 0), (2, 0)], Bug::Grasshopper);
        println!("{}", board);
        let mut moves = [None; 6];
        let mut n = 0;
        board.generate_jumps(ORIGIN, &mut moves, &mut n);
        board.assert_movements(&moves[..n], (0, 0), &[(0, 2), (3, 0)]);
    }

    #[test]
    fn test_generate_beetle() {
        let mut board = Board::default();
        board.fill_board(&[(0, 0), (1, 1)], Bug::Beetle);
        println!("{}", board);
        let mut moves = [None; 6];
        let mut n = 0;
        board.generate_walk1(ORIGIN, &mut moves, &mut n);
        board.generate_walk_up(ORIGIN, &mut moves, &mut n);
        board.assert_movements(&moves[..n], (0, 0), &[(0, 1), (1, 0), (1, 1)]);
    }

    #[test]
    fn test_generate_walk3() {
        let mut board = Board::default();
        //．．．🕷．．．．．
        // ．．．🕷．🕷．．
        //．．．🕷．．🕷．
        // ．．．🕷🕷🕷
        board.fill_board(
            &[(-1, -1), (0, 0), (2, 0), (0, 1), (3, 1), (1, 2), (2, 2), (3, 2)],
            Bug::Spider,
        );
        println!("{}", board);
        let mut moves = [None; 6];
        let mut n = 0;
        let start = board.alloc((-1, -1));
        board.generate_walk3(start, &mut moves, &mut n);
        board.assert_movements(&moves[..n], (-1, -1), &[(0, 2), (1, -1), (1, 1), (2, 1)]);

        // ．．🕷．🕷．．
        //．．🕷🕷．🕷．
        // ．．🕷🕷🕷
        board.remove_loc((-1, -1));
        board.insert_loc((1, 1), Bug::Spider, Color::Black);
        println!("{}", board);
        moves = [None; 6];
        n = 0;
        let start = board.alloc((1, 1));
        board.generate_walk3(start, &mut moves, &mut n);
        board.assert_movements(&moves[..n], (1, 1), &[(-1, -1), (0, -1), (1, -1), (2, -1)]);
    }

    #[test]
    fn test_generate_walk_all() {
        let mut board = Board::default();
        //．．．🐜．．．．
        // ．．．🐜．．．
        //．．．🐜．🐜．
        // ．．．🐜🐜
        board.fill_board(&[(-1, -1), (0, 0), (0, 1), (2, 1), (1, 2), (2, 2)], Bug::Ant);
        println!("{}", board);
        let mut moves = [None; 20];
        let mut n = 0;
        let start = board.alloc((-1, -1));
        board.generate_walk_all(start, &mut moves, &mut n);
        board.assert_movements(
            &moves[..n],
            (-1, -1),
            &[
                (0, -1),
                (-1, 0),
                (1, 0),
                (2, 0),
                (-1, 1),
                (3, 1),
                (0, 2),
                (3, 2),
                (1, 3),
                (2, 3),
                (3, 3),
            ],
        );
    }

    #[test]
    fn test_winner() {
        use minimax::{Game, Move};

        // Draw by stalemate
        let mut board = Board::default();
        let x1 = board.alloc((-1, -1));
        let x2 = board.alloc((-1, 0));
        let y1 = board.alloc((1, 1));
        let y2 = board.alloc((1, 0));
        crate::Move::Place(ORIGIN, Bug::Spider).apply(&mut board);
        assert_eq!(None, self::Game::get_winner(&board));
        crate::Move::Place(x1, Bug::Queen).apply(&mut board);
        assert_eq!(None, self::Game::get_winner(&board));
        crate::Move::Place(y1, Bug::Queen).apply(&mut board);
        assert_eq!(None, self::Game::get_winner(&board));
        crate::Move::Movement(x1, x2).apply(&mut board);
        assert_eq!(None, self::Game::get_winner(&board));
        crate::Move::Movement(y1, y2).apply(&mut board);
        assert_eq!(None, self::Game::get_winner(&board));
        crate::Move::Movement(x2, x1).apply(&mut board);
        assert_eq!(None, self::Game::get_winner(&board));
        crate::Move::Movement(y2, y1).apply(&mut board);
        // This is the first repeat of a board position, a slightly aggressive
        // interpretation of chess stalemate rules.
        assert_eq!(Some(minimax::Winner::Draw), self::Game::get_winner(&board));
        // Undo reverts zobrist and history.
        crate::Move::Movement(y2, y1).undo(&mut board);
        assert_eq!(None, self::Game::get_winner(&board));
    }

    #[test]
    fn test_minimax() {
        use minimax::strategies::negamax::{Negamax, Options};
        use minimax::{Move, Strategy};

        // Find the winning move.
        // ．．．🐝🕷．．
        //．．🐜🐜🐝．．
        // ．．．🦗🪲
        let mut board = Board::default();
        crate::Move::Place(board.alloc((0, 0)), Bug::Queen).apply(&mut board);
        crate::Move::Place(board.alloc((1, 0)), Bug::Spider).apply(&mut board);
        crate::Move::Place(board.alloc((-1, 1)), Bug::Ant).apply(&mut board);
        crate::Move::Place(board.alloc((0, 1)), Bug::Ant).apply(&mut board);
        crate::Move::Place(board.alloc((1, 2)), Bug::Grasshopper).apply(&mut board);
        crate::Move::Place(board.alloc((1, 1)), Bug::Queen).apply(&mut board);
        crate::Move::Place(board.alloc((2, 2)), Bug::Beetle).apply(&mut board);
        crate::Move::Pass.apply(&mut board);
        println!("{}", board);
        let mut strategy = Negamax::<DumbEvaluator>::new(Options { max_depth: 1 });
        let player = minimax::Player::Computer;
        let m = strategy.choose_move(&mut board, player);
        board.assert_movements(&[m], (-1, 1), &[(2, 1)]);

        // TODO: Switch colors.
    }
}
