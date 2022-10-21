extern crate minimax;

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
use crate::cli::CliPlayer;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
use crate::uhp_client::UhpPlayer;
use crate::{BasicEvaluator, Board, Rules, Turn};
use minimax::*;
use std::time::Duration;

// A player that can play one color's moves.
pub(crate) trait Player {
    fn name(&self) -> String;
    fn new_game(&mut self, game_type: &str);
    fn play_move(&mut self, m: Turn);
    fn undo_move(&mut self, m: Turn);
    fn generate_move(&mut self) -> Turn;
    fn principal_variation(&self) -> Vec<Turn> {
        Vec::new()
    }
    fn set_max_depth(&mut self, _depth: u8) {}
    fn set_timeout(&mut self, _time: Duration) {}
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn face_off(
    game_type: &str, mut player1: Box<dyn Player>, mut player2: Box<dyn Player>,
) -> Option<String> {
    let mut b = Board::from_game_type(game_type).unwrap();
    player1.new_game(game_type);
    player2.new_game(game_type);
    let mut players = [player1, player2];
    let mut p = 0;
    loop {
        b.println();
        println!("{} ({:?}) to move", players[p].name(), b.to_move());
        let m = players[p].generate_move();
        let mut moves = Vec::new();
        Rules::generate_moves(&b, &mut moves);
        if !moves.contains(&m) {
            println!("{} played an illegal move: {}", players[p].name(), b.to_move_string(m));
            println!("Game log: {}", b.game_log());
            return Some(players[1 - p].name());
        }
        m.apply(&mut b);
        if let Some(winner) = Rules::get_winner(&b) {
            b.println();
            println!("Game log: {}", b.game_log());
            return match winner {
                minimax::Winner::Draw => None,
                minimax::Winner::PlayerJustMoved => Some(players[p].name()),
                minimax::Winner::PlayerToMove => Some(players[1 - p].name()),
            };
        }
        players[p].play_move(m);
        p = 1 - p;
        players[p].play_move(m);
    }
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn get_player(name: &str, config: &PlayerConfig) -> Box<dyn Player> {
    match name {
        "nokamute" => config.new_player(),
        "ai" => config.new_player(),
        "human" => Box::new(CliPlayer::new()),
        // Try to launch this as a UHP server
        _ => Box::new(UhpPlayer::new(name).unwrap()),
    }
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub fn play_game(
    config: PlayerConfig, game_type: &str, name1: &str, name2: &str, depth: Option<u8>,
    timeout: Option<String>,
) {
    let mut player1 = get_player(name1, &config);
    let mut player2 = get_player(name2, &config);
    if let Some(depth) = depth {
        player1.set_max_depth(depth);
        player2.set_max_depth(depth);
    } else if let Some(input) = timeout {
        let timeout = if input.ends_with('m') {
            input[..input.len() - 1].parse::<u64>().map(Duration::from_secs)
        } else if input.ends_with('m') {
            input[..input.len() - 1].parse::<u64>().map(|m| Duration::from_secs(m * 60))
        } else {
            exit("Could not parse --timeout (add units)".to_string());
        }
        .unwrap_or_else(|_| exit("Could not parse --timeout (add units)".to_string()));
        player1.set_timeout(timeout);
        player2.set_timeout(timeout);
    }
    match face_off(game_type, player1, player2) {
        None => println!("Game over: draw."),
        Some(name) => println!("Game over: {} won.", name),
    }
}

struct NokamutePlayer {
    board: Board,
    strategy: Box<dyn Strategy<Rules>>,
    name: String,
}

impl NokamutePlayer {
    fn new(strategy: Box<dyn Strategy<Rules>>) -> Self {
        Self::new_with_name("nokamute", strategy)
    }

    fn new_with_name(name: &str, mut strategy: Box<dyn Strategy<Rules>>) -> Self {
        strategy.set_timeout(Duration::from_secs(5));
        NokamutePlayer { board: Board::default(), strategy, name: name.to_owned() }
    }
}

impl Player for NokamutePlayer {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn new_game(&mut self, game_string: &str) {
        self.board = Board::from_game_string(game_string).unwrap();
    }

    fn play_move(&mut self, m: Turn) {
        m.apply(&mut self.board);
    }

    fn undo_move(&mut self, m: Turn) {
        m.undo(&mut self.board);
    }

    fn generate_move(&mut self) -> Turn {
        self.strategy.choose_move(&self.board).unwrap()
    }

    fn principal_variation(&self) -> Vec<Turn> {
        self.strategy.principal_variation()
    }

    fn set_max_depth(&mut self, depth: u8) {
        self.strategy.set_max_depth(depth);
    }

    fn set_timeout(&mut self, time: Duration) {
        self.strategy.set_timeout(time);
    }
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn exit(msg: String) -> ! {
    eprintln!("{}", msg);
    std::process::exit(1)
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub(crate) enum PlayerStrategy {
    Iterative(YbwOptions),
    LazySmp(LazySmpOptions),
    Random,
    Mcts(MCTSOptions),
}

pub struct PlayerConfig {
    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    pub(crate) num_threads: Option<usize>,
    pub(crate) opts: IterativeOptions,
    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    pub(crate) strategy: PlayerStrategy,
    pub(crate) eval: BasicEvaluator,
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub fn configure_player() -> Result<(PlayerConfig, Vec<String>), pico_args::Error> {
    let mut args = pico_args::Arguments::from_env();

    let mut config = PlayerConfig::new();

    // Configure common minimax options.
    if args.contains(["-v", "--verbose"]) {
        config.opts = config.opts.verbose();
    }
    let table_size: Option<usize> = args.opt_value_from_str("--table_mb")?;
    config.opts =
        config.opts.with_table_byte_size(table_size.unwrap_or(100).checked_shl(20).unwrap());
    let window_arg: Option<u32> = args.opt_value_from_str("--aspiration-window")?;
    if let Some(window) = window_arg {
        config.opts = config.opts.with_aspiration_window(window as minimax::Evaluation);
    }
    if args.contains("--double-step") {
        config.opts = config.opts.with_double_step_increment();
    }
    if args.contains("--null-move-pruning") {
        config.opts = config.opts.with_null_move_depth(3);
    }
    if args.contains("--quiet-search") {
        config.opts = config.opts.with_quiescence_search_depth(2);
    }

    // 0 for num_cpu threads; >0 for specific count.
    config.num_threads = args.opt_value_from_str("--num-threads")?.map(|thread_arg: String| {
        if thread_arg == "max" || thread_arg == "all" {
            0
        } else if let Ok(num) = thread_arg.parse::<usize>() {
            num
        } else {
            exit(format!("Could not parse num_threads={}. Expected int or 'max'", thread_arg));
        }
    });

    // Configure specific strategy.
    let strategy: Option<String> = args.opt_value_from_str("--strategy")?;
    config.strategy = match strategy.as_deref().unwrap_or("iterative") {
        "random" => PlayerStrategy::Random,
        "mcts" => PlayerStrategy::Mcts(MCTSOptions::default().with_max_rollout_depth(200)),
        "mtdf" => {
            config.opts = config.opts.with_mtdf();
            config.num_threads = Some(1);
            PlayerStrategy::Iterative(YbwOptions::new())
        }
        "iterative" | "ybw" => {
            let mut ybw_opts = YbwOptions::new();
            if args.contains("--background-ponder") {
                ybw_opts = ybw_opts.with_background_pondering();
            }
            PlayerStrategy::Iterative(ybw_opts)
        }
        "lazysmp" => {
            let mut smp_opts = LazySmpOptions::new();
            if args.contains("--differing-depths") {
                smp_opts = smp_opts.with_differing_depths();
            }
            PlayerStrategy::LazySmp(smp_opts)
        }
        _ => exit(format!("Unrecognized strategy: {}", strategy.unwrap_or_default())),
    };
    Ok((config, args.finish().into_iter().map(|s| s.into_string().unwrap()).collect::<Vec<_>>()))
}

impl Default for PlayerConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl PlayerConfig {
    pub fn new() -> Self {
        Self {
            #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
            num_threads: None,
            opts: IterativeOptions::new().with_countermoves(),
            #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
            strategy: PlayerStrategy::Iterative(YbwOptions::new()),
            eval: BasicEvaluator::default(),
        }
    }

    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    pub(crate) fn new_player(&self) -> Box<dyn Player> {
        Box::new(NokamutePlayer::new(Box::new(IterativeSearch::new(self.eval, self.opts))))
    }

    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    pub(crate) fn new_player(&self) -> Box<dyn Player> {
        Box::new(match &self.strategy {
            PlayerStrategy::Random => {
                NokamutePlayer::new_with_name("random", Box::new(minimax::Random::default()))
            }
            PlayerStrategy::Mcts(opts) => {
                let mut opts = opts.clone();
                let num_threads = self.num_threads.unwrap_or(0);
                if num_threads > 0 {
                    opts = opts.with_num_threads(num_threads);
                }
                NokamutePlayer::new(Box::new(MonteCarloTreeSearch::new(opts)))
            }
            PlayerStrategy::Iterative(ybw_opts) => {
                let mut ybw_opts = *ybw_opts;
                let num_threads = self.num_threads.unwrap_or(0);
                if num_threads > 0 {
                    ybw_opts = ybw_opts.with_num_threads(num_threads);
                }
                NokamutePlayer::new(if num_threads == 1 {
                    Box::new(IterativeSearch::new(self.eval, self.opts))
                } else {
                    Box::new(ParallelYbw::new(self.eval, self.opts, ybw_opts))
                })
            }
            PlayerStrategy::LazySmp(smp_opts) => {
                let mut smp_opts = *smp_opts;
                let num_threads = self.num_threads.unwrap_or(1);
                if num_threads > 0 {
                    smp_opts = smp_opts.with_num_threads(num_threads);
                }
                NokamutePlayer::new(Box::new(LazySmp::new(self.eval, self.opts, smp_opts)))
            }
        })
    }
}
