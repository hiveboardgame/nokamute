use crate::board::*;
use minimax;

// An evaluator that knows nothing but the rules, and maximally explores the tree.
pub struct DumbEvaluator;

impl minimax::Evaluator for DumbEvaluator {
    type G = Game;
    fn evaluate(_: &Board) -> minimax::Evaluation {
        minimax::Evaluation::Score(0)
    }
}

// An evaluator that counts movable pieces and how close to death the queen is.
pub struct BasicEvaluator;

impl minimax::Evaluator for BasicEvaluator {
    type G = Game;
    fn evaluate(board: &Board) -> minimax::Evaluation {
        const QUEEN_FACTOR: i64 = 20;
        const MOVABLE_BUG_FACTOR: i64 = 1;

        let queens_surrounded = board.queens_surrounded();
        let immovable = board.find_cut_vertexes();

        fn value(bug: Bug) -> i64 {
            // Mostly made up. All I know is that ants are good.
            match bug {
                Bug::Queen => 10,
                Bug::Ant => 7,
                Bug::Beetle => 6,
                Bug::Grasshopper => 4,
                Bug::Spider => 3,
                Bug::Mosquito => 0, // See below.
                Bug::Ladybug => 5,
                Bug::Pillbug => 4,
            }
        }

        let mut score: i64 = queens_surrounded[1 - board.to_move() as usize] as i64
            - queens_surrounded[board.to_move() as usize] as i64;
        score *= QUEEN_FACTOR;

        for (id, node) in (0..).zip(board.nodes.iter()) {
            if let Some(ref tile) = node.tile {
                let mut bug_score = value(tile.bug);
                if tile.bug == Bug::Pillbug
                    && node.adj.iter().any(|&adj| {
                        board
                            .get(adj)
                            .map(|tile2| tile2.bug == Bug::Queen && tile2.color == tile.color)
                            .unwrap_or(false)
                    })
                {
                    // Pillbugs get a bonus if adjacent to matching queen.
                    bug_score += 9;
                } else if tile.underneath.is_none() && immovable.get(id) {
                    continue;
                }
                if tile.bug == Bug::Mosquito {
                    // Mosquitos are valued as they can currently move.
                    if tile.underneath.is_some() {
                        bug_score = value(Bug::Beetle);
                    } else {
                        bug_score = node
                            .adj
                            .iter()
                            .map(|&id| board.get(id).map(|tile| value(tile.bug) % 9).unwrap_or(0))
                            .max()
                            .unwrap_or(0);
                    }
                }
                bug_score *= MOVABLE_BUG_FACTOR;
                if tile.color != board.to_move() {
                    bug_score = -bug_score;
                }
                score += bug_score;
            }
        }

        minimax::Evaluation::Score(score)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minimax() {
        use minimax::strategies::negamax::{Negamax, Options};
        use minimax::{Move, Strategy};

        // Find the winning move.
        // ．．．🐝🕷．．
        //．．🐜🐜🐝．．
        // ．．．🦗🪲
        let mut board = Board::default();
        crate::Move::Place(board.id((0, 0)), Bug::Queen).apply(&mut board);
        crate::Move::Place(board.id((1, 0)), Bug::Spider).apply(&mut board);
        crate::Move::Place(board.id((-1, 1)), Bug::Ant).apply(&mut board);
        crate::Move::Place(board.id((0, 1)), Bug::Ant).apply(&mut board);
        crate::Move::Place(board.id((1, 2)), Bug::Grasshopper).apply(&mut board);
        crate::Move::Place(board.id((1, 1)), Bug::Queen).apply(&mut board);
        crate::Move::Place(board.id((2, 2)), Bug::Beetle).apply(&mut board);
        crate::Move::Pass.apply(&mut board);
        println!("{}", board);
        for depth in 0..2 {
            let mut strategy = Negamax::<DumbEvaluator>::new(Options { max_depth: depth });
            let m = strategy.choose_move(&mut board);
            assert_eq!(Some(crate::Move::Movement(board.id((-1, 1)), board.id((2, 1)))), m);

            let mut strategy = Negamax::<BasicEvaluator>::new(Options { max_depth: depth });
            let m = strategy.choose_move(&mut board);
            assert_eq!(Some(crate::Move::Movement(board.id((-1, 1)), board.id((2, 1)))), m);
        }

        // Find queen escape.
        //．．🕷🐝🐝．
        // ．．🦗🕷．
        let mut board = Board::default();
        crate::Move::Place(board.id((0, 0)), Bug::Queen).apply(&mut board);
        crate::Move::Place(board.id((1, 0)), Bug::Queen).apply(&mut board);
        crate::Move::Place(board.id((1, 1)), Bug::Spider).apply(&mut board);
        crate::Move::Place(board.id((0, 1)), Bug::Grasshopper).apply(&mut board);
        crate::Move::Place(board.id((-1, 0)), Bug::Spider).apply(&mut board);
        crate::Move::Pass.apply(&mut board);
        println!("{}", board);
        for depth in 0..3 {
            let mut strategy = Negamax::<BasicEvaluator>::new(Options { max_depth: depth });
            let m = strategy.choose_move(&mut board);
            assert_eq!(Some(crate::Move::Movement(board.id((0, 0)), board.id((0, -1)))), m);
        }
    }
}
