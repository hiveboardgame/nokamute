use crate::{PlayerConfig, UhpServer};
use std::io::Cursor;
use wasm_bindgen::prelude::*;

static mut UHP_SERVER: *mut UhpServer<Cursor<Vec<u8>>> = std::ptr::null_mut();

#[wasm_bindgen]
pub fn uhp(args: &str) -> String {
    // Manual lazy_static.
    let server = unsafe {
        if UHP_SERVER.is_null() {
            let mut config = PlayerConfig::new();
            config.opts = config.opts.with_table_byte_size(8 << 20);
            UHP_SERVER = Box::into_raw(Box::new(UhpServer::new(config, Cursor::new(Vec::new()))));
        }
        UHP_SERVER.as_mut().unwrap()
    };
    server.swap_output(Cursor::new(Vec::new()));
    server.command(args);
    let buf = server.swap_output(Cursor::new(Vec::new()));
    String::from_utf8(buf.into_inner())
        .unwrap_or_else(|_| "err encoding".to_string())
        .trim()
        .to_string()
}

#[cfg(test)]
pub mod test {
    use super::uhp;
    use wasm_bindgen_test::*;
    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    fn info_test() {
        let info = uhp("info");
        assert!(info.contains("nokamute"));
    }

    #[wasm_bindgen_test]
    fn valid_moves_test() {
        uhp("newgame Base");
        let out = uhp("validmoves");
        let mut moves = out.split(";").collect::<Vec<&str>>();
        moves.sort();
        assert_eq!(moves, &["wA1", "wB1", "wG1", "wS1"]);
    }

    #[wasm_bindgen_test]
    fn play_test() {
        uhp("newgame Base");
        uhp("play wA1");
        uhp("play bB1 -wA1");
        let state = uhp("play wQ wA1-");
        assert_eq!(state, "Base;InProgress;Black[2];wA1;bB1 -wA1;wQ wA1-");
    }

    #[wasm_bindgen_test]
    fn bestmove_depth_test() {
        uhp("newgame Base");
        let best = uhp("bestmove depth 1");
        assert!(["wA1", "wB1", "wG1", "wS1"].contains(&best.as_str()));
    }
}
